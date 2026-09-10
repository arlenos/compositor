#!/usr/bin/env bash
# Does the compositor actually come up and put a frame on a screen?
#
# Every CI job in this repo compiles the compositor. A compositor that builds,
# passes clippy and never starts is green on all of them, and it takes the only
# visual channel this project has with it. This is the check that fails on that.
#
# It asserts three things, in order, because each one fails differently:
#
#   1. the process starts and stays up          -> a panic on boot fails here
#   2. it advertises a Wayland socket           -> a backend that cannot bind fails here
#   3. a real client connects and its pixels    -> a compositor that runs but renders
#      reach a capture of the output               nothing fails here
#
# It deliberately does NOT compare the frame to a baseline. This job exists to
# catch "it does not start", not to review the picture; a pixel baseline in CI
# would fail on font and driver differences and be switched off within a month.
# What it checks is that the capture is not a single flat colour, which is what
# an output with nothing on it looks like.
#
# WHY Xvfb WORKS, since the sibling scripts say it does not. The X11 backend
# needs DRI3 and fails under Xvfb, but cosmic-comp falls back to winit, and
# winit's X11 path binds EGL through Mesa's software rasteriser. Measured on
# 8 Sep: the fallback comes up on a Wayland socket and renders. The nested-sway
# setup in `session-lock-conformance.sh` exists because those harnesses need a
# host the *lock* protocol can drive, not because Xvfb cannot host at all.
#
# Usage: dev/smoke.sh [out.png]
# Env:   SMOKE_DISPLAY (default :99), SMOKE_TIMEOUT seconds to come up (default 30)
set -euo pipefail

CP="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-$(mktemp -d)/smoke.png}"
DISP="${SMOKE_DISPLAY:-:99}"
TIMEOUT="${SMOKE_TIMEOUT:-30}"
BIN="$CP/target/debug/cosmic-comp"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
mkdir -p "$(dirname "$OUT")" "$XDG_RUNTIME_DIR"

[ -x "$BIN" ] || { echo "FAIL: no compositor at $BIN - build it first" >&2; exit 2; }
for tool in Xvfb grim; do
  command -v "$tool" >/dev/null || { echo "FAIL: $tool is not installed" >&2; exit 2; }
done

# grim is refused while the sensing master switch is off, so the switch is set
# here rather than inherited: a smoke test that depends on the host's switch
# fails for a reason that has nothing to do with the compositor.
CFG="$(mktemp -d)"
mkdir -p "$CFG/arlen"
printf 'screen_capture = true\n' > "$CFG/arlen/sensing.toml"
export XDG_CONFIG_HOME="$CFG"

LOG="${SMOKE_LOG:-$(mktemp)}"
cleanup() {
  kill "${CLIENT_PID:-}" "${CC_PID:-}" "${XVFB_PID:-}" 2>/dev/null || true
  wait 2>/dev/null || true
  rm -rf "$CFG"
  [ -n "${KEEP_LOG:-}${SMOKE_LOG:-}" ] || rm -f "$LOG"
}

# What went wrong, rather than the last 30 lines of whatever was printed.
#
# The first version of this tailed the log blindly. A compositor panic prints a
# backtrace far longer than that, so the run on 10 September reported a failure
# whose entire diagnostic was the bottom of a stack trace - `__libc_start_main`,
# `_start` - with the panic message itself scrolled off. A failure report that
# omits the reason costs a whole CI round trip to recover.
diagnose() {
  echo "--- compositor log ---" >&2
  if grep -q "panicked at" "$LOG"; then
    echo "PANIC:" >&2
    grep -A4 "panicked at" "$LOG" | head -12 >&2
    echo "..." >&2
  fi
  grep -iE "ERROR|WARN|refus" "$LOG" | tail -12 >&2
  echo "--- last lines ---" >&2
  tail -12 "$LOG" >&2
}
trap cleanup EXIT

rm -f "/tmp/.X${DISP#:}-lock"
Xvfb "$DISP" -screen 0 1920x1080x24 >/dev/null 2>&1 &
XVFB_PID=$!
for _ in $(seq 1 20); do
  DISPLAY="$DISP" xdpyinfo >/dev/null 2>&1 && break
  sleep 0.5
done

# WAYLAND_DISPLAY is unset, not merely overridden: winit prefers Wayland
# whenever it is set, which on a developer machine nests into the real session
# instead of the Xvfb this script just started.
env -u WAYLAND_DISPLAY DISPLAY="$DISP" "$BIN" > "$LOG" 2>&1 &
CC_PID=$!

WL=""
for _ in $(seq 1 $((TIMEOUT * 2))); do
  if ! kill -0 "$CC_PID" 2>/dev/null; then
    echo "FAIL: the compositor exited during startup." >&2
    diagnose
    exit 1
  fi
  WL="$(grep -oE 'wayland-[0-9]+' "$LOG" | head -1 || true)"
  [ -n "$WL" ] && [ -S "$XDG_RUNTIME_DIR/$WL" ] && break
  WL=""
  sleep 0.5
done
if [ -z "$WL" ]; then
  echo "FAIL: no Wayland socket after ${TIMEOUT}s." >&2
  diagnose
  exit 1
fi
echo "ok: compositor is up on $WL"

# A client with pixels of its own, so the capture can tell "the compositor
# rendered something" from "the compositor rendered its own empty background".
PIXELS=""
if command -v kitty >/dev/null; then
  WAYLAND_DISPLAY="$WL" DISPLAY="" kitty --title smoke \
    -o background=#c81e78 -o font_size=40 sh -c 'echo SMOKE; sleep 300' \
    >/dev/null 2>&1 &
  CLIENT_PID=$!
  PIXELS=1
elif [ -x "$CP/target/debug/test-client" ]; then
  # The in-repo client paints one pixel, which is enough to prove the
  # compositor accepts a connection and keeps running but not enough to show up
  # in a colour count - so the colour assertion below is skipped for it.
  WAYLAND_DISPLAY="$WL" DISPLAY="" "$CP/target/debug/test-client" >/dev/null 2>&1 &
  CLIENT_PID=$!
else
  echo "note: no client available; checking the compositor's own frame only" >&2
fi
sleep 5

if ! WAYLAND_DISPLAY="$WL" grim "$OUT" 2>/dev/null; then
  echo "FAIL: the compositor is up but produced no frame to capture." >&2
  diagnose
  exit 1
fi
echo "ok: captured a frame to $OUT"

if ! kill -0 "$CC_PID" 2>/dev/null; then
  echo "FAIL: the compositor died while a client was connected." >&2
  diagnose
  exit 1
fi

if command -v magick >/dev/null && [ -n "$PIXELS" ]; then
  colors="$(magick "$OUT" -format %k info: 2>/dev/null || echo 0)"
  echo "capture has $colors distinct colours"
  if [ "$colors" -le 1 ]; then
    echo "FAIL: the frame is a single flat colour - the client never reached the screen." >&2
    diagnose
    exit 1
  fi
fi

# What this run did NOT check, so a green line is not read as more than it is:
# nothing here reviews the picture, exercises input, or touches the KMS backend,
# and without kitty it does not check that a client's pixels reached the frame.
echo "PASS: the compositor started, served a client and rendered a frame"

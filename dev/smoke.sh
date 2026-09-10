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
#   3. a client connects and is given frame     -> a compositor that runs but renders
#      callbacks                                    nothing fails here
#
# It deliberately does NOT compare pixels to a baseline. This job exists to
# catch "it does not start", not to review the picture; a pixel baseline in CI
# would fail on font and driver differences and be switched off within a month.
# It does not even require a screenshot: see the frame-callback comment below
# for why a capture is the wrong assertion on a headless runner.
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
command -v Xvfb >/dev/null || { echo "FAIL: Xvfb is not installed" >&2; exit 2; }
command -v grim >/dev/null || echo "note: grim absent, no frame will be captured" >&2

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

# DID IT RENDER? A frame callback answers that, and a screenshot does not
# have to. The compositor sends a client its frame callback once the frame
# carrying it has been submitted, so a client that gets one has proof the
# render path ran end to end - the same signal `crate::presented` uses to
# decide a window has been on screen.
#
# This used to assert on a `grim` capture instead, which failed on a GitHub
# runner for a reason that has nothing to do with the compositor working: with
# no DRI3 device, EGL device binding fails (`Unable to initialize bind
# display`, then `BAD_SURFACE` from `eglQuerySurface`) and the screencopy path
# has nothing to hand over. The compositor was up and rendering the whole time.
# So the capture is now best-effort evidence and the frame callback is the
# assertion.
if [ ! -x "$CP/target/debug/wallpaper-probe" ]; then
  echo "FAIL: no wallpaper-probe at $CP/target/debug/wallpaper-probe" >&2
  echo "  build it: cargo build --bin wallpaper-probe --features test-client" >&2
  exit 2
fi

PROBE_OUT="$(mktemp)"
WAYLAND_DISPLAY="$WL" DISPLAY="" "$CP/target/debug/wallpaper-probe" 6 > "$PROBE_OUT" 2>&1 &
CLIENT_PID=$!
wait "$CLIENT_PID" 2>/dev/null || true
CLIENT_PID=""

FRAMES="$(sed -n 's/.*total frames granted: \([0-9]*\).*/\1/p' "$PROBE_OUT" | tail -1)"
FRAMES="${FRAMES:-0}"
if [ "$FRAMES" -eq 0 ]; then
  echo "FAIL: a client connected and was never given a frame - nothing was rendered." >&2
  echo "--- probe output ---" >&2
  cat "$PROBE_OUT" >&2
  rm -f "$PROBE_OUT"
  diagnose
  exit 1
fi
echo "ok: the compositor rendered $FRAMES frames for a connected client"
rm -f "$PROBE_OUT"

if ! kill -0 "$CC_PID" 2>/dev/null; then
  echo "FAIL: the compositor died while a client was connected." >&2
  diagnose
  exit 1
fi

# Best-effort picture for the artifact. A runner whose EGL cannot bind a device
# has no screencopy, and that is not this job's business to fail on.
if WAYLAND_DISPLAY="$WL" grim "$OUT" 2>/dev/null; then
  echo "ok: captured a frame to $OUT"
else
  echo "note: no screencopy on this host; the frame-callback check stands alone" >&2
fi

# What this run did NOT check, so a green line is not read as more than it is:
# nothing here reviews the picture, exercises input, or touches the KMS backend,
# and a frame callback proves a frame was submitted, not that it looked right.
echo "PASS: the compositor started, served a client and rendered a frame"

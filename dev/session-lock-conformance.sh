#!/usr/bin/env bash
# Drive `ext-session-lock-v1` against a real running compositor and look at what
# reaches the screen.
#
# The lock screen's guarantees belong to the compositor, not to the lock client,
# so they can only be checked from outside - by a client that behaves the way a
# broken or hostile lock client would. `src/bin/lock-probe.rs` is that client;
# this script gives it a compositor to talk to and photographs the result.
#
# WHY A HEADLESS SWAY AND NOT Xvfb. cosmic-comp needs a DRM-capable host: its
# X11 backend wants DRI3 and its winit fallback wants `EGL_EXT_device_drm`, and
# Xvfb has neither, so every run there dies before it opens a Wayland socket. A
# real X server does satisfy it - but nesting on the live session puts a window
# in front of whoever is working. Headless sway is both: a real Wayland host
# with a real GPU, and nothing visible anywhere.
#
# WHAT IT CANNOT SEE. The nesting backend gives the compositor exactly one
# output (`WinitState`: "no notion of multiple windows"), so the multi-monitor
# case cannot be exercised here however many heads the host has. Anything about
# two outputs is read from the code, not measured by this script.
#
# Usage:
#   dev/session-lock-conformance.sh <name>:<probe-mode>[:<hold-seconds>] ...
#
#   probe modes: no-surface, surface, crash, unlock (see src/bin/lock-probe.rs)
#
# Env:
#   OUT      where to write the captures (default: a fresh temp dir, reported)
#   TAG      prefix for the capture filenames (default: run)
#
# Requirements: sway, grim, imagemagick, kitty, and a built lock-probe
# (`cargo build --bin lock-probe --features test-client`).
set -euo pipefail

CP="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${OUT:-$(mktemp -d)}"
mkdir -p "$OUT"
TAG="${TAG:-run}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

for bin in "$CP/target/debug/cosmic-comp" "$CP/target/debug/lock-probe"; do
  [ -x "$bin" ] || { echo "missing $bin - build it first" >&2; exit 1; }
done

# The sensing master switch, stated rather than assumed: the compositor refuses
# every capture while it is off, so grim would otherwise depend on whatever
# switch the machine happens to carry. Written into a private config dir so it
# can never touch the user's own.
SENSING_HOME="$(mktemp -d)"
mkdir -p "$SENSING_HOME/arlen"
printf 'screen_capture = true\n' > "$SENSING_HOME/arlen/sensing.toml"
export XDG_CONFIG_HOME="$SENSING_HOME"

SWAYDIR="$(mktemp -d)"
printf 'output HEADLESS-1 mode 1920x1080\n' > "$SWAYDIR/config"

cleanup() {
  kill "${CLIENT_PID:-}" "${CC_PID:-}" "${SWAY_PID:-}" 2>/dev/null || true
  wait 2>/dev/null || true
  rm -rf "$SENSING_HOME" "$SWAYDIR"
}
trap cleanup EXIT

env -u WAYLAND_DISPLAY -u DISPLAY WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
  sway -c "$SWAYDIR/config" > "$SWAYDIR/sway.log" 2>&1 &
SWAY_PID=$!

HOST=""
for _ in $(seq 1 40); do
  if [ -n "$(ls -t "/run/user/$(id -u)/sway-ipc."*".$SWAY_PID.sock" 2>/dev/null | head -1 || true)" ]; then
    child="$(pgrep -P "$SWAY_PID" -n 2>/dev/null || true)"
    if [ -n "$child" ]; then
      HOST="$(tr '\0' '\n' < "/proc/$child/environ" 2>/dev/null | grep -m1 '^WAYLAND_DISPLAY=' | cut -d= -f2 || true)"
    fi
    [ -n "$HOST" ] && break
  fi
  sleep 0.5
done
[ -n "$HOST" ] || { echo "headless sway did not start"; cat "$SWAYDIR/sway.log"; exit 1; }

LOG="$OUT/comp-$TAG.log"
env -u DISPLAY WAYLAND_DISPLAY="$HOST" "$CP/target/debug/cosmic-comp" > "$LOG" 2>&1 &
CC_PID=$!
WL=""
for _ in $(seq 1 60); do
  WL="$(grep -oE 'wayland-[0-9]+' "$LOG" | grep -v "^$HOST\$" | head -1 || true)"
  [ -n "$WL" ] && [ -S "$XDG_RUNTIME_DIR/$WL" ] && break
  sleep 0.5
done
[ -n "$WL" ] || { echo "cosmic-comp did not start"; tail -20 "$LOG"; exit 1; }
export WAYLAND_DISPLAY="$WL"
echo "cosmic-comp on $WL inside headless sway on $HOST; captures in $OUT"

# Something with recognisable pixels, so a capture can tell "the lock hid the
# desktop" from "the desktop was empty anyway".
DISPLAY="" kitty --title lock-victim -o background=#c81e78 -o font_size=40 \
  sh -c 'echo SECRET-DESKTOP-CONTENT; sleep 600' >/dev/null 2>&1 &
CLIENT_PID=$!
sleep 5
grim "$OUT/$TAG-0-desktop.png"
magick "$OUT/$TAG-0-desktop.png" -format "desktop: colors=%k mean=%[fx:int(255*mean)]\n" info:

failures=0
step() {
  local name="$1"; shift
  echo "=== $name ==="
  local rc=0
  "$CP/target/debug/lock-probe" "$@" 2>&1 | sed "s/^/  /" || rc=$?
  # Exit 1 is the probe's own verdict and counts. The `crash` mode ends in
  # SIGABRT on purpose - dying without unlocking is the thing being tested - so
  # its exit code says nothing; what answers for that step is the capture below
  # and whether the compositor is still alive at the end.
  if [ "$rc" -eq 1 ]; then
    failures=$((failures + 1))
  fi
  sleep 2
  grim "$OUT/$TAG-$name.png" || { echo "  grim failed"; return 0; }
  # A locked screen is one flat opaque colour, whether that is the lock
  # surface or the compositor's own blank; the desktop capture above is the
  # control that says what "not locked" looks like.
  magick "$OUT/$TAG-$name.png" -format "  screen: colors=%k mean=%[fx:int(255*mean)]\n" info:
}

for spec in "$@"; do
  IFS=: read -r name mode hold <<< "$spec"
  step "$name" "$mode" "${hold:-4}"
done

if kill -0 "$CC_PID" 2>/dev/null; then
  echo "compositor: ALIVE"
else
  echo "compositor: DEAD - it did not survive the run"
  failures=$((failures + 1))
fi

[ "$failures" -eq 0 ] || { echo "$failures step(s) failed"; exit 1; }
echo "all steps passed"

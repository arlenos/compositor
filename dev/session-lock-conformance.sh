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
#                plus `input-leak`, which is this script's own: it locks with a
#                well-behaved lock client and then drives a pointer and a
#                keyboard at an ordinary window to check nothing gets through.
#
# Env:
#   OUT      where to write the captures (default: a fresh temp dir, reported)
#   TAG      prefix for the capture filenames (default: run)
#
# Requirements: sway, grim, imagemagick, kitty, and a built lock-probe
# (`cargo build --bin lock-probe --features test-client`). The `input-leak`
# mode additionally needs `wtype` and a built pointer-driver.
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
  exec 4>&- 2>/dev/null || true
  kill "${DRIVER_PID:-}" "${TYPIST_PID:-}" "${CLIENT_PID:-}" "${CC_PID:-}" "${SWAY_PID:-}" 2>/dev/null || true
  wait 2>/dev/null || true
  rm -rf "$SENSING_HOME" "$SWAYDIR" "${TYPED:-}"
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

# A second ordinary window that writes down everything it is typed at. Nothing
# it records after the lock should exist; `cat` is line-buffered, hence the
# Return after every word.
TYPED="$(mktemp)"
DISPLAY="" kitty --title lock-typist -o background=#1e78c8 \
  sh -c "cat > $TYPED" >/dev/null 2>&1 &
TYPIST_PID=$!
sleep 4
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

# Does anything reach an ordinary client while the session is locked? The lock
# screen has exactly one job and this is it. Keyboard goes in over
# zwp_virtual_keyboard_v1 (`wtype`) and the pointer over zwlr_virtual_pointer_v1
# into the HOST (`pointer-driver`), because the compositor under test has no
# input devices of its own - see dev/pointer-input-check.sh for why the obvious
# routes do not work.
input_leak_step() {
  local hold="${1:-14}"
  echo "=== input-leak ==="
  command -v wtype >/dev/null || { echo "  SKIP: wtype is not installed"; return 0; }
  [ -x "$CP/target/debug/pointer-driver" ] || {
    echo "  SKIP: no pointer-driver (cargo build --bin pointer-driver --features test-client)"
    return 0
  }

  local pipe driver_log probe_log before
  pipe="$(mktemp -u)"; mkfifo "$pipe"
  driver_log="$(mktemp)"; probe_log="$(mktemp)"
  WAYLAND_DISPLAY="$HOST" "$CP/target/debug/pointer-driver" < "$pipe" > "$driver_log" 2>&1 &
  local driver_pid=$!
  DRIVER_PID="$driver_pid"
  exec 4>"$pipe"
  for _ in $(seq 1 20); do grep -q "^ready" "$driver_log" && break; sleep 0.5; done

  # A control first: with the session unlocked, the same injection must land.
  printf 'move 900 500\nsleep 200\nclick\nsleep 200\n' >&4
  sleep 1
  wtype UNLOCKED >/dev/null 2>&1; sleep 0.4; wtype -k Return >/dev/null 2>&1; sleep 1
  if ! grep -q UNLOCKED "$TYPED"; then
    echo "  FAIL: the control did not land - injection is not reaching clients at all,"
    echo "        so this step cannot say anything about the locked case."
    exec 4>&-; kill "$driver_pid" 2>/dev/null; rm -f "$pipe" "$driver_log" "$probe_log"
    failures=$((failures + 1))
    return 0
  fi
  echo "  ok: with the session unlocked, typing reaches the window"

  "$CP/target/debug/lock-probe" surface "$hold" > "$probe_log" 2>&1 &
  local probe_pid=$!
  for _ in $(seq 1 40); do grep -q " locked " "$probe_log" && break; sleep 0.25; done
  if ! grep -q " locked " "$probe_log"; then
    echo "  FAIL: the session never locked, so nothing here was tested"
    sed "s/^/    /" "$probe_log"
    exec 4>&-; kill "$driver_pid" "$probe_pid" 2>/dev/null; rm -f "$pipe" "$driver_log" "$probe_log"
    failures=$((failures + 1))
    return 0
  fi

  before="$(tr -d '\r\n' < "$TYPED")"
  printf 'move 900 500\nsleep 200\nclick\nsleep 200\n' >&4
  sleep 1
  wtype LEAKED >/dev/null 2>&1; sleep 0.4; wtype -k Return >/dev/null 2>&1; sleep 1
  printf 'move 300 300\nsleep 200\nclick\nsleep 200\n' >&4
  sleep 1
  wtype ALSOLEAKED >/dev/null 2>&1; sleep 0.4; wtype -k Return >/dev/null 2>&1; sleep 1.5

  local after; after="$(tr -d '\r\n' < "$TYPED")"
  if [ "$after" != "$before" ]; then
    echo "  FAIL: input reached a client while the session was locked."
    echo "    before: [$before]"
    echo "    after : [$after]"
    failures=$((failures + 1))
  else
    echo "  ok: nothing reached the client while locked"
  fi
  grim "$OUT/$TAG-input-leak.png" 2>/dev/null \
    && magick "$OUT/$TAG-input-leak.png" -format "  screen: colors=%k mean=%[fx:int(255*mean)]\n" info:

  exec 4>&-
  kill "$driver_pid" 2>/dev/null
  wait "$probe_pid" 2>/dev/null
  rm -f "$pipe" "$driver_log" "$probe_log"
  sleep 2
}

for spec in "$@"; do
  IFS=: read -r name mode hold <<< "$spec"
  if [ "$mode" = input-leak ]; then
    input_leak_step "${hold:-14}"
  else
    step "$name" "$mode" "${hold:-4}"
  fi
done

if kill -0 "$CC_PID" 2>/dev/null; then
  echo "compositor: ALIVE"
else
  echo "compositor: DEAD - it did not survive the run"
  failures=$((failures + 1))
fi

[ "$failures" -eq 0 ] || { echo "$failures step(s) failed"; exit 1; }
echo "all steps passed"

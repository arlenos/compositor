#!/usr/bin/env bash
# Can the compositor be clicked, and does it survive being clicked?
#
# Nothing in this repo used to drive a pointer, so every input path went
# untested and one of them was frozen solid: a read guard held across a write
# guard on the shell lock deadlocked the main thread on the first absolute
# motion event the compositor ever saw. Absolute motion is what nested sessions
# run on, so every nested session froze the moment the mouse moved, and no test
# noticed because no test moved a mouse. This is that test.
#
# It asserts three things:
#
#   1. a click reaches the compositor and it decides a focus target
#   2. a fresh client still gets frames afterwards
#   3. the keyboard follows the click, to the window that was clicked
#
# The second is the deadlock guard and is the reason this script exists: a
# wedged compositor answers (1) for the first event and nothing after it. The
# third is issue #43 - "clicking a window gives pointer focus but not keyboard
# focus" - which does not reproduce on today's code, and this is what keeps it
# that way. It is checked by typing into two terminals that each write what they
# receive to a file, so "which window has the keyboard" is a fact on disk rather
# than a judgement about a picture.
#
# WHY A NESTED SWAY AND A VIRTUAL POINTER. Injecting input is harder than it
# looks. `ydotool` writes to /dev/uinput, so the event lands on whichever
# compositor owns the real seat - the developer's own session, not the one under
# test. A headless sway host has no input devices at all, so `swaymsg seat -
# cursor press` and `wlrctl pointer click` both report success to the host and
# deliver nothing: with no pointer on the seat, the nested compositor never
# binds one. Measured 14 Sep: 864 moves and 25 clicks that way, zero events
# inside. `pointer-driver` is what closes that gap - a zwlr_virtual_pointer_v1
# client that HOLDS its device open, which gives the host seat a pointer for as
# long as the test needs one.
#
# Usage: dev/pointer-input-check.sh
set -uo pipefail
CP="$(cd "$(dirname "$0")/.." && pwd)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

BIN="$CP/target/debug/cosmic-comp"
DRIVER="$CP/target/debug/pointer-driver"
PROBE="$CP/target/debug/wallpaper-probe"
[ -x "$BIN" ] || { echo "FAIL: no compositor at $BIN - build it first" >&2; exit 2; }
for b in "$DRIVER" "$PROBE"; do
  [ -x "$b" ] || { echo "FAIL: no $(basename "$b") - cargo build --bin $(basename "$b") --features test-client" >&2; exit 2; }
done
command -v sway >/dev/null || { echo "FAIL: sway is not installed" >&2; exit 2; }
command -v kitty >/dev/null || { echo "FAIL: kitty is not installed (the click target)" >&2; exit 2; }
command -v wtype >/dev/null || { echo "FAIL: wtype is not installed (it types the keyboard-focus check)" >&2; exit 2; }

CFG="$(mktemp -d)"; mkdir -p "$CFG/arlen"
printf 'screen_capture = true\n' > "$CFG/arlen/sensing.toml"
export XDG_CONFIG_HOME="$CFG"
# Side by side, so there are two windows to click BETWEEN. autotile is runtime
# state rather than a compositor.toml key, so this is the only way to ask for it
# without driving a keybinding.
ST="$(mktemp -d)"; mkdir -p "$ST/arlen/compositor"
printf 'autotile = true\n' > "$ST/arlen/compositor/state.toml"
export XDG_STATE_HOME="$ST"
SWAYDIR="$(mktemp -d)"
# No border, so a host coordinate is the same coordinate inside. No Xwayland
# either: the host has no X clients and a runner without the binary fails to
# start it, which is noise in the log of a test about pointers.
printf 'output HEADLESS-1 mode 1920x1080\ndefault_border none\nxwayland disable\n' > "$SWAYDIR/config"
LOG="$(mktemp)"
TYPED_L="$(mktemp)"; TYPED_R="$(mktemp)"

cleanup() {
  exec 3>&- 2>/dev/null
  kill ${DRIVER_PID:-} ${LEFT_PID:-} ${RIGHT_PID:-} ${CC_PID:-} ${SWAY_PID:-} 2>/dev/null
  wait 2>/dev/null
  rm -rf "$CFG" "$ST" "$SWAYDIR" "${PIPE:-}" "$TYPED_L" "$TYPED_R"
  [ -n "${KEEP_LOG:-}" ] || rm -f "$LOG"
}
trap cleanup EXIT

# WLR_RENDERER=pixman because a CI runner has no DRM render node and wlroots
# refuses to start without one otherwise ("Failed to find any DRM render node").
# The host only has to composite one client here, so software is plenty; the
# compositor under test still renders through its own EGL.
env -u WAYLAND_DISPLAY -u DISPLAY WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
  WLR_RENDERER="${WLR_RENDERER:-pixman}" \
  sway -c "$SWAYDIR/config" > "$SWAYDIR/sway.log" 2>&1 &
SWAY_PID=$!
HOST=""
for _ in $(seq 1 40); do
  child="$(pgrep -P "$SWAY_PID" -n 2>/dev/null)"
  [ -n "$child" ] && HOST="$(tr '\0' '\n' < "/proc/$child/environ" 2>/dev/null | grep -m1 '^WAYLAND_DISPLAY=' | cut -d= -f2)"
  [ -n "$HOST" ] && break
  sleep 0.5
done
[ -n "$HOST" ] || { echo "FAIL: the headless sway host never came up" >&2; tail -5 "$SWAYDIR/sway.log" >&2; exit 1; }

env -u DISPLAY WAYLAND_DISPLAY="$HOST" "$BIN" > "$LOG" 2>&1 &
CC_PID=$!
WL=""
for _ in $(seq 1 60); do
  if ! kill -0 "$CC_PID" 2>/dev/null; then
    echo "FAIL: the compositor exited during startup." >&2
    grep -A4 "panicked at" "$LOG" | head -12 >&2
    tail -8 "$LOG" >&2
    exit 1
  fi
  WL="$(grep -oE 'wayland-[0-9]+' "$LOG" | grep -v "^$HOST\$" | head -1)"
  [ -n "$WL" ] && [ -S "$XDG_RUNTIME_DIR/$WL" ] && break
  WL=""
  sleep 0.5
done
[ -n "$WL" ] || { echo "FAIL: the compositor never advertised a socket." >&2; tail -8 "$LOG" >&2; exit 1; }
echo "ok: compositor is up on $WL, nested in $HOST"

# Two terminals, each writing what it is given to its own file. `cat` is
# line-buffered, which is why the typing below always ends in Return.
WAYLAND_DISPLAY="$WL" DISPLAY="" kitty --title click-target-left \
  -o background=#c81e78 sh -c "cat > $TYPED_L" >/dev/null 2>&1 &
LEFT_PID=$!
sleep 5
WAYLAND_DISPLAY="$WL" DISPLAY="" kitty --title click-target-right \
  -o background=#1e78c8 sh -c "cat > $TYPED_R" >/dev/null 2>&1 &
RIGHT_PID=$!
for _ in $(seq 1 40); do
  [ "$(grep -c "reached the screen for the first time" "$LOG")" -ge 2 ] && break
  sleep 0.5
done
[ "$(grep -c "reached the screen for the first time" "$LOG")" -ge 2 ] || {
  echo "FAIL: the two click targets never both reached the screen." >&2; tail -8 "$LOG" >&2; exit 1; }
echo "ok: two windows are on screen to click between"

PIPE="$(mktemp -u)"; mkfifo "$PIPE"
WAYLAND_DISPLAY="$HOST" "$DRIVER" < "$PIPE" > "$SWAYDIR/driver.log" 2>&1 &
DRIVER_PID=$!
exec 3>"$PIPE"
for _ in $(seq 1 20); do grep -q "^ready" "$SWAYDIR/driver.log" && break; sleep 0.5; done
grep -q "^ready" "$SWAYDIR/driver.log" || { echo "FAIL: the virtual pointer never came up." >&2; cat "$SWAYDIR/driver.log" >&2; exit 1; }

BEFORE="$(grep -c "set_focus:" "$LOG")"
# A short walk across the middle of the output, clicking as it goes: the window
# is somewhere in there and this does not need to know where.
for i in $(seq 1 12); do
  printf 'move %d %d\nsleep 150\nclick\nsleep 150\n' "$((160 + i * 120))" "$((120 + i * 60))" >&3
done
sleep 6
AFTER="$(grep -c "set_focus:" "$LOG")"

if [ "$AFTER" -le "$BEFORE" ]; then
  echo "FAIL: 12 clicks and the compositor decided no focus target - input is not arriving." >&2
  echo "--- driver ---" >&2; tail -4 "$SWAYDIR/driver.log" >&2
  echo "--- compositor ---" >&2; tail -8 "$LOG" >&2
  exit 1
fi
echo "ok: clicks reached the compositor ($((AFTER - BEFORE)) focus decisions)"

# THE DEADLOCK GUARD. A wedged main thread still holds its Wayland socket open,
# so "the process is alive" proves nothing. Serving a client that connected
# after the clicks does.
PROBE_OUT="$(mktemp)"
timeout 30 env WAYLAND_DISPLAY="$WL" DISPLAY="" "$PROBE" 3 > "$PROBE_OUT" 2>&1
FRAMES="$(sed -n 's/.*total frames granted: \([0-9]*\).*/\1/p' "$PROBE_OUT" | tail -1)"
FRAMES="${FRAMES:-0}"
rm -f "$PROBE_OUT"
if [ "$FRAMES" -eq 0 ]; then
  echo "FAIL: after being clicked the compositor stopped serving clients - it is wedged." >&2
  tail -8 "$LOG" >&2
  exit 1
fi
echo "ok: still serving clients after the clicks ($FRAMES frames)"

# DOES THE KEYBOARD FOLLOW THE CLICK? Click one window, type a word only that
# window could have received, then the other. Both directions, because a focus
# path can be right one way and stuck the other.
type_into() {  # $1 = x of the window to click, $2 = the word to type
  printf 'move %d 600\nsleep 300\nclick\nsleep 300\n' "$1" >&3
  sleep 1.5
  WAYLAND_DISPLAY="$WL" DISPLAY="" wtype "$2" >/dev/null 2>&1
  sleep 0.5
  WAYLAND_DISPLAY="$WL" DISPLAY="" wtype -k Return >/dev/null 2>&1
  sleep 1.5
}
type_into 400 LEFTWINDOW
type_into 1500 RIGHTWINDOW
type_into 400 LEFTAGAIN

GOT_L="$(tr -d '\r\n' < "$TYPED_L")"
GOT_R="$(tr -d '\r\n' < "$TYPED_R")"
if [ "$GOT_L" != "LEFTWINDOWLEFTAGAIN" ] || [ "$GOT_R" != "RIGHTWINDOW" ]; then
  echo "FAIL: the keyboard did not follow the click." >&2
  echo "  left window received : [$GOT_L]   expected [LEFTWINDOWLEFTAGAIN]" >&2
  echo "  right window received: [$GOT_R]   expected [RIGHTWINDOW]" >&2
  echo "--- focus decisions ---" >&2
  grep -oE "set_focus: [^ ]+ -> [^ ]+" "$LOG" | tail -6 >&2
  exit 1
fi
echo "ok: the keyboard follows the click, both directions"

# What this does NOT check: anything about the picture, the KMS backend, or
# input from a real device - the keys here arrive over zwp_virtual_keyboard_v1,
# which joins the same seat but not the same backend.
echo "PASS: the compositor takes clicks, routes them and keeps running"

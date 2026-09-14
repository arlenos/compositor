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
# It asserts two things:
#
#   1. a click reaches the compositor and it decides a focus target
#   2. a fresh client still gets frames afterwards
#
# The second one is the deadlock guard and is the reason this script exists:
# a wedged compositor answers (1) for the first event and nothing after it.
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

CFG="$(mktemp -d)"; mkdir -p "$CFG/arlen"
printf 'screen_capture = true\n' > "$CFG/arlen/sensing.toml"
export XDG_CONFIG_HOME="$CFG"
SWAYDIR="$(mktemp -d)"
# No border, so a host coordinate is the same coordinate inside.
printf 'output HEADLESS-1 mode 1920x1080\ndefault_border none\n' > "$SWAYDIR/config"
LOG="$(mktemp)"

cleanup() {
  exec 3>&- 2>/dev/null
  kill ${DRIVER_PID:-} ${CLIENT_PID:-} ${CC_PID:-} ${SWAY_PID:-} 2>/dev/null
  wait 2>/dev/null
  rm -rf "$CFG" "$SWAYDIR" "${PIPE:-}"
  [ -n "${KEEP_LOG:-}" ] || rm -f "$LOG"
}
trap cleanup EXIT

env -u WAYLAND_DISPLAY -u DISPLAY WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
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

WAYLAND_DISPLAY="$WL" DISPLAY="" kitty --title click-target \
  -o background=#c81e78 sh -c 'sleep 600' >/dev/null 2>&1 &
CLIENT_PID=$!
for _ in $(seq 1 40); do
  grep -q "reached the screen for the first time" "$LOG" && break
  sleep 0.5
done
grep -q "reached the screen" "$LOG" || { echo "FAIL: the click target never reached the screen." >&2; tail -8 "$LOG" >&2; exit 1; }
echo "ok: a window is on screen to click on"

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

# What this does NOT check: which window got the focus, whether the keyboard
# followed the pointer, or anything about the picture. It answers "does input
# arrive and does the compositor survive it", and those are two different
# failures that both used to go unnoticed.
echo "PASS: the compositor takes clicks and keeps running"

#!/usr/bin/env bash
# Super+drag moves a window, and a drag without Super does not.
#
# This is the gesture that decides whether a click belongs to the compositor or
# to the application, and the fork has already lost the decision once: the check
# for whether Super is PHYSICALLY held - as opposed to a modifier mask the host
# handed over with no key press behind it - was added in `b9e365b8` for a real
# nested-session bug and dropped again by an unrelated commit, which left the
# computation in place and stopped using it. With the mask alone, a session that
# starts with a stale Super swallows every click into a move grab and no
# application ever sees one.
#
# So both directions are driven here:
#
#   1. Super held + drag on the window body  -> the window moves
#   2. drag on the window body, no modifier  -> the window stays put
#
# WHY Xvfb AND xdotool, when dev/pointer-input-check.sh uses a nested sway. Keys
# here have to be REAL key presses. `wtype` speaks zwp_virtual_keyboard_v1 and
# can assert a modifier without pressing anything, which is precisely the state
# check (1) exists to reject - it would fail this test for the right reason and
# prove nothing. An X server delivers an actual keycode.
#
# And the X server needs one piece of help: there is no window manager on it, so
# nothing ever gives the compositor's window the input focus and every key event
# goes to the root window instead. `xdotool windowfocus` does it by hand.
# Pointer events are routed by position and do not need it, which is why this is
# easy to miss - clicks work and keys silently do not.
#
# Usage: dev/super-drag-check.sh
set -uo pipefail
CP="$(cd "$(dirname "$0")/.." && pwd)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
BIN="$CP/target/debug/cosmic-comp"
DISP="${SUPER_DRAG_DISPLAY:-:93}"

[ -x "$BIN" ] || { echo "FAIL: no compositor at $BIN - build it first" >&2; exit 2; }
for t in Xvfb xdotool grim kitty; do
  command -v "$t" >/dev/null || { echo "FAIL: $t is not installed" >&2; exit 2; }
done
python3 -c "import PIL" 2>/dev/null || { echo "FAIL: python3 Pillow is needed to find the window" >&2; exit 2; }

CFG="$(mktemp -d)"; mkdir -p "$CFG/arlen"
printf 'screen_capture = true\n' > "$CFG/arlen/sensing.toml"
export XDG_CONFIG_HOME="$CFG"
SHOTS="$(mktemp -d)"
LOG="$(mktemp)"
cleanup() {
  kill ${CLIENT_PID:-} ${CC_PID:-} ${XVFB_PID:-} 2>/dev/null
  wait 2>/dev/null
  rm -rf "$CFG" "$SHOTS"
  [ -n "${KEEP_LOG:-}" ] || rm -f "$LOG"
}
trap cleanup EXIT

rm -f "/tmp/.X${DISP#:}-lock"
Xvfb "$DISP" -screen 0 1920x1080x24 >/dev/null 2>&1 &
XVFB_PID=$!
for _ in $(seq 1 20); do DISPLAY="$DISP" xdpyinfo >/dev/null 2>&1 && break; sleep 0.5; done

env -u WAYLAND_DISPLAY DISPLAY="$DISP" "$BIN" > "$LOG" 2>&1 &
CC_PID=$!
WL=""
for _ in $(seq 1 60); do
  if ! kill -0 "$CC_PID" 2>/dev/null; then
    echo "FAIL: the compositor exited during startup." >&2
    grep -A4 "panicked at" "$LOG" | head -12 >&2; tail -8 "$LOG" >&2; exit 1
  fi
  WL="$(grep -oE 'wayland-[0-9]+' "$LOG" | head -1)"
  [ -n "$WL" ] && [ -S "$XDG_RUNTIME_DIR/$WL" ] && break
  WL=""; sleep 0.5
done
[ -n "$WL" ] || { echo "FAIL: the compositor never advertised a socket." >&2; tail -8 "$LOG" >&2; exit 1; }

# A single window in a colour nothing else on screen has, so its rectangle can
# be read straight out of a capture.
WAYLAND_DISPLAY="$WL" DISPLAY="" kitty --title drag-me \
  -o background=#c81e78 sh -c 'sleep 600' >/dev/null 2>&1 &
CLIENT_PID=$!
for _ in $(seq 1 40); do
  grep -q "reached the screen for the first time" "$LOG" && break
  sleep 0.5
done
grep -q "reached the screen" "$LOG" || { echo "FAIL: the window never reached the screen." >&2; tail -8 "$LOG" >&2; exit 1; }
echo "ok: compositor is up on $WL with a window to drag"

export DISPLAY="$DISP"
WID="$(xdotool search --name . | tail -1)"
xdotool windowfocus "$WID" 2>/dev/null
[ -n "$WID" ] || { echo "FAIL: no X window to focus - keys would go nowhere." >&2; exit 1; }

body() {  # prints "x0 y0 x1 y1" of the window's body, or "none"
  WAYLAND_DISPLAY="$WL" grim "$1" 2>/dev/null
  python3 - "$1" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB")
px, (w, h) = im.load(), im.size
xs, ys = [], []
for y in range(0, h, 4):
    for x in range(0, w, 4):
        r, g, b = px[x, y]
        if abs(r - 200) < 40 and abs(g - 30) < 40 and abs(b - 120) < 40:
            xs.append(x); ys.append(y)
print("%d %d %d %d" % (min(xs), min(ys), max(xs), max(ys)) if xs else "none")
PY
}

drag() {  # $1 = "super" or "plain", $2..$5 = from x y to x y
  local mode="$1" fx="$2" fy="$3" tx="$4" ty="$5"
  xdotool mousemove "$fx" "$fy"; sleep 0.4
  [ "$mode" = super ] && { xdotool keydown super; sleep 0.4; }
  xdotool mousedown 1; sleep 0.4
  local i
  for i in 1 2 3 4; do
    xdotool mousemove $((fx + (tx - fx) * i / 4)) $((fy + (ty - fy) * i / 4))
    sleep 0.2
  done
  sleep 0.5
  xdotool mouseup 1; sleep 0.5
  [ "$mode" = super ] && { xdotool keyup super; sleep 0.5; }
  sleep 0.8
}

B0="$(body "$SHOTS/0.png")"
[ "$B0" != none ] || { echo "FAIL: could not find the window in a capture." >&2; exit 1; }
set -- $B0
CX=$(( ($1 + $3) / 2 )); CY=$(( ($2 + $4) / 2 ))

drag plain "$CX" "$CY" $((CX + 200)) $((CY + 100))
B1="$(body "$SHOTS/1.png")"
if [ "$B1" != "$B0" ]; then
  echo "FAIL: dragging the window body with no modifier moved it." >&2
  echo "  before: $B0" >&2
  echo "  after : $B1" >&2
  echo "That drag belongs to the application, not to the compositor." >&2
  exit 1
fi
echo "ok: a plain drag on the window body leaves the window alone"

drag super "$CX" "$CY" $((CX - 240)) $((CY - 60))
B2="$(body "$SHOTS/2.png")"
if [ "$B2" = "$B0" ]; then
  echo "FAIL: Super+drag did not move the window." >&2
  echo "  before: $B0" >&2
  echo "  after : $B2" >&2
  echo "--- did the key arrive at all? ---" >&2
  grep -c "super_tap" "$LOG" >&2
  exit 1
fi
echo "ok: Super+drag moved the window ($B0 -> $B2)"

# What this does NOT check: where the window landed, snapping, or the preview
# drawn during the drag. It answers who owns the click.
echo "PASS: the compositor takes the drag when Super is held and not otherwise"

#!/usr/bin/env bash
# Do the window-control buttons respond where they are drawn?
#
# The compositor paints the header itself - close, maximise, minimise - and
# nothing had ever clicked one. The defect that found: `PointerTarget::enter`
# did not refresh the header's hover state and `motion` did, and smithay sends
# `enter` INSTEAD of `motion` for the event that first lands on a window. A
# pointer arriving directly on the close button therefore left it `Idle`, the
# press was declined, and the click did nothing. A hand on a mouse hides it (the
# second motion event arms the button); the first movement of a session, a
# touchpad tap or a fast flick does not.
#
# So the assertion is deliberately mean: ONE jump straight onto the button and
# ONE click, with no approach.
#
# Usage: dev/header-button-check.sh
set -uo pipefail
CP="$(cd "$(dirname "$0")/.." && pwd)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
BIN="$CP/target/debug/cosmic-comp"
DISP="${HEADER_CHECK_DISPLAY:-:92}"

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

# The body colour is how the window's rectangle is found; the header is the
# strip the compositor reserves directly above it.
spawn() {
  WAYLAND_DISPLAY="$WL" DISPLAY="" kitty --title header-probe \
    -o background=#c81e78 sh -c 'sleep 600' >/dev/null 2>&1 &
  CLIENT_PID=$!
}
rect() {
  WAYLAND_DISPLAY="$WL" grim "$1" 2>/dev/null
  python3 - "$1" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB")
px, (w, h) = im.load(), im.size
xs, ys = [], []
for y in range(0, h, 2):
    for x in range(0, w, 2):
        r, g, b = px[x, y]
        if abs(r - 200) < 40 and abs(g - 30) < 40 and abs(b - 120) < 40:
            xs.append(x); ys.append(y)
print("%d %d %d %d" % (min(xs), min(ys), max(xs), max(ys)) if xs else "none")
PY
}
hover_state() {
  sed 's/\x1b\[[0-9;]*m//g' "$LOG" \
    | grep -oE 'interaction=[A-Za-z]+\([A-Za-z]+\)|interaction=Idle' | tail -1
}

spawn
for _ in $(seq 1 40); do
  grep -q "reached the screen for the first time" "$LOG" && break
  sleep 0.5
done
grep -q "reached the screen" "$LOG" || { echo "FAIL: the window never reached the screen." >&2; tail -8 "$LOG" >&2; exit 1; }
export DISPLAY="$DISP"

R="$(rect "$SHOTS/0.png")"
[ "$R" != none ] || { echo "FAIL: could not find the window in a capture." >&2; exit 1; }
set -- $R
CLOSE_X=$(( $3 - 18 ))     # the close button sits at the header's right end
HEADER_Y=$(( $2 - 18 ))    # the reserved strip above the body
MIDDLE_X=$(( ($1 + $3) / 2 ))
echo "ok: window at $R, header row y=$HEADER_Y"

# 1. A click in the middle of the header is a drag handle, not a button.
xdotool mousemove "$MIDDLE_X" "$HEADER_Y"; sleep 0.6
xdotool click 1; sleep 2
if grep -q "window.closed" "$LOG"; then
  echo "FAIL: clicking the middle of the header closed the window." >&2
  exit 1
fi
echo "ok: a click on the header's drag zone does not press a button"

# 2. ONE jump onto the close button must hover it. No approach: that is the
#    whole point - an `enter` has to arm the button like a `motion` does.
xdotool mousemove 10 10; sleep 0.6
xdotool mousemove "$CLOSE_X" "$HEADER_Y"; sleep 1
STATE="$(hover_state)"
if [ "$STATE" != "interaction=Hover(Close)" ]; then
  echo "FAIL: arriving on the close button in one motion did not hover it." >&2
  echo "  wanted interaction=Hover(Close), got ${STATE:-nothing}" >&2
  echo "  That is the enter-vs-motion defect: the click after this does nothing." >&2
  exit 1
fi
echo "ok: one motion onto the close button hovers it"

# 3. And the click that follows must actually close the window.
xdotool click 1
CLOSED=no
for _ in $(seq 1 15); do
  sleep 0.4
  grep -q "window.closed" "$LOG" && { CLOSED=yes; break; }
done
if [ "$CLOSED" != yes ]; then
  echo "FAIL: the close button was hovered and the click did not close the window." >&2
  tail -8 "$LOG" >&2
  exit 1
fi
echo "ok: the close button closes the window"

# What this does NOT check: maximise and minimise (neither has an outcome this
# harness can read without a second capture pass), the header's text, or its
# colours.
echo "PASS: the window controls respond where they are drawn"

#!/usr/bin/env bash
# Does the scratchpad park a window and bring it back, more than once?
#
# Super+Shift+Minus parks the focused window; Super+Minus shows it and, pressed
# again, hides it. Until 8 October the second press did nothing at all: the
# shown window was never focused, and "visible but not focused: focus it" was a
# comment over an empty branch. So this goes round twice.
#
# Keys are real X key events on Xvfb (see dev/super-drag-check.sh for why). The
# compositor's layout is pinned to `us` in a private compositor.toml: xdotool
# picks keycodes from the X server's US map, and a compositor that inherits a
# German system layout reads the minus key as `ß`.
#
# Usage: dev/scratchpad-check.sh
set -uo pipefail
CP="$(cd "$(dirname "$0")/.." && pwd)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
BIN="$CP/target/debug/cosmic-comp"
DISP="${SCRATCHPAD_CHECK_DISPLAY:-:68}"
[ -x "$BIN" ] || { echo "FAIL: no compositor at $BIN - build it first" >&2; exit 2; }
for t in Xvfb xdotool grim kitty; do command -v "$t" >/dev/null || { echo "FAIL: $t is not installed" >&2; exit 2; }; done

# compositor.toml is read from $HOME/.config/arlen, not $XDG_CONFIG_HOME.
H="$(mktemp -d)"; mkdir -p "$H/.config/arlen"
printf 'screen_capture = true\n' > "$H/.config/arlen/sensing.toml"
printf '[xkb_config]\nlayout = "us"\n' > "$H/.config/arlen/compositor.toml"
LOG="$(mktemp)"; SHOT="$(mktemp)"
cleanup() {
  kill ${K:-} ${CC:-} ${XV:-} 2>/dev/null; wait 2>/dev/null
  rm -rf "$H" "$SHOT"; [ -n "${KEEP_LOG:-}" ] || rm -f "$LOG"
}
trap cleanup EXIT

rm -f "/tmp/.X${DISP#:}-lock"
Xvfb "$DISP" -screen 0 1920x1080x24 >/dev/null 2>&1 & XV=$!
for _ in $(seq 1 20); do DISPLAY="$DISP" xdpyinfo >/dev/null 2>&1 && break; sleep 0.5; done
HOME="$H" XDG_CONFIG_HOME="$H/.config" env -u WAYLAND_DISPLAY DISPLAY="$DISP" "$BIN" > "$LOG" 2>&1 & CC=$!
WL=""
for _ in $(seq 1 60); do
  WL="$(grep -oE 'wayland-[0-9]+' "$LOG" | head -1)"
  [ -n "$WL" ] && [ -S "$XDG_RUNTIME_DIR/$WL" ] && break; WL=""; sleep 0.5
done
[ -n "$WL" ] || { echo "FAIL: the compositor never advertised a socket." >&2; tail -8 "$LOG" >&2; exit 1; }
WAYLAND_DISPLAY="$WL" DISPLAY="" kitty -o background=#c81e78 sh -c 'sleep 600' >/dev/null 2>&1 & K=$!
for _ in $(seq 1 30); do grep -q "reached the screen" "$LOG" && break; sleep 0.5; done
sleep 1.5
export DISPLAY="$DISP"
xdotool windowfocus "$(xdotool search --name . | tail -1)" 2>/dev/null

visible() {  # how much of the window's colour is on screen
  WAYLAND_DISPLAY="$WL" grim "$SHOT" 2>/dev/null
  python3 - "$SHOT" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB"); p = im.load(); w, h = im.size
print(sum(1 for y in range(0, h, 3) for x in range(0, w, 3)
          if abs(p[x, y][0] - 200) < 40 and abs(p[x, y][1] - 30) < 40 and abs(p[x, y][2] - 120) < 40))
PY
}
expect() {  # $1 = label, $2 = shown|hidden
  local n; n="$(visible)"
  if { [ "$2" = shown ] && [ "$n" -gt 1000 ]; } || { [ "$2" = hidden ] && [ "$n" -eq 0 ]; }; then
    echo "ok: $1 - $2"
  else
    echo "FAIL: $1 - wanted $2, $n window pixels on screen" >&2; exit 1
  fi
}

expect "before" shown
xdotool key super+shift+minus; sleep 1.5; expect "parked" hidden
for round in 1 2; do
  xdotool key super+minus; sleep 1.5; expect "round $round, shown" shown
  xdotool key super+minus; sleep 1.5; expect "round $round, hidden again" hidden
done
echo "PASS: the scratchpad parks a window and toggles it, twice round"

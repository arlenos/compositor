#!/usr/bin/env bash
# Is every corner of a decorated window drawn once, by the frame?
#
# A window with a compositor header has four outer corners: the header's two at
# the top and the client's two at the bottom. Its client's top corners are not
# corners of the window at all - the header sits on them - so they must be cut
# square, or a second arc shows under the header. Two defects of exactly this
# shape have been found by eye already:
#
#   14 Sep  the header's arc (r=12) and the frame's (r=16) did not meet
#   8 Oct   bare corner indices swapped in April: the client kept an arc at its
#           top-left under the header, and every decorated window had a square
#           bottom-left
#
# So this measures, at each radius setting given:
#
#   1. the client's top corners are square         (no second arc)
#   2. the client's bottom corners are round        (the window's corners)
#   3. the header's top-left arc has the same curve as the client's bottom-left
#      arc, mirrored - i.e. both are the frame's radius, drawn the same way
#
# Curves are compared at the sub-pixel x where coverage crosses 50%, row by row,
# which is robust to anti-aliasing and to the two edges having different
# colours behind them.
#
# Usage: dev/corner-check.sh [radius_intensity ...]     (default: 0.5 1.0 2.0)
set -uo pipefail
CP="$(cd "$(dirname "$0")/.." && pwd)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
BIN="$CP/target/debug/cosmic-comp"
DISP="${CORNER_CHECK_DISPLAY:-:71}"
[ -x "$BIN" ] || { echo "FAIL: no compositor at $BIN - build it first" >&2; exit 2; }
for t in Xvfb grim kitty; do command -v "$t" >/dev/null || { echo "FAIL: $t is not installed" >&2; exit 2; }; done
python3 -c "import PIL" 2>/dev/null || { echo "FAIL: python3 Pillow is needed" >&2; exit 2; }
INTENSITIES=("$@"); [ ${#INTENSITIES[@]} -gt 0 ] || INTENSITIES=(0.5 1.0 2.0)

SHOTS="$(mktemp -d)"
trap 'rm -rf "$SHOTS"' EXIT

shoot() {  # $1 = radius intensity, $2 = output png
  local cfg log xv cc k wl
  cfg="$(mktemp -d)"; mkdir -p "$cfg/arlen"
  printf 'screen_capture = true\n' > "$cfg/arlen/sensing.toml"
  printf '[overrides]\nradius_intensity = %s\n' "$1" > "$cfg/arlen/appearance.toml"
  log="$(mktemp)"
  rm -f "/tmp/.X${DISP#:}-lock"
  Xvfb "$DISP" -screen 0 1920x1080x24 >/dev/null 2>&1 & xv=$!
  for _ in $(seq 1 20); do DISPLAY="$DISP" xdpyinfo >/dev/null 2>&1 && break; sleep 0.5; done
  XDG_CONFIG_HOME="$cfg" env -u WAYLAND_DISPLAY DISPLAY="$DISP" "$BIN" > "$log" 2>&1 & cc=$!
  wl=""
  for _ in $(seq 1 60); do
    wl="$(grep -oE 'wayland-[0-9]+' "$log" | head -1)"
    [ -n "$wl" ] && [ -S "$XDG_RUNTIME_DIR/$wl" ] && break; wl=""; sleep 0.5
  done
  if [ -n "$wl" ]; then
    WAYLAND_DISPLAY="$wl" DISPLAY="" kitty -o background=#c81e78 sh -c 'sleep 600' >/dev/null 2>&1 & k=$!
    for _ in $(seq 1 30); do grep -q "reached the screen" "$log" && break; sleep 0.5; done
    sleep 1.5
    WAYLAND_DISPLAY="$wl" grim "$2" 2>/dev/null
    kill "$k" 2>/dev/null
  fi
  kill "$cc" "$xv" 2>/dev/null; wait "$cc" "$xv" 2>/dev/null
  rm -rf "$cfg" "$log"
}

failures=0
for i in "${INTENSITIES[@]}"; do
  shot="$SHOTS/corner-$i.png"
  shoot "$i" "$shot"
  [ -s "$shot" ] || { echo "FAIL: intensity $i - no capture" >&2; failures=$((failures + 1)); continue; }
  python3 - "$shot" "$i" <<'PY' || failures=$((failures + 1))
import sys
from PIL import Image
path, intensity = sys.argv[1], sys.argv[2]
im = Image.open(path).convert("RGB"); px = im.load(); w, h = im.size
BODY = (200, 30, 120)
near = lambda c, t, tol=40: all(abs(a - b) < tol for a, b in zip(c, t))
xs, ys = [], []
for y in range(0, h, 2):
    for x in range(0, w, 2):
        if near(px[x, y], BODY): xs.append(x); ys.append(y)
if not xs:
    print(f"FAIL: intensity {intensity} - no window in the capture"); sys.exit(1)
x0, y0, x1, y1 = min(xs), min(ys), max(xs), max(ys)
# The coarse scan above steps by 2 px; an arc comparison needs the exact edges,
# so walk them at full resolution along the window's middle row and column.
cx, cy = (x0 + x1) // 2, (y0 + y1) // 2
while x0 > 0 and near(px[x0 - 1, cy], BODY): x0 -= 1
while x0 < w and not near(px[x0, cy], BODY): x0 += 1
while y1 + 1 < h and near(px[cx, y1 + 1], BODY): y1 += 1
while y0 > 0 and near(px[cx, y0 - 1], BODY): y0 -= 1
while x1 + 1 < w and near(px[x1 + 1, cy], BODY): x1 += 1
top = y0 - 36                                    # the header's top edge

def crossing(y, bg, fg, x_from, x_to):
    ch = max(range(3), key=lambda i: abs(bg[i] - fg[i])); mid = (bg[ch] + fg[ch]) / 2; prev = None
    for x in range(x_from, x_to):
        v = px[x, y][ch]
        if prev is not None and (prev - mid) * (v - mid) <= 0 and prev != v:
            return (x - 1 + (mid - prev) / (v - prev)) - x0
        prev = v
    return None

ok = True
# 1 + 2: sample 3 px in along each corner's diagonal
def filled(x, y): return near(px[x, y], BODY)
checks = {
    "client top-left square": filled(x0 + 1, y0 + 1),
    "client top-right square": filled(x1 - 1, y0 + 1),
    "client bottom-left round": not filled(x0 + 1, y1 - 1),
    "client bottom-right round": not filled(x1 - 1, y1 - 1),
}
for name, good in checks.items():
    if not good:
        print(f"FAIL: intensity {intensity} - {name}: no"); ok = False

# 3: header top-left arc vs client bottom-left arc, mirrored
# What is behind each arc is sampled on the arc's own row: the drop shadow is
# not the same darkness at the top and the bottom of a window.
header = px[x0 + 25, top + 18]
body = px[x0 + 25, y0 + 30]
rows = 10
hdr = [crossing(top + dy, px[max(0, x0 - 20), top + dy], header, x0 - 30, x0 + 80) for dy in range(rows)]
bot = [crossing(y1 - dy, px[max(0, x0 - 20), y1 - dy], body, x0 - 30, x0 + 80) for dy in range(rows)]
pairs = [(a, b) for a, b in zip(hdr, bot) if a is not None and b is not None]
if len(pairs) < rows // 2:
    print(f"FAIL: intensity {intensity} - could not trace the arcs ({len(pairs)} rows)"); ok = False
else:
    worst = max(abs(a - b) for a, b in pairs)
    if __import__("os").environ.get("CORNER_DEBUG"): print("   hdr", [round(a,2) for a,_ in pairs]); print("   bot", [round(b,2) for _,b in pairs])
    print(f"  intensity {intensity}: header arc vs frame arc, max {worst:.2f} px over {len(pairs)} rows")
    # One pixel, not zero. The two arcs are drawn by different rasterisers - the
    # header by tiny-skia on the CPU, the client by the clipping shader - and
    # measured 8 Oct they agree to 0.42 px at intensity 0.5 and 1.0 and to
    # 0.75 px at 2.0 (r about 28), worst on the very first row. The defects this
    # exists for were 4.3 px (two radii) and a whole missing corner.
    if worst > 1.0:
        print(f"FAIL: intensity {intensity} - the header's corner is not the frame's ({worst:.2f} px)"); ok = False
if ok:
    print(f"ok: intensity {intensity} - every corner drawn once, by the frame")
sys.exit(0 if ok else 1)
PY
done
[ "$failures" -eq 0 ] || { echo "FAIL: $failures radius setting(s) failed" >&2; exit 1; }
# Not checked: a fractional output scale (winit under Xvfb always reports 1),
# stacks (they need a keybinding or a drag to create), and the shadow.
echo "PASS: decorated windows draw each corner once, at every radius tried"

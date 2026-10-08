#!/usr/bin/env bash
# Look for the overlay ghost (roadmap M0 item 6) on the real GPU, headlessly.
#
# `ghost-repro` paints a 600x600 magenta block, holds it, then unmaps it. A
# correct compositor leaves no magenta behind. This runs it in cosmic-comp nested
# in a headless sway, so the compositor renders on the machine's GPU through
# winit and nothing appears on any screen, and counts magenta pixels on the host
# twice: while the block is up (the control: the capture has to see it) and after
# the unmap (the test: none may be left).
#
# `SOFTWARE=1` runs the same with LIBGL_ALWAYS_SOFTWARE, so both renderers can be
# compared on one machine. What this cannot reach is the KMS path, where the ghost
# was seen in the VM: that needs a DRM device of its own (vkms, loaded by root).
#
# Usage: dev/ghost-gpu-check.sh [layer|toplevel ...]
# Requirements: sway, grim, imagemagick and a built cosmic-comp and ghost-repro
# (`cargo build --features test-client`).
set -uo pipefail

CP="$(cd "$(dirname "$0")/.." && pwd)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
for bin in "$CP/target/debug/cosmic-comp" "$CP/target/debug/ghost-repro"; do
  [ -x "$bin" ] || { echo "missing $bin - build it first" >&2; exit 1; }
done
[ $# -gt 0 ] || set -- layer toplevel
GL=()
[ "${SOFTWARE:-0}" = 1 ] && GL=(LIBGL_ALWAYS_SOFTWARE=1)
failures=0

# Pixels of exactly the repro's magenta in what the HOST shows. Capturing
# cosmic-comp itself would not do: its screencopy renders the scene afresh for
# the capture, and a ghost is a stale frame left on the display by damage that
# was never repainted, which a fresh render cannot contain. sway shows the frames
# cosmic-comp actually presented in its window.
magenta() {
  WAYLAND_DISPLAY="$1" grim "$2" 2>/dev/null || { echo -1; return; }
  magick "$2" -fill black +opaque "#FF00FF" -fill white -opaque "#FF00FF" \
    -format "%[fx:int(mean*w*h)]" info:
}

run_mode() {
  local mode="$1" d sw cc g host wl c
  d="$(mktemp -d)"
  printf 'output HEADLESS-1 mode 1920x1080\n' > "$d/config"
  mkdir -p "$d/cfg/arlen"; printf 'screen_capture = true\n' > "$d/cfg/arlen/sensing.toml"
  env -u WAYLAND_DISPLAY -u DISPLAY WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
    sway -c "$d/config" > "$d/sway.log" 2>&1 & sw=$!
  host=""
  for _ in $(seq 1 40); do
    c="$(pgrep -P "$sw" -n 2>/dev/null)"
    [ -n "$c" ] && host="$(tr '\0' '\n' < "/proc/$c/environ" 2>/dev/null | grep -m1 '^WAYLAND_DISPLAY=' | cut -d= -f2)"
    [ -n "$host" ] && break; sleep 0.5
  done
  env -u DISPLAY "${GL[@]}" XDG_CONFIG_HOME="$d/cfg" WAYLAND_DISPLAY="$host" \
    "$CP/target/debug/cosmic-comp" > "$d/comp.log" 2>&1 & cc=$!
  wl=""
  for _ in $(seq 1 60); do
    wl="$(grep -oE 'Listening on "wayland-[0-9]+"' "$d/comp.log" | grep -oE 'wayland-[0-9]+')"
    [ -n "$wl" ] && break; sleep 0.5
  done
  sleep 3

  echo "=== $mode ==="
  sed -E "s/\x1b\[[0-9;]*m//g" "$d/comp.log" | grep -oE 'GL Renderer: "[^"(]*' | head -1 | sed 's/^/  /'
  WAYLAND_DISPLAY="$wl" "$CP/target/debug/ghost-repro" "$mode" 3000 > "$d/repro.log" 2>&1 & g=$!
  sleep 1.8
  local during after
  during="$(magenta "$host" "$d/during.png")"
  sleep 3.5
  after="$(magenta "$host" "$d/after.png")"
  echo "  magenta while up: $during   after the unmap: $after"
  if [ "$during" -lt 300000 ]; then
    echo "  FAIL: the capture did not see the block, so a clean result proves nothing"
    failures=$((failures + 1))
  elif [ "$after" -ne 0 ]; then
    echo "  FAIL: the ghost is real here, $after pixels of the block stayed"
    failures=$((failures + 1))
  else
    echo "  ok: the block was seen and left nothing behind"
  fi
  kill "$g" "$cc" "$sw" 2>/dev/null; wait 2>/dev/null; rm -rf "$d"
}

for mode in "$@"; do run_mode "$mode"; done
[ "$failures" -eq 0 ] || { echo "$failures check(s) failed"; exit 1; }
echo "all checks passed"

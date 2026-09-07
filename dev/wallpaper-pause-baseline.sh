#!/usr/bin/env bash
# What does the compositor already grant a wallpaper that nobody can see?
#
# WP-R3 (`wallpaper-plan.md`) wants a live wallpaper that draws zero frames when
# it is covered, when an app is fullscreen, and when the session is locked, and
# it treats the `wl_surface.frame` throttle as a baseline that is not enough. The
# only way to know how much of that the fork already does is to run a client that
# behaves like a live wallpaper and count the frames it is given.
#
# Runs `wallpaper-probe` on the background layer under a nested compositor, lets
# it settle uncovered, then applies one cover and keeps reading the rate. Read
# the frames_last_second column across the cover point; the `none` case is the
# control that says the drop came from the cover and not from the compositor
# going quiet on its own.
#
# The compositor is nested in a private headless sway for the same reasons as
# `session-lock-conformance.sh` - Xvfb cannot host it, and the live session
# should not have a window thrown onto it.
#
# Usage: dev/wallpaper-pause-baseline.sh [none|maximized|fullscreen|translucent|locked]
#
# Env: OUT (capture directory, default a temp dir), SECONDS_TOTAL (probe run
# length, default 14).
#
# Requirements: sway, grim, imagemagick, kitty, and built probes
# (`cargo build --bin wallpaper-probe --bin lock-probe --features test-client`).
set -euo pipefail

CP="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${OUT:-$(mktemp -d)}"
mkdir -p "$OUT"
CASE="${1:-maximized}"
TOTAL="${SECONDS_TOTAL:-14}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

for bin in "$CP/target/debug/cosmic-comp" "$CP/target/debug/wallpaper-probe"; do
  [ -x "$bin" ] || { echo "missing $bin - build it first" >&2; exit 1; }
done
[ "$CASE" != "locked" ] || [ -x "$CP/target/debug/lock-probe" ] || {
  echo "the locked case needs target/debug/lock-probe" >&2; exit 1; }

SENSING_HOME="$(mktemp -d)"
mkdir -p "$SENSING_HOME/arlen"
printf 'screen_capture = true\n' > "$SENSING_HOME/arlen/sensing.toml"
export XDG_CONFIG_HOME="$SENSING_HOME"

SWAYDIR="$(mktemp -d)"
printf 'output HEADLESS-1 mode 1920x1080\n' > "$SWAYDIR/config"

cleanup() {
  kill "${COVER_PID:-}" "${PROBE_PID:-}" "${CC_PID:-}" "${SWAY_PID:-}" 2>/dev/null || true
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

LOG="$OUT/comp-wallpaper-$CASE.log"
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
echo "case: $CASE   cosmic-comp on $WL   captures in $OUT"

DISPLAY="" "$CP/target/debug/wallpaper-probe" "$TOTAL" 2>&1 | sed 's/^/  /' &
PROBE_PID=$!
sleep 5
echo ">>> applying cover: $CASE"
case "$CASE" in
  none)
    echo "    control: nothing covers the wallpaper, the rate must not drop"
    ;;
  maximized)
    # The real occlusion test. NOT fullscreen: `render_input_order` skips the
    # background layer outright whenever the workspace has a fullscreen window,
    # so a fullscreen cover exercises that shortcut instead of occlusion.
    DISPLAY="" kitty --start-as=maximized --title cover \
      -o background=#103080 -o background_opacity=1.0 -o font_size=40 \
      sh -c 'echo COVER; sleep 600' >/dev/null 2>&1 & COVER_PID=$!
    ;;
  fullscreen)
    DISPLAY="" kitty --start-as=fullscreen --title cover \
      -o background=#103080 -o background_opacity=1.0 -o font_size=40 \
      sh -c 'echo COVER; sleep 600' >/dev/null 2>&1 & COVER_PID=$!
    ;;
  translucent)
    # An opacity below 1 makes kitty skip its opaque region, so nothing tells
    # the compositor the wallpaper behind it cannot be seen.
    DISPLAY="" kitty --start-as=fullscreen --title cover \
      -o background=#103080 -o background_opacity=0.98 -o font_size=40 \
      sh -c 'echo COVER; sleep 600' >/dev/null 2>&1 & COVER_PID=$!
    ;;
  locked)
    DISPLAY="" "$CP/target/debug/lock-probe" surface 30 >/dev/null 2>&1 & COVER_PID=$!
    ;;
  *)
    echo "unknown case $CASE" >&2; exit 2
    ;;
esac
sleep 8
if grim "$OUT/wallpaper-$CASE.png" 2>/dev/null; then
  magick "$OUT/wallpaper-$CASE.png" -format "screen: colors=%k mean=%[fx:int(255*mean)]\n" info:
fi
wait "$PROBE_PID" 2>/dev/null || true

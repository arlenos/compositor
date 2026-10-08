#!/usr/bin/env bash
# The session lock with two outputs, which `session-lock-conformance.sh` cannot
# reach: its winit nesting gives the compositor exactly one.
#
# The X11 backend can open several windows, one output each, and does so under
# `COSMIC_X11_OUTPUTS`. It needs an X server with DRI3, which Xvfb is not; the
# Xwayland of a headless sway is, and shows nothing anywhere. So: headless sway,
# its Xwayland, cosmic-comp on that with two outputs, and `lock-probe` against
# cosmic-comp.
#
# For each mode it checks what holds on EVERY output:
#   surface  the lock surface reaches both outputs, `locked` waits for both
#            and neither shows the desktop while locked
#   crash    the same, then the lock client dies: neither output shows the
#            desktop again
#   unlock   the control: after a clean unlock the desktop is back
#
# Usage: dev/session-lock-two-outputs.sh [surface|crash|unlock ...]
# Requirements: sway with Xwayland, wlr-randr, grim, imagemagick, kitty and a built
# cosmic-comp and lock-probe (`cargo build --features test-client`).
set -uo pipefail

CP="$(cd "$(dirname "$0")/.." && pwd)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
for bin in "$CP/target/debug/cosmic-comp" "$CP/target/debug/lock-probe"; do
  [ -x "$bin" ] || { echo "missing $bin - build it first" >&2; exit 1; }
done
[ $# -gt 0 ] || set -- surface crash unlock
failures=0

# How many colours one output shows: 1 is a flat lock screen, more is content.
capture() {
  WAYLAND_DISPLAY="$1" grim -o "$2" "$3" 2>/dev/null || { echo 0; return; }
  magick "$3" -format %k info:
}

# One fresh compositor per mode, so a lock left behind by `crash` cannot decide
# the next mode's result.
run_mode() {
  local mode="$1" d sw cc k wl x sock
  d="$(mktemp -d)"
  printf 'output HEADLESS-1 mode 1920x1080\nxwayland enable\n' > "$d/config"
  mkdir -p "$d/cfg/arlen"; printf 'screen_capture = true\n' > "$d/cfg/arlen/sensing.toml"

  env -u WAYLAND_DISPLAY -u DISPLAY WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
    sway -c "$d/config" > "$d/sway.log" 2>&1 & sw=$!
  sock=""
  for _ in $(seq 1 20); do
    sock="$(ls -t "/run/user/$(id -u)/sway-ipc."*".$sw.sock" 2>/dev/null | head -1)"
    [ -n "$sock" ] && break; sleep 0.5
  done
  # Xwayland starts on first use; asking for $DISPLAY from inside sway starts it.
  swaymsg -s "$sock" exec "echo \$DISPLAY > $d/display" > /dev/null
  for _ in $(seq 1 20); do [ -s "$d/display" ] && break; sleep 0.5; done
  x="$(head -1 "$d/display")"

  env -u WAYLAND_DISPLAY XDG_CONFIG_HOME="$d/cfg" COSMIC_X11_OUTPUTS=2 COSMIC_BACKEND=x11 \
    DISPLAY="$x" "$CP/target/debug/cosmic-comp" > "$d/comp.log" 2>&1 & cc=$!
  wl=""
  for _ in $(seq 1 40); do
    wl="$(grep -oE 'Listening on "wayland-[0-9]+"' "$d/comp.log" | grep -oE 'wayland-[0-9]+')"
    [ -n "$wl" ] && break; sleep 0.5
  done

  echo "=== $mode ==="
  if [ -z "$wl" ]; then
    echo "  FAIL: cosmic-comp did not start"; tail -5 "$d/comp.log" | sed 's/^/    /'
    failures=$((failures + 1)); kill "$cc" "$sw" 2>/dev/null; wait 2>/dev/null; rm -rf "$d"; return
  fi

  # With no output config the two outputs both start at 0,0, one on top of the
  # other. Put them side by side the way a user's display settings would.
  sleep 1
  WAYLAND_DISPLAY="$wl" wlr-randr --output X11-1 --pos 956,0
  sleep 1

  # Something recognisable on the desktop, so a capture can tell "the lock hid
  # the desktop" from "the desktop was empty anyway".
  WAYLAND_DISPLAY="$wl" DISPLAY="" kitty -o background=#c81e78 -o font_size=40 \
    sh -c 'echo SECRET-DESKTOP-CONTENT; sleep 600' >/dev/null 2>&1 & k=$!
  local desktop="" o
  for _ in $(seq 1 30); do
    sleep 0.5
    for o in X11-0 X11-1; do
      [ "$(capture "$wl" "$o" "$d/desk-$o.png")" -gt 1 ] && desktop="$o"
    done
    [ -n "$desktop" ] && break
  done
  if [ -z "$desktop" ]; then
    echo "  FAIL: the desktop never showed the window, so hiding it proves nothing"
    failures=$((failures + 1)); kill "$k" "$cc" "$sw" 2>/dev/null; wait 2>/dev/null; rm -rf "$d"; return
  fi
  echo "  desktop: the window is on $desktop"

  WAYLAND_DISPLAY="$wl" "$CP/target/debug/lock-probe" "$mode" 4 > "$d/probe.log" 2>&1 &
  local probe=$!
  sleep 2
  local hidden=0
  for o in X11-0 X11-1; do
    [ "$(capture "$wl" "$o" "$d/locked-$o.png")" -eq 1 ] && hidden=$((hidden + 1))
  done
  wait "$probe" 2>/dev/null
  sleep 1.5
  grep -E "outputs|frame|locked |verdict|crash|unlock" "$d/probe.log" | sed 's/^/    /'

  grep -q "outputs count=2" "$d/probe.log" \
    || { echo "  FAIL: the lock client did not see two outputs"; failures=$((failures + 1)); }
  grep -q "with 2 of 2 outputs already presented" "$d/probe.log" \
    || { echo "  FAIL: locked did not wait for both outputs"; failures=$((failures + 1)); }
  # The protocol sets no deadline, but a lock that takes seconds to be confirmed
  # is a locker that cannot suspend: the fixed path takes about 30 ms here, one
  # vblank on hardware, and the defect this was written against took 2.3 s or
  # never came. One second leaves room for a loaded machine.
  local at; at="$(grep -oE "locked arrived at [0-9.]+ms" "$d/probe.log" | grep -oE "[0-9.]+" | cut -d. -f1)"
  [ -n "$at" ] && [ "$at" -lt 1000 ] \
    || { echo "  FAIL: locked took ${at:-forever} ms"; failures=$((failures + 1)); }
  [ "$hidden" -eq 2 ] && echo "  ok: while locked, both outputs show one flat colour" \
    || { echo "  FAIL: while locked, $((2 - hidden)) output(s) still showed more"; failures=$((failures + 1)); }

  local shown=0
  for o in X11-0 X11-1; do
    [ "$(capture "$wl" "$o" "$d/after-$o.png")" -gt 1 ] && shown=$((shown + 1))
  done
  case "$mode" in
    unlock)
      [ "$shown" -ge 1 ] && echo "  ok: the desktop is back after a clean unlock" \
        || { echo "  FAIL: the desktop did not come back after unlock"; failures=$((failures + 1)); } ;;
    *)
      [ "$shown" -eq 0 ] && echo "  ok: after the client is gone, neither output shows the desktop" \
        || { echo "  FAIL: an output shows the desktop after the lock client went away"; failures=$((failures + 1)); } ;;
  esac
  kill -0 "$cc" 2>/dev/null || { echo "  FAIL: the compositor died"; failures=$((failures + 1)); }

  kill "$k" "$cc" "$sw" 2>/dev/null; wait 2>/dev/null; rm -rf "$d"
}

for mode in "$@"; do run_mode "$mode"; done
[ "$failures" -eq 0 ] || { echo "$failures check(s) failed"; exit 1; }
echo "all checks passed"

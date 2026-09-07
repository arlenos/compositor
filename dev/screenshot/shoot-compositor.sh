#!/usr/bin/env bash
# Headless full-app screenshot under the REAL Arlen compositor. Runs cosmic-comp
# nested in Xvfb (its winit X11 backend), launches a Wayland client under it, and
# grim-captures the composited output.
#
# Unlike shoot-app.sh (X11 / WebKitWebDriver, no Wayland) this hosts genuine
# Wayland clients - the shell's wlr-layer-shell topbar, the Tauri apps - so it is
# the path for verifying shell + compositor UI that the webview-only harness
# cannot reach (the top bar, window decorations, cross-app focus).
#
# Usage:
#   dev/screenshot/shoot-compositor.sh <out.png> <client-cmd> [args...]
#
#   <out.png>     where to write the screenshot
#   <client-cmd>  the Wayland client to launch; it is run with WAYLAND_DISPLAY set
#                 to the compositor's socket and DISPLAY cleared
#
# Tiling captures (verify gaps / tiled headers / borders without a runtime keybind,
# which injection cannot reach): the nested compositor starts a workspace tiling only
# when autotile is on, so seed it headlessly -
#   ARLEN_COMPOSITOR_CONFIG=cfg.toml  (cfg.toml: [layout] inner_gap/outer_gap, smart_gaps=false,
#                                      tiled_headers=true, + a [[layout.window_rules]] action="tile"
#                                      match.app_id="<client app_id>")
#   XDG_STATE_HOME=statedir           (statedir/arlen/compositor/state.toml: `autotile = true`)
#   SHOOT_CLIENT2="<client> [args]"   (a second window so the BSP split is visible)
# Confirmed 28 Jun: two kitty windows tile with gaps + per-window headers + rounded corners.
#
# This is the closed nested verify loop (autonomous-verify-pipeline-plan.md): boot
# the compositor nested -> optionally INJECT input -> grim-capture -> optionally
# COMPARE to a baseline. With no inject/baseline it is just a capture (its original
# use). With them it is a self-checking regression tripwire.
#
# Env:
#   COMPOSITOR_PATH    the compositor repo (default ~/Repositories/compositor)
#   SHOOT_SETTLE       seconds to wait for the client to render (default 5)
#   SHOOT_DISPLAY      the Xvfb display to use (default :99)
#   SHOOT_CLIENT_LOG   capture the client's stdout/stderr here (default /dev/null);
#                      set it to a file to debug why a client did not render
#   SHOOT_INJECT       a command run after settle, before capture, to inject input.
#                      NOTE on reach (tested 28 Jun): injecting into the NESTED
#                      surface under this Xvfb harness is unsolved. The compositor
#                      runs its x11 backend (picks x11 when DISPLAY is set) reading
#                      XInput2 events on its Xvfb window, but `xdotool`/XTEST into
#                      Xvfb did NOT reach the nested Wayland client even with
#                      windowfocus (the key never appeared at a nested shell prompt);
#                      ydotool/uinput inject at the evdev layer and reach the HOST
#                      seat, not the nested surface. So this harness is reliable for
#                      CAPTURE/render verification (grim of compositor chrome + the
#                      client surface), and inject-requiring tests (click-path) need
#                      the QEMU VM pass (QMP input-send-event into a real-evdev guest)
#                      or a DRM/headless-seat nested setup, not this Xvfb path. The
#                      command still runs with both the Xvfb DISPLAY and the
#                      compositor's WAYLAND_DISPLAY set (so an X11 tool can at least
#                      connect to Xvfb, e.g. to inspect windows).
#   SHOOT_INJECT_SETTLE seconds to wait after inject before capture (default 1)
#   SHOOT_CLIENT2      a second client command launched under the compositor after
#                      the first settles, for multi-window / tiling-chrome captures
#   SHOOT_BASELINE     a reference PNG; if set, compare the capture to it after
#                      grim and FAIL (exit 3) when the differing-pixel count
#                      exceeds SHOOT_TOLERANCE. A missing baseline writes the shot
#                      and passes (first-time inspection). Net-new surfaces with no
#                      baseline are left for visual inspection of <out.png>.
#   SHOOT_TOLERANCE    max differing-pixel count for a baseline PASS (default 100)
#
# Requirements: Xvfb, grim, a built cosmic-comp at
# $COMPOSITOR_PATH/target/debug/cosmic-comp; plus ydotool/wtype if SHOOT_INJECT is
# used and imagemagick (`magick compare`) if SHOOT_BASELINE is used. A ydotool
# inject auto-starts ydotoold (this host's /dev/uinput is ACL-granted to the
# user, so no sudo) and tears it down with the rest.
set -euo pipefail

OUT="${1:?usage: shoot-compositor.sh <out.png> <client-cmd> [args...]}"
shift
[ "$#" -ge 1 ] || { echo "usage: shoot-compositor.sh <out.png> <client-cmd> [args...]" >&2; exit 2; }

COMPOSITOR_PATH="${COMPOSITOR_PATH:-$HOME/Repositories/compositor}"
CC_BIN="$COMPOSITOR_PATH/target/debug/cosmic-comp"

# Build before capturing, rather than running whatever binary happens to be on
# disk. This script used to skip straight to the check below, and on 6 August it
# screenshotted a compositor built on 29 June: a change was verified against six
# weeks of other code, and the screenshot said it passed. A harness that silently
# verifies the wrong artifact is worse than one that fails, because its output
# looks exactly like evidence.
#
# Cheap in the normal case - an up-to-date tree links in a few seconds - and the
# cost is only ever paid where it is about to be wrong. Set COMPOSITOR_SKIP_BUILD
# to run a binary you built deliberately.
if [ -z "${COMPOSITOR_SKIP_BUILD:-}" ]; then
  echo "building cosmic-comp (COMPOSITOR_SKIP_BUILD=1 to skip)" >&2
  ( cd "$COMPOSITOR_PATH" && cargo build --bin cosmic-comp ) >&2 \
    || { echo "compositor build failed; refusing to screenshot a stale binary" >&2; exit 1; }
fi

[ -x "$CC_BIN" ] || { echo "no cosmic-comp at $CC_BIN (build it, or set COMPOSITOR_PATH)" >&2; exit 1; }

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

# The sensing master switch, stated rather than assumed. The compositor refuses
# every capture while it is off, so this harness would otherwise depend on the
# switch being absent on the machine that runs it - and a switch our own tooling
# quietly walks around is a switch an app walks around. Setting it on turns that
# dependency into a statement: this run captures because the switch permits it.
#
# Written into a private config dir so it can never touch the user's own switch.
# Set ARLEN_SENSING_HOME to point at a config tree with the switch OFF to watch
# the refusal instead; the run then fails at grim, which is the correct outcome.
if [ -z "${ARLEN_SENSING_HOME:-}" ]; then
  SENSING_HOME="$(mktemp -d)"
  mkdir -p "$SENSING_HOME/arlen"
  printf 'screen_capture = true\n' > "$SENSING_HOME/arlen/sensing.toml"
  trap 'rm -rf "$SENSING_HOME"' EXIT
else
  SENSING_HOME="$ARLEN_SENSING_HOME"
fi
export XDG_CONFIG_HOME="$SENSING_HOME"
SETTLE="${SHOOT_SETTLE:-5}"
DISP="${SHOOT_DISPLAY:-:99}"
LOG="$(mktemp)"

cleanup() {
  kill "${CLIENT2_PID:-}" "${CLIENT_PID:-}" "${CC_PID:-}" "${XVFB_PID:-}" "${YDOTOOLD_PID:-}" 2>/dev/null || true
  wait 2>/dev/null || true
  # The compositor's own log, kept only when asked for. It answers questions a
  # screenshot cannot - which surface it gave keyboard focus to, for one, which
  # it already prints at debug level - and until now it was deleted on the way
  # out, so anyone wanting it had to edit this script first.
  if [ -n "${SHOOT_COMPOSITOR_LOG:-}" ] && [ -s "$LOG" ]; then
    cp "$LOG" "$SHOOT_COMPOSITOR_LOG" 2>/dev/null || true
  fi
  rm -f "/tmp/.X${DISP#:}-lock" "$LOG" 2>/dev/null || true
}
trap cleanup EXIT

rm -f "/tmp/.X${DISP#:}-lock"
# Xvfb is the one non-obvious dependency of the whole screenshot loop, and
# without it these scripts died with a bare "xvfb-run: command not found" that
# says nothing about what to install. The screenshot loop is mandatory for any
# UI change, so failing to run it must be actionable, never cryptic.
require_xvfb() {
  local bin="$1"
  command -v "$bin" >/dev/null 2>&1 && return 0
  echo "error: '$bin' not found - the headless screenshot loop needs Xvfb." >&2
  echo "  Arch/EndeavourOS: sudo pacman -S xorg-server-xvfb" >&2
  echo "  Debian/Ubuntu:    sudo apt install xvfb" >&2
  echo "  Fedora:           sudo dnf install xorg-x11-server-Xvfb" >&2
  exit 127
}
# SHOOT_SKIP_XVFB reuses an X server already running on $SHOOT_DISPLAY instead of
# starting one. It exists because cosmic-comp cannot run on Xvfb at all: its X11
# backend needs DRI3 and its winit fallback needs `EGL_EXT_device_drm`, and Xvfb
# provides neither, so every run here died before creating a Wayland socket. A
# DRM-capable X server does satisfy it, and on a Wayland host XWayland (:0) is one.
#
# The cost is honest: the nested compositor then opens a window on that real
# session, briefly and visibly, and is torn down with the run. Capture is
# unaffected either way, since grim reads the nested compositor's own output and
# not the X server.
if [ -n "${SHOOT_SKIP_XVFB:-}" ]; then
  if ! DISPLAY="$DISP" xdpyinfo >/dev/null 2>&1; then
    echo "error: SHOOT_SKIP_XVFB is set but nothing answers on $DISP" >&2
    exit 1
  fi
  echo "using the existing X server on $DISP (SHOOT_SKIP_XVFB)" >&2
else
  require_xvfb Xvfb
  Xvfb "$DISP" -screen 0 1920x1080x24 >/dev/null 2>&1 &
  XVFB_PID=$!
  sleep 2
fi

# WAYLAND_DISPLAY is UNSET deliberately, not merely overridden. cosmic-comp picks
# its backend from the environment (`backend/mod.rs:29`): X11 first, then winit on
# failure, and winit prefers Wayland whenever WAYLAND_DISPLAY is set. Inherited
# from a developer's own session that sends it nesting into the real compositor
# rather than the Xvfb this script just started, which is not what the script
# means. Unsetting it demonstrably moves winit onto X11.
#
# It is NOT sufficient on its own, and saying so here is the point: with it unset
# the run still fails on this host, one step later and for a different reason -
# `EGL_EXT_device_drm` is missing because Xvfb has no DRI3, so cosmic-comp's winit
# backend cannot bind a display. Anyone chasing "the compositor will not start"
# should look there and not re-litigate this line.
env -u WAYLAND_DISPLAY DISPLAY="$DISP" "$CC_BIN" >"$LOG" 2>&1 &
CC_PID=$!

# cosmic-comp picks its own socket name (wayland-N, ignoring WAYLAND_DISPLAY); it
# logs "Listening on \"wayland-N\"". Parse it rather than guess, then wait for the
# socket file to actually exist.
WL=""
for _ in $(seq 1 40); do
  WL="$(grep -oE 'wayland-[0-9]+' "$LOG" | head -1 || true)"
  [ -n "$WL" ] && [ -S "$XDG_RUNTIME_DIR/$WL" ] && break
  sleep 0.5
done
if [ -z "$WL" ] || [ ! -S "$XDG_RUNTIME_DIR/$WL" ]; then
  echo "cosmic-comp did not come up on $DISP; last log lines:" >&2
  tail -20 "$LOG" >&2
  exit 1
fi
echo "compositor up on $WL (display $DISP)"

CLIENT_LOG="${SHOOT_CLIENT_LOG:-/dev/null}"
WAYLAND_DISPLAY="$WL" DISPLAY="" "$@" >"$CLIENT_LOG" 2>&1 &
CLIENT_PID=$!
sleep "$SETTLE"

# Optional second client (SHOOT_CLIENT2), for multi-window / tiling-chrome
# captures: a verbatim command launched under the compositor's WAYLAND_DISPLAY
# with its own settle so both surfaces paint before capture. Tiling layout (so the
# two windows sit side by side rather than stacked) is the compositor's own config
# concern, not set here.
if [ -n "${SHOOT_CLIENT2:-}" ]; then
  echo "client2: $SHOOT_CLIENT2"
  WAYLAND_DISPLAY="$WL" DISPLAY="" bash -c "$SHOOT_CLIENT2" >>"$CLIENT_LOG" 2>&1 &
  CLIENT2_PID=$!
  sleep "$SETTLE"
fi

# A ydotool inject needs ydotoold (the uinput daemon ydotool talks to over a
# socket). Start it ourselves so the click-path tests are self-sufficient: this
# host grants the invoking user rw on /dev/uinput via an ACL, so no sudo/udev
# step is needed. We pin YDOTOOL_SOCKET to the runtime dir and export it so the
# inject command inherits it. If the socket never appears (no uinput access) the
# inject simply fails gracefully below, recording the pre-inject state.
if [ -n "${SHOOT_INJECT:-}" ] && printf '%s' "$SHOOT_INJECT" | grep -q ydotool; then
  export YDOTOOL_SOCKET="${YDOTOOL_SOCKET:-$XDG_RUNTIME_DIR/.ydotool_socket}"
  if [ ! -S "$YDOTOOL_SOCKET" ]; then
    ydotoold -p "$YDOTOOL_SOCKET" >/dev/null 2>&1 &
    YDOTOOLD_PID=$!
    for _ in $(seq 1 10); do
      [ -S "$YDOTOOL_SOCKET" ] && break
      sleep 0.3
    done
    [ -S "$YDOTOOL_SOCKET" ] || echo "ydotoold did not come up (uinput access?); inject will likely fail" >&2
  fi
fi

# Optional input injection, then a brief re-settle so the result paints before
# capture. The command runs verbatim with both the Xvfb DISPLAY and the
# compositor's WAYLAND_DISPLAY set; see the SHOOT_INJECT header note on the
# nested-surface reach caveat. A failing inject is logged but does not abort the
# capture (so the shot still records the pre-inject state for debugging).
if [ -n "${SHOOT_INJECT:-}" ]; then
  echo "inject: $SHOOT_INJECT"
  WAYLAND_DISPLAY="$WL" DISPLAY="$DISP" bash -c "$SHOOT_INJECT" \
    || echo "inject step failed (continuing to capture)" >&2
  sleep "${SHOOT_INJECT_SETTLE:-1}"
fi

WAYLAND_DISPLAY="$WL" grim "$OUT"
echo "wrote $OUT"

# A capture of one flat colour is reported, not written silently. It is what this
# harness produces for a wlr-layer-shell client (the desktop shell): the surface
# never becomes a composited toplevel here, so grim returns the empty output and
# the PNG looks exactly like a client that failed to render. Both the modified and
# the unmodified shell produced the identical 444x1425 single-colour frame on
# 9 August, which is how the difference between "broken" and "not visible to this
# harness" was established - after half an hour spent reading the blank one as a
# regression. The shell's own checks are `just shell-smoke` (it starts, it answers)
# and `dev/vm/verify.py --require-bar` (the bar reaches the screen).
if command -v magick >/dev/null 2>&1; then
  colors="$(magick "$OUT" -format %k info: 2>/dev/null || echo "")"
  if [ "$colors" = "1" ]; then
    echo "NOTE: the capture is one flat colour - nothing of the client is in it." >&2
    echo "      A layer-shell client (the desktop shell) looks like this here even" >&2
    echo "      when healthy; use 'just shell-smoke' or dev/vm/verify.py --require-bar." >&2
  fi
fi

# Optional baseline tripwire: fail if the capture differs from a reference PNG by
# more than SHOOT_TOLERANCE pixels. `magick compare -metric AE` is the installed
# odiff equivalent; it prints the differing-pixel count to stderr and exits 0/1
# (identical/differs) or >=2 on a real error (e.g. a size mismatch), which is a
# FAIL rather than a silent pass.
if [ -n "${SHOOT_BASELINE:-}" ]; then
  if [ ! -f "$SHOOT_BASELINE" ]; then
    echo "baseline $SHOOT_BASELINE not found; wrote $OUT for first-time inspection" >&2
    exit 0
  fi
  set +e
  diff_out="$(magick compare -metric AE "$SHOOT_BASELINE" "$OUT" null: 2>&1)"
  cmp_rc=$?
  set -e
  if [ "$cmp_rc" -ge 2 ]; then
    echo "FAIL: compare error (size/format mismatch?): $diff_out" >&2
    exit 3
  fi
  diff_px="${diff_out%%[!0-9]*}"
  diff_px="${diff_px:-0}"
  tol="${SHOOT_TOLERANCE:-100}"
  echo "baseline diff: ${diff_px}px (tolerance ${tol})"
  if [ "$diff_px" -gt "$tol" ]; then
    echo "FAIL: capture differs from baseline by ${diff_px}px (> ${tol})" >&2
    exit 3
  fi
  echo "PASS: within tolerance of baseline"
fi

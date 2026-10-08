#!/usr/bin/env bash
# The `Terminate` binding is refused while the session is locked.
#
# Whoever is at the keyboard of a locked machine must not be able to end the
# owner's session and everything unsaved in it. Arlen binds `terminate` to no key
# by default, so this gives a private compositor one (Super+Alt+Escape) through
# its own HOME, locks it with `lock-probe`, presses the chord, and checks the
# compositor is still there. Then the control: the same chord with the lock
# lifted must end it, or the locked press proved nothing.
#
# Runs cosmic-comp nested in a headless sway, so nothing appears on any screen,
# and with a temporary HOME, so the user's own compositor.toml is never read.
#
# Requirements: sway, xkbcli and a built cosmic-comp, lock-probe and pointer-driver
# (`cargo build --features test-client`).
set -uo pipefail

CP="$(cd "$(dirname "$0")/.." && pwd)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
for bin in "$CP/target/debug/cosmic-comp" "$CP/target/debug/lock-probe" "$CP/target/debug/pointer-driver"; do
  [ -x "$bin" ] || { echo "missing $bin - build it first" >&2; exit 1; }
done

D="$(mktemp -d)"
cleanup() {
  exec 4>&- 2>/dev/null
  kill "${PROBE:-}" "${DRIVER:-}" "${CC:-}" "${SW:-}" 2>/dev/null
  wait 2>/dev/null
  [ -n "${KEEP_LOG:-}" ] && cp "$D/comp.log" "$KEEP_LOG"; rm -rf "$D"
}
trap cleanup EXIT

printf 'output HEADLESS-1 mode 1920x1080\n' > "$D/sway.conf"
mkdir -p "$D/home/.config/arlen"
printf '[keybindings]\n"Super+Alt+Escape" = "terminate"\n' > "$D/home/.config/arlen/compositor.toml"

env -u WAYLAND_DISPLAY -u DISPLAY WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
  sway -c "$D/sway.conf" > "$D/sway.log" 2>&1 & SW=$!
HOST=""
for _ in $(seq 1 40); do
  c="$(pgrep -P "$SW" -n 2>/dev/null)"
  [ -n "$c" ] && HOST="$(tr '\0' '\n' < "/proc/$c/environ" 2>/dev/null | grep -m1 '^WAYLAND_DISPLAY=' | cut -d= -f2)"
  [ -n "$HOST" ] && break; sleep 0.5
done
[ -n "$HOST" ] || { echo "headless sway did not start"; exit 1; }

env -u DISPLAY HOME="$D/home" XDG_CONFIG_HOME="$D/home/.config" WAYLAND_DISPLAY="$HOST" \
  "$CP/target/debug/cosmic-comp" > "$D/comp.log" 2>&1 & CC=$!
WL=""
for _ in $(seq 1 60); do
  WL="$(grep -oE 'Listening on "wayland-[0-9]+"' "$D/comp.log" | grep -oE 'wayland-[0-9]+')"
  [ -n "$WL" ] && break; sleep 0.5
done
[ -n "$WL" ] || { echo "cosmic-comp did not start"; tail -5 "$D/comp.log"; exit 1; }
sleep 2

# The chord goes in as real evdev codes on the HOST seat (Super 125, Alt 56,
# Escape 1), through `pointer-driver`. See its header for why `wtype` cannot.
mkfifo "$D/keys"
WAYLAND_DISPLAY="$HOST" "$CP/target/debug/pointer-driver" < "$D/keys" > "$D/driver.log" 2>&1 & DRIVER=$!
exec 4>"$D/keys"
for _ in $(seq 1 20); do grep -q "^ready" "$D/driver.log" && break; sleep 0.25; done
chord() {
  printf 'chord %s\n' "${1:-125 56 1}" >&4
  for _ in $(seq 1 20); do grep -q "ok chord" "$D/driver.log" && break; sleep 0.1; done
  sed -i 's/ok chord/done chord/' "$D/driver.log"
}
failures=0

WAYLAND_DISPLAY="$WL" "$CP/target/debug/lock-probe" unlock 6 > "$D/probe.log" 2>&1 & PROBE=$!
for _ in $(seq 1 40); do grep -q " locked " "$D/probe.log" && break; sleep 0.25; done
if ! grep -q " locked " "$D/probe.log"; then
  echo "FAIL: the session never locked, so nothing here was tested"; exit 1
fi

chord
sleep 1.5
if kill -0 "$CC" 2>/dev/null; then
  echo "ok: while locked, Super+Alt+Escape left the session running"
else
  echo "FAIL: the Terminate binding ended a locked session"; exit 1
fi

wait "$PROBE" 2>/dev/null; PROBE=""
grep -q "unlock_and_destroy sent" "$D/probe.log" || { echo "FAIL: the probe did not unlock"; exit 1; }
sleep 1

chord
for _ in $(seq 1 20); do kill -0 "$CC" 2>/dev/null || break; sleep 0.25; done
if kill -0 "$CC" 2>/dev/null; then
  echo "FAIL: the control did not land: unlocked, the chord did not end the session either"
  failures=$((failures + 1))
else
  echo "ok: unlocked, the same chord ends the session (the control)"
fi

[ "$failures" -eq 0 ] || { echo "$failures check(s) failed"; exit 1; }
echo "all checks passed"

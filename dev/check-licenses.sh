#!/usr/bin/env bash
# REUSE compliance, plus the one thing REUSE cannot notice here.
#
# WHAT `reuse lint` GATES IN THIS REPO, honestly. Licensing is stated by path in
# REUSE.toml, with a blanket `**` annotation saying the tree is GPL-3.0-only.
# That is the true statement about this fork, but it means the usual headline
# defect - "somebody added a file and never said what it is" - CANNOT fail here:
# the blanket answers it for them. Measured 8 Sep: dropping an unlicensed .rs
# into src/ leaves `reuse lint` green. So lint still gates that every declared
# identifier has its text in LICENSES/, that no expression is bad or deprecated,
# and that REUSE.toml parses and its paths still resolve - and nothing more.
#
# THE DEFECT THAT MATTERS HERE IS THE OPPOSITE ONE. The danger in a fork that
# merges upstream weekly and vendors the occasional file is a NEW licence
# arriving in the tree: a vendored MIT or proprietary file, or an upstream
# dependency changing terms. That is invisible to lint - a correctly-headed MIT
# file is perfectly compliant - and it is exactly the event a person should look
# at. So this also pins the set of licences the tree is allowed to contain.
# Growing that set is not an error; it is a review, and the way to record the
# review is to add the licence here on purpose.
#
# Usage: dev/check-licenses.sh
set -euo pipefail
cd "$(dirname "$0")/.."

command -v reuse >/dev/null || { echo "FAIL: reuse is not installed (pipx install reuse)" >&2; exit 2; }

# The licences this tree is known to contain, and why each is here:
#   GPL-3.0-only        the fork and its upstream base
#   GPL-3.0-or-later    src/dbus/power.rs
#   GPL-2.0-or-later    the Arlen protocol XMLs under resources/protocols
#   MPL-2.0, MIT,
#   Apache-2.0          src/logger/serializer.rs, a vendored tri-licensed file
#   AGPL-3.0-only       dev/fixtures/sensing-vectors/README.md, carried over
#                       from the monorepo
# Three of those are questioned in compositor-reports.md rather than normalised
# here. This list records what IS in the tree, not what ought to be.
EXPECTED="AGPL-3.0-only Apache-2.0 GPL-2.0-or-later GPL-3.0-only GPL-3.0-or-later MIT MPL-2.0"

echo "== reuse lint =="
reuse lint || exit 1

echo
echo "== licence set =="
FOUND="$(reuse lint 2>/dev/null | sed -n 's/^\* Used licenses: //p' | tr -d ' ' | tr ',' '\n' | sort | tr '\n' ' ')"
WANT="$(printf '%s\n' $EXPECTED | sort | tr '\n' ' ')"
if [ "$FOUND" != "$WANT" ]; then
  echo "FAIL: the set of licences in this tree changed." >&2
  echo "  recorded: $WANT" >&2
  echo "  found   : $FOUND" >&2
  echo "A new licence is a review, not a mistake: work out which file brought it" >&2
  echo "in, decide whether it belongs in a GPL-3.0-only fork, and if it does, add" >&2
  echo "it to EXPECTED in this script with the reason." >&2
  exit 1
fi
echo "ok: $WANT"

# Not checked here: whether each declaration is CORRECT for its file. Nothing
# mechanical can decide that a first-party protocol XML ought to be
# GPL-2.0-or-later; a reader has to.
echo
echo "PASS: REUSE-compliant and the licence set is unchanged"

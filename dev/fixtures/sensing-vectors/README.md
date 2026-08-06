<!--
SPDX-FileCopyrightText: 2026 Tim Kicker

SPDX-License-Identifier: AGPL-3.0-only
-->
# Sensing switch reader vectors

Three components read the sensing master switch and each keeps its own copy of
the four-line predicate that decides what it says: Settings (writes it), the xdg
portal (enforces it at the capture portals) and the compositor (enforces it at
the Wayland capture protocols and the context-menu screenshot).

Copying was the right call for two and stayed the right call for three, because
the alternative is a cross-repo release dependency for four lines. But the risk
was never the duplication, it was **divergence**: if one copy drifts, one path
silently stops enforcing, and a master switch that enforces on two paths out of
three is not a master switch.

So the copies are not merged, they are **held to one written table**. Every
implementation runs against every file here and must agree on the answer. Three
implementations agreeing on a table are as safe as one implementation, and it
costs no coupling between the repositories.

## The format has no parser

Each file **is** a switch file, verbatim, and its expected reading is the part of
its name before `__`:

| prefix | the reader must answer |
|---|---|
| `off` | the key is stated off, capture is refused |
| `on` | the key is stated on |
| `not-stated` | the file parses but is about some other switch, so this one is unconfigured |
| `unreadable` | nothing parses, or the value is neither `true` nor `false`; treated as off |

There is deliberately no manifest and no encoding to decode. A table listing
inputs would need an escaping scheme, that scheme would be a fifth parser, and it
would be the one thing with no test of its own.

## Adding a case

Drop in a file named for the answer. Every reader picks it up on its next run
with no code change, which is the point: a case is added once and three
implementations are held to it.

`unreadable__truncated-mid-value.toml` is the one worth understanding before
changing anything here. `screen_capture = fal` is a write caught in the middle. A
reader asking only "does this say false" answers no and resumes capture for
somebody who believes they switched it off.

## The compositor's copy

The compositor is a separate repository and cannot read this directory, so it
carries a copy at the same relative path. `dev/scripts/check-sensing-vectors.sh`
compares the two wherever both trees are checked out, which is where anyone edits
them. If you change a vector, run it.

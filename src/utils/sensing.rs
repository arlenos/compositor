// SPDX-License-Identifier: GPL-3.0-only

//! The sensing master switch, as it binds the compositor.
//!
//! The switch is the user's statement that screen capture is off for everyone.
//! Its only asset is their belief that it worked, so a remaining path does not
//! weaken it, it voids it. The xdg portal is one route to the compositor's
//! pixels; the Wayland capture protocols are another, and this is that side.
//!
//! **A trust filter is not a kill switch.** The capture globals are already
//! behind `client_not_sandboxed`, and that is not this check and cannot stand in
//! for it. A filter asks *who may speak this protocol*; the switch asks *whether
//! the capability is live at all*, and it exists precisely to bind principals
//! that ARE trusted - our own shell is trusted and must stop too. The filter also
//! happens to be wide open today (a client with no security context passes it,
//! and `arlen-run` confinement is off by default), so declining to check here
//! would be pointing at a gate that stands open.
//!
//! **What it binds:** a client asking for pixels. Not the compositor drawing
//! frames to a screen, which is not capture.
//!
//! This is a third copy of a four-line predicate, which is one more than the
//! reasoning that allowed the first two. Settings and the portal keep separate
//! copies deliberately rather than coupling an app to a daemon; a third consumer
//! in a different repository is where that stops paying, and the consolidation
//! into one small crate is the right follow-up - it is not done here only because
//! this repository reaches the Arlen crates over a git dependency, so the crate
//! would have to be published before this could compile against it.

use std::path::PathBuf;

/// What the file says about one key.
///
/// Three answers rather than a boolean, because "the file does not say" and "the
/// file is not sayable" need opposite treatment.
#[derive(Debug, PartialEq)]
enum Reading {
    /// The key is stated off.
    Off,
    /// The key is stated on.
    On,
    /// The key is not stated, but the file states other settings, so it is about
    /// a different switch and this capability is unconfigured.
    NotStated,
    /// Nothing parses as a setting, or the value is neither `true` nor `false`.
    Unreadable,
}

fn switch_file() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|c| c.join("arlen/sensing.toml"))
}

fn read_key(text: &str, key: &str) -> Reading {
    let mut saw_a_setting = false;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let (name, value) = (name.trim(), value.trim());
        if name.is_empty() {
            continue;
        }
        saw_a_setting = true;
        if name == key {
            return match value {
                "false" => Reading::Off,
                "true" => Reading::On,
                _ => Reading::Unreadable,
            };
        }
    }
    if saw_a_setting {
        Reading::NotStated
    } else {
        Reading::Unreadable
    }
}

/// Whether screen capture is switched off system-wide.
///
/// Read fresh on every call, never cached: the intent is "off, right now", and a
/// value sampled when a capture session opened would keep an already-running
/// screen share alive for the rest of the session. That is the case the switch is
/// most obviously for.
///
/// The cost is a small read per captured frame, which is a page-cached file
/// against a path already throttled to one frame per vblank, and correctness at a
/// master switch is worth more than the syscall.
pub fn screen_capture_is_off() -> bool {
    let Some(path) = switch_file() else {
        return false;
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => matches!(
            read_key(&text, "screen_capture"),
            Reading::Off | Reading::Unreadable
        ),
        // No file: nobody configured anything, and a system nobody configured is
        // a working system.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        // Present and unreadable is the opposite state: somebody stated an intent
        // that can no longer be read, and the safe reading of an unreadable intent
        // is the protective one.
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bytes Settings writes for the off position. Held in the Arlen repo at
    /// `dev/fixtures/sensing-off.toml`, and repeated here rather than included
    /// because this is a different repository - the two cannot share a file, so
    /// they share a test instead.
    const OFF_FIXTURE: &str = "\
# Sensing master switches. Off subtracts from every app at once and is
# enforced where the capability is exercised, not in the settings UI.
screen_capture = false
";

    #[test]
    fn the_file_settings_writes_reads_as_off() {
        assert_eq!(read_key(OFF_FIXTURE, "screen_capture"), Reading::Off);
    }

    #[test]
    fn a_stated_value_is_read_as_stated() {
        assert_eq!(read_key("screen_capture = false", "screen_capture"), Reading::Off);
        assert_eq!(read_key("screen_capture = true", "screen_capture"), Reading::On);
    }

    #[test]
    fn a_file_about_other_switches_leaves_this_one_unconfigured() {
        assert_eq!(read_key("microphone = false", "screen_capture"), Reading::NotStated);
    }

    #[test]
    fn a_corrupted_file_is_unreadable_rather_than_silently_on() {
        // The failure the enum exists for: each of these was "not off" under a
        // boolean reader, so a truncated write resumed capture without saying so.
        assert_eq!(read_key("", "screen_capture"), Reading::Unreadable);
        assert_eq!(read_key("screen_captu", "screen_capture"), Reading::Unreadable);
        assert_eq!(read_key("screen_capture = fal", "screen_capture"), Reading::Unreadable);
        assert_eq!(read_key("# screen_capture = false", "screen_capture"), Reading::Unreadable);
    }

    #[test]
    fn a_trailing_comment_does_not_hide_the_setting() {
        assert_eq!(
            read_key("screen_capture = false # off for the meeting", "screen_capture"),
            Reading::Off
        );
    }
}

/// Where the shared vector table lives, relative to this crate.
const VECTOR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/dev/fixtures/sensing-vectors");

#[cfg(test)]
mod vector_tests {
    use super::*;

    /// The same table Settings and the xdg portal answer, copied into this
    /// repository because a separate repo cannot read theirs. Copying the
    /// predicate was the right call over a cross-repo release dependency for four
    /// lines; what stops the copies diverging is that all three answer this table,
    /// and `dev/scripts/check-sensing-vectors.sh` in the Arlen tree compares the
    /// two directories wherever both are checked out.
    #[test]
    fn every_reader_agrees_with_the_shared_vector_table() {
        let dir = std::path::Path::new(VECTOR_DIR);
        let entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("vector table missing at {}: {e}", dir.display()))
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
            .collect();
        assert!(entries.len() >= 12, "the table lost cases: {} left", entries.len());

        for entry in entries {
            let path = entry.path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let (want, _) = name
                .split_once("__")
                .unwrap_or_else(|| panic!("vector {name} is not named <expected>__<case>.toml"));
            let text = std::fs::read_to_string(&path).unwrap();
            let got = read_key(&text, "screen_capture");
            let expected = match want {
                "off" => Reading::Off,
                "on" => Reading::On,
                "not-stated" => Reading::NotStated,
                "unreadable" => Reading::Unreadable,
                other => panic!("vector {name} claims an answer nobody defines: {other}"),
            };
            assert_eq!(got, expected, "{name}");
        }
    }
}

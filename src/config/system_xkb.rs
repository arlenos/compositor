// SPDX-License-Identifier: GPL-3.0-only

//! The keyboard layout the system was installed with, read from
//! `localectl status` and, failing that, `/etc/vconsole.conf`.
//!
//! The compositor starts with this layout unless `compositor.toml` names one,
//! so a fresh install types in the language it was set up in. Fork-only; moved
//! out of upstream's `config/mod.rs` unchanged on 8 October.

/// Reads the system XKB layout from `localectl status`.
///
/// Parses lines like:
///   X11 Layout: de
///   X11 Variant: nodeadkeys
///   X11 Model: pc105
///   X11 Options: compose:ralt
pub(super) struct SystemXkb {
    pub(super) layout: String,
    pub(super) variant: String,
    pub(super) model: String,
    pub(super) options: String,
}

/// One `localectl status` value, with systemd's not-set placeholder read as not
/// set.
///
/// `localectl` prints `X11 Layout: (unset)` when nothing is configured, and the
/// literal was being taken as a layout NAME. Two things followed, and the second
/// is the worse one: xkbcommon was handed `pc+(unset)+inet(evdev)`, refused the
/// whole keymap ("Illegal include statement"), and fell back to `us`; and because
/// `"(unset)"` is not empty, the caller's chain stopped there and never reached
/// its own last resort, `/etc/vconsole.conf`. So a machine whose only keyboard
/// configuration was the console keymap got a US layout and one error line.
///
/// Seen on a real boot of the shipped image on 12 Aug, in the serial log.
pub(super) fn systemd_value(raw: &str) -> String {
    let v = raw.trim();
    // `(unset)` is systemd's own spelling for absent, printed by `localectl` and
    // `hostnamectl` alike. `n/a` is its sibling in a few versions.
    if v == "(unset)" || v == "n/a" {
        return String::new();
    }
    v.to_string()
}

pub(super) fn read_system_xkb_layout() -> SystemXkb {
    let mut result = SystemXkb {
        layout: String::new(),
        variant: String::new(),
        model: String::new(),
        options: String::new(),
    };

    let output = match std::process::Command::new("localectl")
        .arg("status")
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return result,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("X11 Layout:") {
            result.layout = systemd_value(v);
        } else if let Some(v) = line.strip_prefix("X11 Variant:") {
            result.variant = systemd_value(v);
        } else if let Some(v) = line.strip_prefix("X11 Model:") {
            result.model = systemd_value(v);
        } else if let Some(v) = line.strip_prefix("X11 Options:") {
            result.options = systemd_value(v);
        }
    }

    if !result.layout.is_empty() {
        tracing::info!(
            "read_system_xkb_layout: layout={} variant={} model={} options={}",
            result.layout,
            result.variant,
            result.model,
            result.options,
        );
    }

    result
}

/// Reads the KEYMAP setting from /etc/vconsole.conf.
///
/// Returns the layout string (e.g. "de", "us") or None if not found.
pub(super) fn parse_vconsole_keymap() -> Option<String> {
    let content = std::fs::read_to_string("/etc/vconsole.conf").ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("KEYMAP=") {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

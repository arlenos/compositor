// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the fork's configuration loading. They lived inside upstream's
//! `config/mod.rs` until 8 October; upstream has none there.

/// systemd's placeholder is not a layout name.
///
/// The failure it caused was two-sided: xkbcommon refused the whole keymap
/// (`pc+(unset)+inet(evdev)` is not an include it can resolve, so the user
/// got `us` plus one error line), and the non-empty string convinced the
/// resolution chain it had an answer, so `/etc/vconsole.conf` - the one place
/// a console-only machine records its keyboard - was never read.
#[test]
fn the_unset_placeholder_reads_as_no_value() {
    assert_eq!(super::system_xkb::systemd_value("(unset)"), "");
    assert_eq!(super::system_xkb::systemd_value("  (unset)  "), "");
    assert_eq!(super::system_xkb::systemd_value("n/a"), "");
    assert_eq!(super::system_xkb::systemd_value(""), "");

    // And a real value still survives, including one that merely contains
    // the word.
    assert_eq!(super::system_xkb::systemd_value(" de "), "de");
    assert_eq!(super::system_xkb::systemd_value("nodeadkeys"), "nodeadkeys");
    assert_eq!(
        super::system_xkb::systemd_value("unset-layout"),
        "unset-layout"
    );
}

use super::*;

#[test]
fn test_load_sparse_toml() {
    let dir = std::env::temp_dir().join("arlen-config-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("test-compositor.toml");
    std::fs::write(
        &path,
        r#"
[xkb_config]
layout = "de"

[workspaces]
workspace_layout = "Horizontal"
"#,
    )
    .unwrap();

    let tc = load_toml_config(&path);
    assert_eq!(tc.cosmic.xkb_config.layout, "de", "layout must be 'de'");
    assert_eq!(tc.cosmic.xkb_config.repeat_rate, 25);
    // No [keybindings] section -> defaults are loaded.
    assert!(
        !tc.keybindings.is_empty(),
        "default keybindings should be loaded when no [keybindings] section"
    );
    assert!(
        tc.keybindings.iter().any(|k| k.action == "focus_left"),
        "default keybindings should include focus_left"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn test_load_empty_toml() {
    let dir = std::env::temp_dir().join("arlen-config-test-empty");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("test-empty.toml");
    std::fs::write(&path, "").unwrap();

    let tc = load_toml_config(&path);
    assert_eq!(tc.cosmic.xkb_config.layout, "");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn test_load_missing_toml() {
    let tc = load_toml_config(std::path::Path::new("/nonexistent/path.toml"));
    assert_eq!(tc.cosmic.xkb_config.layout, "");
}

#[test]
fn test_load_layout_config() {
    let dir = std::env::temp_dir().join("arlen-config-test-layout");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("test-layout.toml");
    std::fs::write(
        &path,
        r#"
[layout]
inner_gap = 4
outer_gap = 12
smart_gaps = false

[[layout.window_rules]]
match = { app_id = "pavucontrol" }
action = "float"

[[layout.window_rules]]
match = { app_id = "firefox", title = ".*Picture-in-Picture.*" }
action = "float"
"#,
    )
    .unwrap();

    let tc = load_toml_config(&path);
    assert_eq!(tc.layout.inner_gap, 4);
    assert_eq!(tc.layout.outer_gap, 12);
    assert!(!tc.layout.smart_gaps);
    assert_eq!(tc.layout.window_rules.len(), 2);
    assert_eq!(tc.layout.window_rules[0].action, WindowAction::Float);
    assert!(
        tc.layout.window_rules[0]
            .matcher
            .app_id
            .as_ref()
            .unwrap()
            .is_match("pavucontrol")
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn test_load_keybindings() {
    let dir = std::env::temp_dir().join("arlen-config-test-kb");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("test-keybindings.toml");
    std::fs::write(
        &path,
        r#"
[keybindings]
"Super+T" = "toggle_tiling"
"Super+Shift+H" = "move_left"
"Super+Ctrl+L" = "resize_grow_width"
"#,
    )
    .unwrap();

    let tc = load_toml_config(&path);
    assert_eq!(tc.keybindings.len(), 3);

    let toggle = tc
        .keybindings
        .iter()
        .find(|k| k.action == "toggle_tiling")
        .unwrap();
    assert!(toggle.modifiers.super_key);
    assert!(!toggle.modifiers.shift);
    assert_eq!(toggle.key, "T");

    let move_left = tc
        .keybindings
        .iter()
        .find(|k| k.action == "move_left")
        .unwrap();
    assert!(move_left.modifiers.super_key);
    assert!(move_left.modifiers.shift);
    assert_eq!(move_left.key, "H");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn test_window_match() {
    let match_all = WindowMatch {
        app_id: None,
        title: None,
        window_type: None,
    };
    assert!(match_all.matches("any", "any", false));

    let match_app = WindowMatch {
        app_id: Some(regex::Regex::new("pavucontrol").unwrap()),
        title: None,
        window_type: None,
    };
    assert!(match_app.matches("pavucontrol", "Volume Control", false));
    assert!(!match_app.matches("firefox", "Mozilla", false));

    let match_both = WindowMatch {
        app_id: Some(regex::Regex::new("firefox").unwrap()),
        title: Some(regex::Regex::new(".*PiP.*").unwrap()),
        window_type: None,
    };
    assert!(match_both.matches("firefox", "PiP Window", false));
    assert!(!match_both.matches("firefox", "Normal Page", false));

    let match_dialog = WindowMatch {
        app_id: None,
        title: None,
        window_type: Some("dialog".into()),
    };
    assert!(match_dialog.matches("any", "any", true));
    assert!(!match_dialog.matches("any", "any", false));
}

// ── Keybinding fragments ───────────────────────────────────────────

#[test]
fn load_keybinding_fragments_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let fragments = load_keybinding_fragments(dir.path());
    assert!(fragments.is_empty());
}

#[test]
fn load_keybinding_fragments_missing_dir_is_ok() {
    let dir = std::env::temp_dir().join("arlen-does-not-exist-xyz");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(load_keybinding_fragments(&dir).is_empty());
}

#[test]
fn load_keybinding_fragments_reads_all_tomls() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("com.example.a.toml"),
        "[keybindings]\n\"Super+A\" = \"module:com.example.a:open\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("com.example.b.toml"),
        "[keybindings]\n\"Super+B\" = \"module:com.example.b:activate\"\n\
         \"Ctrl+B\" = \"module:com.example.b:secondary\"\n",
    )
    .unwrap();
    // Non-TOML must be ignored.
    std::fs::write(dir.path().join("README.md"), "ignore me").unwrap();

    let mut fragments = load_keybinding_fragments(dir.path());
    fragments.sort_by(|a, b| a.binding.cmp(&b.binding));
    assert_eq!(fragments.len(), 3);
    assert!(
        fragments
            .iter()
            .any(|e| e.binding == "Super+A" && e.module_id == "com.example.a")
    );
    assert!(
        fragments
            .iter()
            .filter(|e| e.module_id == "com.example.b")
            .count()
            == 2
    );
}

#[test]
fn load_keybinding_fragments_skips_malformed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("broken.toml"), "not valid toml = = =").unwrap();
    std::fs::write(
        dir.path().join("ok.toml"),
        "[keybindings]\n\"Super+X\" = \"module:ok:x\"\n",
    )
    .unwrap();
    let fragments = load_keybinding_fragments(dir.path());
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0].module_id, "ok");
}

#[test]
fn keybinding_fragment_dir_is_sibling_of_toml() {
    let toml = std::path::Path::new("/etc/arlen/compositor.toml");
    let frag = keybinding_fragment_dir(toml);
    assert_eq!(
        frag,
        std::path::Path::new("/etc/arlen/compositor.d/keybindings.d")
    );
}

/// Round-trip: serialize a `ArlenRuntimeState`, parse it back,
/// and confirm the fields survive. Catches accidental serde
/// drift on the runtime-state schema.
#[test]
fn runtime_state_round_trips_through_toml() {
    let original = ArlenRuntimeState {
        autotile: Some(true),
        pinned_workspaces: Some(Vec::new()),
    };
    let serialized = arlen_runtime_serialize(&original).expect("serialize");
    let parsed: ArlenRuntimeState = toml::from_str(&serialized).expect("parse round-trip");
    assert_eq!(parsed.autotile, original.autotile);
    assert_eq!(parsed.pinned_workspaces.as_ref().map(|v| v.len()), Some(0));
}

/// Missing fields stay `None`. The override semantics in
/// `apply_runtime_state_overrides` then leave the TOML value
/// untouched, so a partially-populated state file can never
/// silently revert a TOML setting it doesn't mention.
#[test]
fn runtime_state_missing_fields_use_defaults() {
    let parsed: ArlenRuntimeState = toml::from_str("").expect("empty body parses");
    assert_eq!(
        parsed.autotile, None,
        "missing autotile must be None so TOML wins"
    );
    assert_eq!(
        parsed.pinned_workspaces, None,
        "missing pinned_workspaces must be None so TOML wins"
    );

    // Partial: only autotile present.
    let parsed: ArlenRuntimeState = toml::from_str("autotile = true").expect("partial body parses");
    assert_eq!(parsed.autotile, Some(true));
    assert_eq!(parsed.pinned_workspaces, None);
}

/// `load_runtime_state` returns defaults (all-`None`) when the
/// file is missing. Combined with `apply_runtime_state_overrides`,
/// this means a fresh install never overwrites the user's TOML
/// with hardcoded false/empty defaults.
#[test]
fn load_runtime_state_missing_path_is_default() {
    let state = Config::load_runtime_state(&None);
    assert_eq!(state.autotile, None);
    assert_eq!(state.pinned_workspaces, None);

    // Path that doesn't exist on disk.
    let nonexistent = Some(std::path::PathBuf::from(
        "/tmp/nonexistent-arlen-runtime-state.toml",
    ));
    let state = Config::load_runtime_state(&nonexistent);
    assert_eq!(state.autotile, None);
}

/// `[system_actions]` parses known keys onto the System enum
/// and skips unknown keys with a warning rather than crashing.
#[test]
fn parse_system_actions_known_and_unknown() {
    use shortcuts::action::System;
    let toml: toml::Table = toml::from_str(
        r#"
[system_actions]
VolumeRaise = "spawn:wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%+"
BrightnessUp = "shell:brightness_up"
NotARealAction = "spawn:nope"
"#,
    )
    .unwrap();
    let actions = parse_system_actions(&toml);
    assert_eq!(
        actions.get(&System::VolumeRaise),
        Some(&"spawn:wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%+".to_string())
    );
    assert_eq!(
        actions.get(&System::BrightnessUp),
        Some(&"shell:brightness_up".to_string())
    );
    // Unknown variant is silently dropped (logged warn at runtime).
    assert_eq!(actions.len(), 2);
}

/// Defaults must cover the everyday Fn-row keys (volume,
/// brightness, mute, play/pause). A user with a bare
/// compositor.toml and no `[system_actions]` should still be
/// able to use those keys.
#[test]
fn default_system_actions_covers_fn_row() {
    use shortcuts::action::System;
    let defaults = default_system_actions();
    for action in [
        System::VolumeRaise,
        System::VolumeLower,
        System::Mute,
        System::BrightnessUp,
        System::BrightnessDown,
        System::PlayPause,
        System::PlayNext,
        System::PlayPrev,
    ] {
        assert!(
            defaults.contains_key(&action),
            "default missing for {action:?}"
        );
    }
}

/// Every default system-action value must start with one of
/// the known dispatch prefixes (`shell:` or `spawn:`). The
/// `Action::System` arm in `input/actions.rs` strips these
/// prefixes before dispatch — a bare value would have been
/// passed verbatim to `/bin/sh -c` and silently failed. This
/// is the data-side guard against the regression flagged in
/// compositor #29 review HIGH 1.
#[test]
fn default_system_actions_use_known_prefix() {
    for (action, command) in default_system_actions() {
        assert!(
            command.starts_with("shell:") || command.starts_with("spawn:"),
            "default for {action:?} = {command:?} must use a known prefix \
             (shell: or spawn:); bare strings break system-action dispatch"
        );
    }
}

/// Cosmic-shape Shortcuts is built from our toml_keybindings.
/// Cosmic-`Action` variants flow through, Arlen-private
/// actions (shell:, module:, scratchpad, etc.) are filtered
/// out so only what the cosmic dispatch loops understand ends
/// up in the table.
#[test]
fn build_cosmic_shortcuts_maps_only_cosmic_actions() {
    let kbs = vec![
        KeyBinding {
            modifiers: KeyBindingModifiers {
                super_key: true,
                shift: true,
                ..Default::default()
            },
            key: "q".into(),
            action: "close_window".into(),
        },
        KeyBinding {
            modifiers: KeyBindingModifiers {
                super_key: true,
                ..Default::default()
            },
            key: "space".into(),
            action: "shell:waypointer_open".into(),
        },
        KeyBinding {
            modifiers: KeyBindingModifiers {
                super_key: true,
                ..Default::default()
            },
            key: "F".into(),
            action: "fullscreen".into(),
        },
    ];
    let shortcuts = build_cosmic_shortcuts(&kbs);
    // close_window + fullscreen are cosmic actions → in the table.
    // shell:waypointer_open is Arlen-private → filtered out.
    assert_eq!(shortcuts.0.len(), 2);
    let actions: Vec<_> = shortcuts.0.values().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, shortcuts::Action::Close)),
        "Close action missing from cosmic shortcuts"
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, shortcuts::Action::Fullscreen)),
        "Fullscreen action missing from cosmic shortcuts"
    );
}

/// `format_keybinding` produces the canonical "Super+Shift+Q"
/// shape used in the context-menu shortcut labels.
#[test]
fn format_keybinding_canonical_order() {
    let mods = KeyBindingModifiers {
        super_key: true,
        ctrl: true,
        alt: true,
        shift: true,
    };
    assert_eq!(format_keybinding(&mods, "Q"), "Super+Ctrl+Alt+Shift+Q");

    let mods_super_only = KeyBindingModifiers {
        super_key: true,
        ..Default::default()
    };
    assert_eq!(format_keybinding(&mods_super_only, "Tab"), "Super+Tab");

    let mods_none = KeyBindingModifiers::default();
    assert_eq!(format_keybinding(&mods_none, "F1"), "F1");
}

/// Override semantics: when a TOML key collides with a
/// default, the user's value wins. This mirrors the Right-most
/// merge order in `Config::load`.
#[test]
fn toml_overrides_beat_defaults_via_merge() {
    use shortcuts::action::System;
    let mut merged = default_system_actions();
    let toml: toml::Table = toml::from_str(
        r#"
[system_actions]
BrightnessUp = "spawn:my-custom-brightness-helper"
"#,
    )
    .unwrap();
    let overrides = parse_system_actions(&toml);
    for (k, v) in overrides {
        merged.insert(k, v);
    }
    assert_eq!(
        merged.get(&System::BrightnessUp),
        Some(&"spawn:my-custom-brightness-helper".to_string())
    );
    // Untouched defaults still present.
    assert!(merged.contains_key(&System::VolumeRaise));
}

/// A corrupt state file is removed and replaced with defaults
/// rather than crashing the compositor.
#[test]
fn load_runtime_state_corrupt_file_resets() {
    let dir = std::env::temp_dir().join("arlen-runtime-state-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("corrupt.toml");
    std::fs::write(&path, "not = valid = toml = at = all").unwrap();
    assert!(path.exists(), "test setup");

    let state = Config::load_runtime_state(&Some(path.clone()));
    assert_eq!(state.autotile, None, "fell back to defaults");
    assert!(
        !path.exists(),
        "corrupt file must be removed so the next write starts clean"
    );
}

/// Override precedence: an empty/missing runtime state must NOT
/// overwrite a TOML-configured `autotile = true`. Codex review
/// HIGH 2 — a fresh install used to revert this silently.
#[test]
fn runtime_state_none_does_not_override_toml() {
    let mut cosmic = CosmicCompConfig::default();
    cosmic.autotile = true;

    let runtime = ArlenRuntimeState::default(); // None / None
    apply_runtime_state_overrides(&mut cosmic, &runtime);

    assert!(
        cosmic.autotile,
        "TOML autotile=true must survive an empty runtime state"
    );
}

/// Override precedence: an explicit `Some(_)` in runtime state
/// wins over the TOML value. Mirrors the post-toggle path
/// where the user has actively flipped autotile.
#[test]
fn runtime_state_some_overrides_toml() {
    let mut cosmic = CosmicCompConfig::default();
    cosmic.autotile = false;

    let runtime = ArlenRuntimeState {
        autotile: Some(true),
        pinned_workspaces: None,
    };
    apply_runtime_state_overrides(&mut cosmic, &runtime);

    assert!(
        cosmic.autotile,
        "explicit Some(true) must override the TOML value"
    );
}

/// `terminate` is a name a binding can use. It has no default key, and the
/// locked session refuses it; `dev/lock-terminate-check.sh` drives both.
#[test]
fn terminate_is_bindable_but_unbound() {
    use cosmic_settings_config::shortcuts;
    assert!(matches!(
        super::action_from_str("terminate"),
        Some(super::Action::Shortcut(shortcuts::Action::Terminate))
    ));
    assert!(
        !super::toml_config::default_keybindings()
            .iter()
            .any(|kb| kb.action == "terminate"),
        "ending the session must not be one stray chord away by default"
    );
}

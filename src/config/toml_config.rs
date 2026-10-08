// SPDX-License-Identifier: GPL-3.0-only

//! Arlen's compositor configuration: `~/.config/arlen/compositor.toml`, the
//! keybinding fragments under `compositor.d/`, and the layout and window
//! rules they carry.
//!
//! Upstream reads its configuration through cosmic-config; this fork replaced
//! that loader, and until 8 October the replacement lived inside upstream's own
//! `config/mod.rs` - 900 lines that every weekly merge had to work around, and
//! the file two of the three failed merges named. It lives here now, and
//! `mod.rs` keeps only the calls into it. Nothing in this file changed in the
//! move.

use super::*;

// ── Layout configuration ─────────────────────────────────────────────────────

/// Layout and tiling configuration loaded from `[layout]` in compositor.toml.
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Inner gap between tiled windows (pixels).
    pub inner_gap: i32,
    /// Outer gap between tiled windows and screen edges (pixels).
    pub outer_gap: i32,
    /// When true, no gaps are applied when a workspace has only one tiled window.
    pub smart_gaps: bool,
    /// Render Arlen window-control headers on **single tiled** SSD
    /// windows. Default `false` to match tiling-WM convention
    /// (i3, sway, hyprland) — tiled chrome is dead pixels in a
    /// keyboard-driven layout. Stacks always keep their tab-bar
    /// header regardless of this setting (the tab-bar is
    /// functional UI for window-switching, not pure decoration).
    /// Floating windows always keep their headers regardless.
    pub tiled_headers: bool,
    /// Window rules for float/tile decisions.
    pub window_rules: Vec<WindowRule>,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            inner_gap: 8,
            outer_gap: 8,
            smart_gaps: true,
            tiled_headers: false,
            window_rules: Vec::new(),
        }
    }
}

/// A rule that determines whether a window should float or tile.
#[derive(Debug, Clone)]
pub struct WindowRule {
    pub matcher: WindowMatch,
    pub action: WindowAction,
}

/// Matching criteria for a window rule.
#[derive(Debug, Clone)]
pub struct WindowMatch {
    /// Regex pattern for the app_id. None = match any.
    pub app_id: Option<regex::Regex>,
    /// Regex pattern for the window title. None = match any.
    pub title: Option<regex::Regex>,
    /// Match on window type (e.g. "dialog").
    pub window_type: Option<String>,
}

/// What to do with a matched window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    Float,
    Tile,
}

impl WindowMatch {
    /// Check whether a window matches this rule.
    pub fn matches(&self, app_id: &str, title: &str, is_dialog: bool) -> bool {
        if let Some(ref wt) = self.window_type
            && wt == "dialog"
            && !is_dialog
        {
            return false;
        }
        if let Some(ref re) = self.app_id
            && !re.is_match(app_id)
        {
            return false;
        }
        if let Some(ref re) = self.title
            && !re.is_match(title)
        {
            return false;
        }
        true
    }
}

// ── Keybinding configuration ─────────────────────────────────────────────────

/// A parsed keybinding: modifier set + key -> action string.
#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub modifiers: KeyBindingModifiers,
    pub key: String,
    pub action: String,
}

/// Modifier flags for a keybinding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyBindingModifiers {
    pub super_key: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// Parse a keybinding string like "Super+Shift+H" into modifiers + key.
///
/// Exposed so the dynamic binding resolver can re-use the same grammar
/// for D-Bus-registered bindings without duplicating parsing logic.
pub fn parse_keybinding(binding: &str) -> Option<(KeyBindingModifiers, String)> {
    let mut mods = KeyBindingModifiers::default();
    let parts: Vec<&str> = binding.split('+').collect();
    if parts.is_empty() {
        return None;
    }
    for part in &parts[..parts.len() - 1] {
        match part.to_lowercase().as_str() {
            "super" | "logo" | "mod4" => mods.super_key = true,
            "shift" => mods.shift = true,
            "ctrl" | "control" => mods.ctrl = true,
            "alt" | "mod1" => mods.alt = true,
            _ => {}
        }
    }
    let key = parts.last()?.to_string();
    Some((mods, key))
}

/// Default TOML config path.
pub(super) const DEFAULT_TOML_PATH: &str = ".config/arlen/compositor.toml";

/// Display profile config (output management) lives in a dedicated
/// file under `compositor.d/` so the compositor can rewrite it on
/// every applied output change without disturbing the user-edited
/// `compositor.toml`. See `docs/architecture/display-system.md` §A1.
pub(super) const DISPLAYS_TOML_PATH: &str = ".config/arlen/compositor.d/displays.toml";

/// Resolve the absolute path of the displays-config TOML, or `None`
/// if `$HOME` is unset (which only happens in degenerate test rigs).
pub fn displays_toml_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|home| {
        let mut p = std::path::PathBuf::from(home);
        p.push(DISPLAYS_TOML_PATH);
        p
    })
}

/// Drop-in directory for keybinding fragments written by `installd`
/// on module install. One `*.toml` file per module, each a flat
/// `[keybindings]` table. Loaded alongside the main compositor.toml
/// and fed into the binding resolver at `BindingScope::Module`.
pub(super) const KEYBINDINGS_FRAGMENT_DIR: &str = "compositor.d/keybindings.d";

/// Return the absolute path of the keybinding fragment directory for
/// the given main `compositor.toml` path.
pub fn keybinding_fragment_dir(toml_path: &std::path::Path) -> std::path::PathBuf {
    toml_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(KEYBINDINGS_FRAGMENT_DIR)
}

/// Parsed entry from a keybinding fragment file. `module_id` is the
/// file stem, so two modules can never collide (fs atomicity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentEntry {
    pub module_id: String,
    pub binding: String,
    pub action: String,
}

/// Scan `dir` and return every `"accelerator" = "action"` pair from
/// every `*.toml` file. Missing / malformed files are skipped with a
/// warning — a broken fragment must not crash the compositor.
pub fn load_keybinding_fragments(dir: &std::path::Path) -> Vec<FragmentEntry> {
    if !dir.exists() {
        return Vec::new();
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            tracing::warn!("keybinding fragments: cannot read {}: {err}", dir.display());
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let module_id = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!(
                    "keybinding fragments: read {} failed: {err}",
                    path.display()
                );
                continue;
            }
        };
        let table: toml::Table = match toml::from_str(&content) {
            Ok(t) => t,
            Err(err) => {
                tracing::warn!(
                    "keybinding fragments: parse {} failed: {err}",
                    path.display()
                );
                continue;
            }
        };
        let Some(kb_table) = table.get("keybindings").and_then(|v| v.as_table()) else {
            continue;
        };
        for (binding, action) in kb_table {
            let Some(action_str) = action.as_str() else {
                continue;
            };
            out.push(FragmentEntry {
                module_id: module_id.clone(),
                binding: binding.clone(),
                action: action_str.to_string(),
            });
        }
    }
    tracing::info!(
        "keybinding fragments: loaded {} binding(s) from {}",
        out.len(),
        dir.display(),
    );
    out
}

/// Load CosmicCompConfig from a TOML file, falling back to defaults.
///
/// The user TOML is typically a sparse file with only the fields the
/// user wants to override. We start from defaults and apply overrides
/// for the sections we recognize.
/// Parsed result from the TOML compositor config.
pub(super) struct TomlConfig {
    pub(super) cosmic: CosmicCompConfig,
    pub(super) layout: LayoutConfig,
    pub(super) keybindings: Vec<KeyBinding>,
    /// User overrides for the system-action → spawn-command map.
    /// Merged on top of `default_system_actions()` at config-load
    /// time. Empty when no `[system_actions]` section is present
    /// in compositor.toml.
    pub(super) system_actions: BTreeMap<shortcuts::action::System, String>,
}

pub(super) fn load_toml_config(path: &std::path::Path) -> TomlConfig {
    let default = || TomlConfig {
        cosmic: CosmicCompConfig::default(),
        layout: LayoutConfig::default(),
        keybindings: Vec::new(),
        system_actions: BTreeMap::new(),
    };

    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => {
            tracing::info!("no compositor.toml at {}, using defaults", path.display());
            return default();
        }
    };

    let table: toml::Table = match toml::from_str(&contents) {
        Ok(t) => t,
        Err(err) => {
            warn!(?err, "failed to parse compositor.toml");
            return default();
        }
    };

    let mut config = CosmicCompConfig::default();

    // Apply xkb_config overrides.
    //
    // XKB natively accepts comma-separated layouts and variants in a
    // single string (`"de,us"` + options like `grp:alt_shift_toggle`).
    // We let users write the friendlier TOML `layouts = ["de", "us"]`
    // form too, and fold both into the upstream single-string field
    // `cosmic_comp_config::XkbConfig::layout`. Single-scalar `layout =
    // "de"` keeps working for back-compat; the array form wins when
    // both are present.
    if let Some(xkb) = table.get("xkb_config").and_then(|v| v.as_table()) {
        if let Some(list) = xkb.get("layouts").and_then(|v| v.as_array()) {
            let joined = list
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(",");
            if !joined.is_empty() {
                config.xkb_config.layout = joined;
            }
        } else if let Some(s) = xkb.get("layout").and_then(|v| v.as_str()) {
            config.xkb_config.layout = s.to_string();
        }
        if let Some(s) = xkb.get("model").and_then(|v| v.as_str()) {
            config.xkb_config.model = s.to_string();
        }
        if let Some(list) = xkb.get("variants").and_then(|v| v.as_array()) {
            let joined = list
                .iter()
                .map(|v| v.as_str().unwrap_or(""))
                .collect::<Vec<_>>()
                .join(",");
            // Trailing empty entries (e.g. `["","dvorak"]` → `",dvorak"`)
            // are legitimate — XKB uses position to pair with layouts.
            config.xkb_config.variant = joined;
        } else if let Some(s) = xkb.get("variant").and_then(|v| v.as_str()) {
            config.xkb_config.variant = s.to_string();
        }
        if let Some(s) = xkb.get("options").and_then(|v| v.as_str())
            && !s.is_empty()
        {
            config.xkb_config.options = Some(s.to_string());
        }
        if let Some(n) = xkb.get("repeat_rate").and_then(|v| v.as_integer()) {
            config.xkb_config.repeat_rate = n as u32;
        }
        if let Some(n) = xkb.get("repeat_delay").and_then(|v| v.as_integer()) {
            config.xkb_config.repeat_delay = n as u32;
        }
    }

    // Apply workspace overrides.
    if let Some(ws) = table.get("workspaces").and_then(|v| v.as_table())
        && let Some(s) = ws.get("workspace_layout").and_then(|v| v.as_str())
    {
        config.workspaces.workspace_layout = match s {
            "Vertical" | "vertical" => cosmic_comp_config::workspace::WorkspaceLayout::Vertical,
            _ => cosmic_comp_config::workspace::WorkspaceLayout::Horizontal,
        };
    }

    // Apply mouse overrides (maps to cosmic_conf.input_default).
    parse_mouse_config(&table, &mut config.input_default);
    // Apply touchpad overrides (maps to cosmic_conf.input_touchpad).
    parse_touchpad_config(&table, &mut config.input_touchpad);

    tracing::info!(
        "loaded compositor config from {} (xkb layout={:?})",
        path.display(),
        config.xkb_config.layout,
    );

    // An empty TOML `[xkb_config].layout` is the *expected* default for
    // most users — the `Config::xkb_config()` method will fill it later
    // from the fallback chain ($XKB_DEFAULT_LAYOUT → `localectl` →
    // `/etc/vconsole.conf`). A WARN at this point caused noisy spam
    // every config reload and confused users into thinking their
    // keyboard was broken. Keep it at DEBUG so the resolution path can
    // still be inspected when it's actually needed.
    if config.xkb_config.layout.is_empty() {
        tracing::debug!(
            "no explicit [xkb_config].layout in {} — will use fallback \
             chain (XKB_DEFAULT_LAYOUT → localectl → /etc/vconsole.conf)",
            path.display()
        );
    }

    let layout = parse_layout_config(&table);
    let keybindings = parse_keybindings_config(&table);
    let system_actions = parse_system_actions(&table);

    TomlConfig {
        cosmic: config,
        layout,
        keybindings,
        system_actions,
    }
}

/// Read the `[system_actions]` table from compositor.toml. Keys are
/// `shortcuts::action::System` enum variants serialised as strings
/// (e.g. `"VolumeRaise"`, `"BrightnessDown"`); values are the spawn
/// commands or `shell:` events that fire when the action triggers.
///
/// Unknown keys are warned-and-skipped so a typo can't poison
/// startup. The defaults table from `default_system_actions()` is
/// always present; the user overrides win on conflict.
pub(super) fn parse_system_actions(
    table: &toml::Table,
) -> BTreeMap<shortcuts::action::System, String> {
    let Some(user_table) = table.get("system_actions").and_then(|v| v.as_table()) else {
        return BTreeMap::new();
    };
    let mut out: BTreeMap<shortcuts::action::System, String> = BTreeMap::new();
    for (key, value) in user_table {
        let Some(command) = value.as_str() else {
            warn!(
                key = %key,
                "[system_actions] value must be a string, skipping"
            );
            continue;
        };
        // Round-trip the key string through serde to map it onto the
        // System enum. Unknown variants come back as a deserialise
        // error, which we log as a warning and skip.
        match toml::Value::String(key.clone()).try_into::<shortcuts::action::System>() {
            Ok(action) => {
                out.insert(action, command.to_string());
            }
            Err(err) => {
                warn!(
                    key = %key,
                    ?err,
                    "[system_actions] unknown action name, skipping"
                );
            }
        }
    }
    out
}

/// Overlay any persisted runtime-state overrides onto a freshly
/// parsed `CosmicCompConfig`. Used by both initial load and the
/// `toml_config_changed` hot-reload path so the precedence rule
/// stays uniform — fixes the medium finding from compositor #29
/// review (TOML hot-reload was reverting in-session toggles).
pub(super) fn apply_runtime_state_overrides(
    cosmic_conf: &mut CosmicCompConfig,
    runtime_state: &ArlenRuntimeState,
) {
    if let Some(autotile) = runtime_state.autotile {
        cosmic_conf.autotile = autotile;
    }
    if let Some(pinned) = &runtime_state.pinned_workspaces {
        cosmic_conf.pinned_workspaces = pinned.clone();
    }
}

/// Convert our parsed Arlen keybindings into the cosmic-shape
/// `Shortcuts` table. Several dispatch paths
/// (`input/mod.rs::handle_keyboard_event`, the resize-mode arrow
/// handler, the tiling-swap grab, the resize indicator) iterate
/// `(Binding, Action)` in cosmic terms; populating this from our
/// TOML keeps those code paths working without a wide rewrite.
///
/// Arlen-private actions (`shell:`, `module:`, scratchpad,
/// monocle, etc.) are filtered out here — the cosmic table only
/// holds cosmic-`Action` variants. Private actions still dispatch
/// through the `toml_keybindings` loop in input handling.
pub(super) fn build_cosmic_shortcuts(toml_keybindings: &[KeyBinding]) -> Shortcuts {
    use cosmic_settings_config::shortcuts::{Binding, Modifiers as CosmicModifiers};
    use std::collections::HashMap;

    let mut map: HashMap<Binding, shortcuts::Action> = HashMap::new();
    for kb in toml_keybindings {
        let Some(action) = key_bindings::action_from_str(&kb.action) else {
            continue;
        };
        let cosmic_action = match action {
            key_bindings::Action::Shortcut(a) => a,
            // Arlen-private actions are dispatched directly from
            // the toml_keybindings loop in input/mod.rs.
            key_bindings::Action::Private(_) => continue,
        };
        let mods = CosmicModifiers {
            ctrl: kb.modifiers.ctrl,
            alt: kb.modifiers.alt,
            shift: kb.modifiers.shift,
            logo: kb.modifiers.super_key,
        };
        let key = key_bindings::keysym_from_str(&kb.key);
        let binding = Binding::new(mods, key);
        map.insert(binding, cosmic_action);
    }
    Shortcuts(map)
}

/// Map a cosmic-side `shortcuts::Action` value to the action
/// string used inside Arlen' `compositor.toml [keybindings]`
/// table. Inverse of the prefix-aware mapping in
/// `key_bindings::action_from_str`. Used by `shortcut_for_action`
/// to find the user-visible accelerator label for a context-menu
/// item.
///
/// Variants without a TOML representation (e.g. `ToggleStacking`,
/// which is invoked from the menu but not user-bindable yet) fall
/// through with `None` — the menu code handles `None` by hiding
/// the shortcut label.
pub(super) fn cosmic_action_to_action_string(action: &shortcuts::Action) -> Option<&'static str> {
    use cosmic_settings_config::shortcuts::Action as A;
    Some(match action {
        A::Close => "close_window",
        A::Minimize => "minimize",
        A::Maximize => "maximize",
        A::Fullscreen => "fullscreen",
        A::ToggleWindowFloating => "toggle_window_floating",
        A::ToggleTiling => "toggle_tiling",
        A::SwapWindow => "swap_window",
        A::NextWorkspace => "workspace_next",
        A::PreviousWorkspace => "workspace_prev",
        _ => return None,
    })
}

/// Render a keybinding (modifiers + key) as the
/// "Super+Shift+Q"-style string the context-menu UI expects.
pub(super) fn format_keybinding(mods: &KeyBindingModifiers, key: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if mods.super_key {
        parts.push("Super");
    }
    if mods.ctrl {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    parts.push(key);
    parts.join("+")
}

/// Built-in defaults for the system-action map. These are what
/// users get when they ship a compositor.toml without a
/// `[system_actions]` section, and they replace the prior
/// `cosmic-settings-daemon` IPC inheritance once CC3 lands.
///
/// The commands intentionally use generic CLI tools (`wpctl`,
/// `playerctl`, `loginctl`, `xdg-open`) so the defaults work on a
/// fresh Arlen install without extra wiring. Distros or users
/// can override individual entries via `[system_actions]`.
pub fn default_system_actions() -> BTreeMap<shortcuts::action::System, String> {
    use shortcuts::action::System;
    let mut m = BTreeMap::new();
    m.insert(
        System::VolumeRaise,
        "spawn:wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%+".into(),
    );
    m.insert(
        System::VolumeLower,
        "spawn:wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-".into(),
    );
    m.insert(
        System::Mute,
        "spawn:wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle".into(),
    );
    m.insert(
        System::MuteMic,
        "spawn:wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle".into(),
    );
    // Brightness goes through the shell-overlay protocol so the
    // already-built coalesced step worker (D3.6) handles it; falling
    // back to brightnessctl would bypass the gamma-corrected slider
    // math and the persistence path.
    m.insert(System::BrightnessUp, "shell:brightness_up".into());
    m.insert(System::BrightnessDown, "shell:brightness_down".into());
    m.insert(System::PlayPause, "spawn:playerctl play-pause".into());
    m.insert(System::PlayNext, "spawn:playerctl next".into());
    m.insert(System::PlayPrev, "spawn:playerctl previous".into());
    m.insert(System::LockScreen, "spawn:loginctl lock-session".into());
    m.insert(System::Suspend, "spawn:systemctl suspend".into());
    m.insert(System::PowerOff, "spawn:systemctl poweroff".into());
    m.insert(
        System::LogOut,
        "spawn:loginctl terminate-session $XDG_SESSION_ID".into(),
    );
    m.insert(System::HomeFolder, "spawn:xdg-open ~".into());
    m.insert(System::WebBrowser, "spawn:xdg-open https:".into());
    m.insert(System::Launcher, "shell:waypointer_open".into());
    m.insert(System::AppLibrary, "shell:waypointer_open".into());
    m.insert(System::WindowSwitcher, "shell:workspace_map_open".into());
    m.insert(System::Screenshot, "spawn:grim".into());
    m
}

/// Parse layout configuration from the TOML table.
pub(super) fn parse_layout_config(table: &toml::Table) -> LayoutConfig {
    let mut layout = LayoutConfig::default();

    let Some(section) = table.get("layout").and_then(|v| v.as_table()) else {
        return layout;
    };

    if let Some(n) = section.get("inner_gap").and_then(|v| v.as_integer()) {
        layout.inner_gap = n as i32;
    }
    if let Some(n) = section.get("outer_gap").and_then(|v| v.as_integer()) {
        layout.outer_gap = n as i32;
    }
    if let Some(b) = section.get("smart_gaps").and_then(|v| v.as_bool()) {
        layout.smart_gaps = b;
    }
    if let Some(b) = section.get("tiled_headers").and_then(|v| v.as_bool()) {
        layout.tiled_headers = b;
    }

    // Parse [[layout.window_rules]] array.
    if let Some(rules) = section.get("window_rules").and_then(|v| v.as_array()) {
        for rule_val in rules {
            let Some(rule_table) = rule_val.as_table() else {
                continue;
            };
            let action = match rule_table.get("action").and_then(|v| v.as_str()) {
                Some("float") => WindowAction::Float,
                Some("tile") => WindowAction::Tile,
                _ => continue,
            };
            let matcher = if let Some(m) = rule_table.get("match").and_then(|v| v.as_table()) {
                WindowMatch {
                    app_id: m
                        .get("app_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| regex::Regex::new(s).ok()),
                    title: m
                        .get("title")
                        .and_then(|v| v.as_str())
                        .and_then(|s| regex::Regex::new(s).ok()),
                    window_type: m
                        .get("window_type")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                }
            } else {
                continue;
            };
            layout.window_rules.push(WindowRule { matcher, action });
        }
    }

    tracing::info!(
        "layout config: inner_gap={} outer_gap={} smart_gaps={} \
         tiled_headers={} rules={}",
        layout.inner_gap,
        layout.outer_gap,
        layout.smart_gaps,
        layout.tiled_headers,
        layout.window_rules.len(),
    );

    layout
}

/// Apply `[mouse]` TOML overrides onto the compositor's default pointer config.
///
/// Missing fields leave the current value untouched. An empty or absent
/// `[mouse]` section is a no-op. Values are clamped to libinput's accepted
/// range (acceleration speed -1.0..=1.0).
pub(super) fn parse_mouse_config(table: &toml::Table, input: &mut InputConfig) {
    let Some(section) = table.get("mouse").and_then(|v| v.as_table()) else {
        return;
    };

    if let Some(f) = section.get("acceleration").and_then(|v| v.as_float()) {
        let speed = f.clamp(-1.0, 1.0);
        let mut accel = input.acceleration.clone().unwrap_or(AccelConfig {
            profile: None,
            speed,
        });
        accel.speed = speed;
        input.acceleration = Some(accel);
    }
    if let Some(b) = section.get("natural_scroll").and_then(|v| v.as_bool()) {
        let mut scroll = input.scroll_config.clone().unwrap_or(ScrollConfig {
            method: None,
            natural_scroll: None,
            scroll_button: None,
            scroll_factor: None,
        });
        scroll.natural_scroll = Some(b);
        input.scroll_config = Some(scroll);
    }
    if let Some(b) = section.get("left_handed").and_then(|v| v.as_bool()) {
        input.left_handed = Some(b);
    }
    // `scroll_speed` maps to libinput's `scroll_factor`: a linear
    // multiplier on the per-axis scroll delta. The sensible range is
    // roughly 0.1..3.0 — clamp there so an over-enthusiastic TOML
    // edit doesn't send the page flying on every tick.
    if let Some(f) = section.get("scroll_speed").and_then(|v| v.as_float()) {
        let factor = f.clamp(0.1, 3.0);
        let mut scroll = input.scroll_config.clone().unwrap_or(ScrollConfig {
            method: None,
            natural_scroll: None,
            scroll_button: None,
            scroll_factor: None,
        });
        scroll.scroll_factor = Some(factor);
        input.scroll_config = Some(scroll);
    }

    tracing::info!(
        "mouse config: accel={:?} natural_scroll={:?} left_handed={:?} scroll_factor={:?}",
        input.acceleration.as_ref().map(|a| a.speed),
        input.scroll_config.as_ref().and_then(|s| s.natural_scroll),
        input.left_handed,
        input.scroll_config.as_ref().and_then(|s| s.scroll_factor),
    );
}

/// Apply `[touchpad]` TOML overrides onto the compositor's default touchpad config.
pub(super) fn parse_touchpad_config(table: &toml::Table, input: &mut InputConfig) {
    let Some(section) = table.get("touchpad").and_then(|v| v.as_table()) else {
        return;
    };

    if let Some(b) = section.get("tap_to_click").and_then(|v| v.as_bool()) {
        let mut tap = input.tap_config.clone().unwrap_or(TapConfig {
            enabled: b,
            button_map: None,
            drag: true,
            drag_lock: false,
        });
        tap.enabled = b;
        input.tap_config = Some(tap);
    }
    if let Some(b) = section.get("natural_scroll").and_then(|v| v.as_bool()) {
        let mut scroll = input.scroll_config.clone().unwrap_or(ScrollConfig {
            method: None,
            natural_scroll: None,
            scroll_button: None,
            scroll_factor: None,
        });
        scroll.natural_scroll = Some(b);
        input.scroll_config = Some(scroll);
    }
    if let Some(b) = section.get("two_finger_scroll").and_then(|v| v.as_bool()) {
        let mut scroll = input.scroll_config.clone().unwrap_or(ScrollConfig {
            method: None,
            natural_scroll: None,
            scroll_button: None,
            scroll_factor: None,
        });
        scroll.method = if b {
            Some(ScrollMethod::TwoFinger)
        } else {
            Some(ScrollMethod::NoScroll)
        };
        input.scroll_config = Some(scroll);
    }
    if let Some(b) = section
        .get("disable_while_typing")
        .and_then(|v| v.as_bool())
    {
        input.disable_while_typing = Some(b);
    }
    if let Some(f) = section.get("acceleration").and_then(|v| v.as_float()) {
        let speed = f.clamp(-1.0, 1.0);
        let mut accel = input.acceleration.clone().unwrap_or(AccelConfig {
            profile: None,
            speed,
        });
        accel.speed = speed;
        input.acceleration = Some(accel);
    }
    // Click method: `"clickfinger"` (default) uses finger count to
    // synthesise right/middle click; `"areas"` splits the bottom edge
    // into three hit zones like a physical trackpad.
    if let Some(s) = section.get("click_method").and_then(|v| v.as_str()) {
        input.click_method = match s {
            "areas" | "buttonareas" | "button_areas" => Some(ClickMethod::ButtonAreas),
            "clickfinger" => Some(ClickMethod::Clickfinger),
            other => {
                tracing::warn!(
                    "touchpad.click_method: unknown value {:?} (use 'clickfinger' or 'areas')",
                    other
                );
                None
            }
        };
    }
    // `tap_drag` is part of the `TapConfig` struct that tap_to_click
    // also populates, so we may have already created the config above.
    if let Some(b) = section.get("tap_drag").and_then(|v| v.as_bool()) {
        let mut tap = input.tap_config.clone().unwrap_or(TapConfig {
            enabled: true,
            button_map: None,
            drag: b,
            drag_lock: false,
        });
        tap.drag = b;
        input.tap_config = Some(tap);
    }

    tracing::info!(
        "touchpad config: tap={:?} natural_scroll={:?} dwt={:?} accel={:?} click_method={:?} tap_drag={:?}",
        input.tap_config.as_ref().map(|t| t.enabled),
        input.scroll_config.as_ref().and_then(|s| s.natural_scroll),
        input.disable_while_typing,
        input.acceleration.as_ref().map(|a| a.speed),
        input.click_method,
        input.tap_config.as_ref().map(|t| t.drag),
    );
}

/// Default keybindings used when no `[keybindings]` section is present.
pub fn default_keybindings() -> Vec<KeyBinding> {
    let mut bindings = Vec::new();
    let defaults = [
        // Window / focus / move
        ("Super+T", "toggle_tiling"),
        ("Super+Shift+Space", "toggle_window_floating"),
        ("Super+H", "focus_left"),
        ("Super+J", "focus_down"),
        ("Super+K", "focus_up"),
        ("Super+L", "focus_right"),
        ("Super+Shift+H", "move_left"),
        ("Super+Shift+J", "move_down"),
        ("Super+Shift+K", "move_up"),
        ("Super+Shift+L", "move_right"),
        ("Super+F", "fullscreen"),
        ("Super+Q", "close_window"),
        ("Super+M", "toggle_monocle"),
        ("Super+Minus", "scratchpad_toggle"),
        ("Super+Shift+Minus", "scratchpad_move"),
        // Workspace switch (Super+1..9)
        ("Super+1", "workspace_switch:1"),
        ("Super+2", "workspace_switch:2"),
        ("Super+3", "workspace_switch:3"),
        ("Super+4", "workspace_switch:4"),
        ("Super+5", "workspace_switch:5"),
        ("Super+6", "workspace_switch:6"),
        ("Super+7", "workspace_switch:7"),
        ("Super+8", "workspace_switch:8"),
        ("Super+9", "workspace_switch:9"),
        // Workspace move (Super+Shift+1..9)
        ("Super+Shift+1", "workspace_move:1"),
        ("Super+Shift+2", "workspace_move:2"),
        ("Super+Shift+3", "workspace_move:3"),
        ("Super+Shift+4", "workspace_move:4"),
        ("Super+Shift+5", "workspace_move:5"),
        ("Super+Shift+6", "workspace_move:6"),
        ("Super+Shift+7", "workspace_move:7"),
        ("Super+Shift+8", "workspace_move:8"),
        ("Super+Shift+9", "workspace_move:9"),
        // App launchers and shell
        ("Super+Return", "spawn:foot"),
        ("Super+Space", "shell:waypointer_open"),
        ("Super+Tab", "shell:workspace_map_open"),
        // Hardware brightness keys (laptop Fn-row). The shell-side
        // handler reads the current backlight via logind and
        // steps it ±5 % per press. See `D3.6` in
        // `docs/architecture/display-system.md`.
        ("XF86MonBrightnessUp", "shell:brightness_up"),
        ("XF86MonBrightnessDown", "shell:brightness_down"),
    ];
    for (key_str, action) in defaults {
        if let Some((modifiers, key)) = parse_keybinding(key_str) {
            bindings.push(KeyBinding {
                modifiers,
                key,
                action: action.to_string(),
            });
        }
    }
    bindings
}

/// Parse keybindings from the TOML `[keybindings]` table.
///
/// If no `[keybindings]` section exists, default keybindings are used.
/// If the section exists (even if empty), only the configured bindings
/// are active (explicit override).
pub(super) fn parse_keybindings_config(table: &toml::Table) -> Vec<KeyBinding> {
    let Some(section) = table.get("keybindings").and_then(|v| v.as_table()) else {
        return default_keybindings();
    };

    let mut bindings = Vec::new();
    for (key_str, action_val) in section {
        let Some(action) = action_val.as_str() else {
            continue;
        };
        if let Some((modifiers, key)) = parse_keybinding(key_str) {
            bindings.push(KeyBinding {
                modifiers,
                key,
                action: action.to_string(),
            });
        }
    }

    if !bindings.is_empty() {
        tracing::info!("loaded {} keybindings from TOML", bindings.len());
        for kb in &bindings {
            tracing::info!(
                "  keybinding: {:?}+{:?} -> {:?}",
                kb.modifiers,
                kb.key,
                kb.action,
            );
        }
    } else {
        tracing::info!("no keybindings found in TOML [keybindings] section");
    }

    bindings
}

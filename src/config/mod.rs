// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    shell::Shell,
    state::{BackendData, State},
    utils::prelude::OutputExt,
    wayland::protocols::{
        output_configuration::OutputConfigurationState, workspace::WorkspaceUpdateGuard,
    },
};
use anyhow::Context;
// cosmic_config is still needed for Shortcuts, WindowRules,
// and the legacy cosmic_helper write-back used by zoom.rs.
use cosmic_settings_config::window_rules::ApplicationException;
use cosmic_settings_config::{Shortcuts, shortcuts};
use serde::{Deserialize, Serialize};
use smithay::wayland::xdg_activation::XdgActivationState;
use smithay::{
    backend::input::InputTime,
    utils::{Clock, Monotonic},
};
pub use smithay::{
    backend::input::{self as smithay_input, KeyState},
    input::keyboard::{Keysym, ModifiersState, keysyms as KeySyms},
    output::{Mode, Output},
    reexports::{
        calloop::LoopHandle,
        input::{
            AccelProfile, ClickMethod, Device as InputDevice, ScrollMethod, SendEventsMode,
            TapButtonMap,
        },
    },
    utils::{Logical, Physical, Point, SERIAL_COUNTER, Size, Transform},
};
use std::{
    cell::{Ref, RefCell},
    collections::BTreeMap,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};
use tracing::{error, warn};

pub mod appearance;
mod input_config;
pub mod key_bindings;
mod system_xkb;
mod toml_config;
mod types;

pub use cosmic_comp_config::EdidProduct;
use cosmic_comp_config::{
    CosmicCompConfig, XkbConfig,
    input::{
        AccelConfig, DeviceState as InputDeviceState, InputConfig, ScrollConfig, TapConfig,
        TouchpadOverride,
    },
    output::comp::{OutputConfig, OutputInfo, OutputState, OutputsConfig, TransformDef},
};
pub use key_bindings::{Action, PrivateAction, action_from_str, keysym_from_str};
use system_xkb::*;
pub use toml_config::*;
use types::WlXkbConfig;

#[derive(Debug)]
pub struct Config {
    pub dynamic_conf: DynamicConfig,
    /// Path to the Arlen TOML compositor config file.
    pub toml_path: PathBuf,
    /// Compositor configuration loaded from TOML.
    pub cosmic_conf: CosmicCompConfig,
    /// Cosmic-shape `Shortcuts` table. Populated from
    /// `toml_keybindings` at load time (CC3) — the
    /// `cosmic-settings-daemon` source is no longer consulted.
    /// Kept in cosmic shape because the input-dispatch paths in
    /// `input/mod.rs`, `shell/element/resize_indicator.rs`, the
    /// tiling-swap grab, and the resize-mode arrow handler all
    /// iterate it as `Vec<(Binding, Action)>`.
    pub shortcuts: Shortcuts,
    /// Tiling exceptions. Historically populated from
    /// `com.system76.CosmicSettings.WindowRules`; that source was
    /// dropped in compositor #29 / CC3. The field remains as an
    /// always-empty `Vec` so call sites that already iterate over
    /// it (`shell/mod.rs::TilingExceptions::new`) compile without
    /// change. Arlen-side window-rule semantics live in
    /// `layout.window_rules`.
    pub tiling_exceptions: Vec<ApplicationException>,
    /// Layout and tiling configuration from TOML.
    pub layout: LayoutConfig,
    /// Keybindings from TOML `[keybindings]` section.
    pub toml_keybindings: Vec<KeyBinding>,
    /// System actions sourced from `[system_actions]` in
    /// `compositor.toml`, layered on top of
    /// `default_system_actions()`.
    pub system_actions: BTreeMap<shortcuts::action::System, String>,
    /// Kiosk mode: one application, no shortcuts and no system actions, and
    /// a config reload must not bring them back.
    pub kiosk_mode: bool,
    /// True when running nested inside another Wayland compositor (Winit/X11 backend).
    /// Note: The XKB layout is applied normally even in nested mode because
    /// the compositor receives scancodes (not keysyms) from the host.
    pub nested: bool,
}

#[derive(Debug)]
pub struct DynamicConfig {
    outputs: (Option<PathBuf>, OutputsConfig),
    numlock: (Option<PathBuf>, NumlockStateConfig),
    accessibility_filter: (Option<PathBuf>, ScreenFilter),
    /// Runtime-mutable compositor state (autotile toggle,
    /// pinned-workspace list). Persisted to TOML at
    /// `~/.local/state/arlen/compositor/state.toml` so the
    /// values survive a restart. Read on session start, written
    /// through `runtime_state_mut()` whenever a user action
    /// changes them.
    ///
    /// The previous home for these was `cosmic_helper.set("autotile", _)`
    /// and `set("pinned_workspaces", _)` — the cosmic-config
    /// writeback we are removing as part of compositor #29.
    runtime_state: (Option<PathBuf>, ArlenRuntimeState),
}

#[derive(Default, Debug, Deserialize, Serialize)]
pub struct NumlockStateConfig {
    pub last_state: bool,
}

/// Runtime-mutable compositor state held in a single state file.
///
/// Distinct from `compositor.toml`: that file is user-authored
/// configuration, this one is runtime memory of the last toggle
/// state and the pinned-workspace list. Lives under XDG state, not
/// XDG config, per the file-system-hierarchy convention.
///
/// Fields are `Option` so the precedence rule is unambiguous:
/// `Some(_)` means "the user has explicitly persisted this value
/// since startup, override compositor.toml with it"; `None` means
/// "no override, use whatever compositor.toml says". A missing or
/// freshly-defaulted state file therefore cannot silently revert
/// the user's TOML — fix for compositor #29 review HIGH 2.
#[derive(Default, Debug, Deserialize, Serialize, Clone)]
pub struct ArlenRuntimeState {
    /// Persisted autotile toggle. `None` ⇒ use TOML's value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autotile: Option<bool>,
    /// Persisted pinned-workspace list. `None` ⇒ use TOML's value.
    /// The shape is the cosmic-comp `PinnedWorkspace` type from the
    /// local `cosmic-comp-config` crate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_workspaces: Option<Vec<cosmic_comp_config::workspace::PinnedWorkspace>>,
}

pub struct CompOutputConfig<'a>(pub Ref<'a, OutputConfig>);

impl CompOutputConfig<'_> {
    pub fn mode_size(&self) -> Size<i32, Physical> {
        self.0.mode.0.into()
    }

    pub fn mode_refresh(&self) -> u32 {
        self.0.mode.1.unwrap_or(60_000)
    }

    pub fn transformed_size(&self) -> Size<i32, Physical> {
        self.transform().transform_size(self.mode_size())
    }

    pub fn output_mode(&self) -> Mode {
        Mode {
            size: self.mode_size(),
            refresh: self.mode_refresh() as i32,
        }
    }

    pub fn transform(&self) -> Transform {
        Transform::from(CompTransformDef(self.0.transform))
    }
}

pub struct CompTransformDef(pub TransformDef);

impl From<Transform> for CompTransformDef {
    fn from(transform: Transform) -> Self {
        let def = match transform {
            Transform::Normal => TransformDef::Normal,
            Transform::_90 => TransformDef::_90,
            Transform::_180 => TransformDef::_180,
            Transform::_270 => TransformDef::_270,
            Transform::Flipped => TransformDef::Flipped,
            Transform::Flipped90 => TransformDef::Flipped90,
            Transform::Flipped180 => TransformDef::Flipped180,
            Transform::Flipped270 => TransformDef::Flipped270,
        };
        CompTransformDef(def)
    }
}

impl From<CompTransformDef> for Transform {
    fn from(comp_transform: CompTransformDef) -> Self {
        match comp_transform.0 {
            TransformDef::Normal => Transform::Normal,
            TransformDef::_90 => Transform::_90,
            TransformDef::_180 => Transform::_180,
            TransformDef::_270 => Transform::_270,
            TransformDef::Flipped => Transform::Flipped,
            TransformDef::Flipped90 => Transform::Flipped90,
            TransformDef::Flipped180 => Transform::Flipped180,
            TransformDef::Flipped270 => Transform::Flipped270,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct ScreenFilter {
    pub inverted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_filter: Option<ColorFilter>,
    /// Night-light multiplicative tint as `(r, g, b)` ratios in
    /// `[0.0, 1.0]`. `None` means "no tint" (= identity / 6500K).
    /// Applied as a final multiply in `offscreen.frag` so it works
    /// on every backend, not just KMS hardware-gamma. Skipped from
    /// serialization because this is computed live from the night-
    /// light state, not a persisted preference.
    #[serde(skip)]
    pub night_light_tint: Option<[f32; 3]>,
}

impl ScreenFilter {
    pub fn is_noop(&self) -> bool {
        !self.inverted && self.color_filter.is_none() && self.night_light_tint.is_none()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
// these values need to match with offscreen.frag
pub enum ColorFilter {
    Greyscale = 1,
    Protanopia = 2,
    Deuteranopia = 3,
    Tritanopia = 4,
}

impl Config {
    pub fn load(loop_handle: &LoopHandle<'_, State>, kiosk_mode: bool) -> Config {
        let xdg = xdg::BaseDirectories::new();

        // Load compositor config from TOML.
        let toml_path = std::env::var("ARLEN_COMPOSITOR_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
                PathBuf::from(home).join(DEFAULT_TOML_PATH)
            });

        let toml_config = load_toml_config(&toml_path);
        let cosmic_comp_config = toml_config.cosmic;
        let layout_config = toml_config.layout;
        // Kiosk mode runs a single application and takes no shortcuts at all,
        // which is upstream's rule for it: with no bindings there is no key
        // that can leave the kiosk.
        let toml_keybindings = if kiosk_mode {
            Vec::new()
        } else {
            toml_config.keybindings
        };

        // Watch the TOML config file for changes via notify.
        if let Some(parent) = toml_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        {
            let watch_path = toml_path.clone();
            let (notify_tx, notify_rx) = calloop::channel::channel::<()>();
            let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
                if let Ok(event) = res {
                    // Editors do atomic writes (rename), so watch for Create and Modify.
                    use notify::EventKind;
                    if matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    ) {
                        let _ = notify_tx.send(());
                    }
                }
            })
            .expect("failed to create config file watcher");

            // Watch the parent directory (editors do atomic renames).
            if let Some(parent) = watch_path.parent() {
                use notify::Watcher;
                watcher
                    .watch(parent, notify::RecursiveMode::NonRecursive)
                    .expect("failed to watch config directory");
            }

            // Also watch the keybinding fragment directory so module
            // installs / uninstalls propagate without restarting the
            // compositor. Best-effort: if the directory does not exist
            // yet (no module has been installed), skip — `installd`
            // creates it on the first write and the next reload picks
            // it up.
            {
                let fragment_dir = keybinding_fragment_dir(&watch_path);
                if fragment_dir.exists() {
                    use notify::Watcher;
                    if let Err(err) =
                        watcher.watch(&fragment_dir, notify::RecursiveMode::NonRecursive)
                    {
                        tracing::warn!(
                            "could not watch keybinding fragment dir {}: {err}",
                            fragment_dir.display()
                        );
                    }
                }
            }

            let toml_path_for_handler = watch_path;
            loop_handle
                .insert_source(notify_rx, move |_, _, state| {
                    toml_config_changed(&toml_path_for_handler, state);
                })
                .expect("failed to add config watcher to event loop");

            // Leak the watcher so it stays alive for the process lifetime.
            // Same pattern as theme.rs (std::mem::forget on watchers).
            std::mem::forget(watcher);
        }

        // Build the system-actions map from defaults plus the
        // user's `[system_actions]` overrides. The
        // cosmic-settings-daemon source was removed in
        // compositor #29 / CC3 — Arlen no longer inherits
        // anything from `com.system76.CosmicSettings.*`.
        let mut system_actions = if kiosk_mode {
            BTreeMap::new()
        } else {
            default_system_actions()
        };
        if !kiosk_mode {
            for (action, command) in toml_config.system_actions.clone() {
                system_actions.insert(action, command);
            }
        }

        // Build the cosmic-shape Shortcuts table from our
        // toml_keybindings list. The dispatch loops in
        // input/mod.rs and the tiling/resize indicators iterate
        // this in cosmic-Action terms; populating it locally
        // keeps those paths functional while the cosmic-settings
        // source disappears.
        let shortcuts = build_cosmic_shortcuts(&toml_keybindings);

        // tiling_exceptions formerly populated from
        // `com.system76.CosmicSettings.WindowRules`. That source is
        // gone — Arlen-side window rules live in
        // `compositor.toml [layout].window_rules` and flow through
        // the `Config.layout.window_rules` field instead. We keep
        // this Vec empty so `shell::TilingExceptions::new` and
        // friends still compile.
        let tiling_exceptions: Vec<ApplicationException> = Vec::new();

        let _ = loop_handle.insert_idle(|state| {
            let filter_conf = state.common.config.dynamic_conf.screen_filter();
            state
                .common
                .a11y_state
                .set_screen_inverted(filter_conf.inverted);
            state
                .common
                .a11y_state
                .set_screen_filter(filter_conf.color_filter);
        });

        // Load runtime state, then merge any explicit overrides
        // into `cosmic_comp_config`. Precedence rule (compositor
        // #29 review HIGH 2): runtime state only wins when the
        // user has actually persisted the field (`Some(_)`); a
        // missing or default state file leaves the TOML value
        // intact.
        let dynamic_conf = Self::load_dynamic(&xdg);
        let mut cosmic_comp_config = cosmic_comp_config;
        apply_runtime_state_overrides(&mut cosmic_comp_config, dynamic_conf.runtime_state());

        Config {
            dynamic_conf,
            cosmic_conf: cosmic_comp_config,
            toml_path,
            shortcuts,
            tiling_exceptions,
            layout: layout_config,
            toml_keybindings,
            system_actions,
            kiosk_mode,
            nested: false,
        }
    }

    fn load_dynamic(xdg: &xdg::BaseDirectories) -> DynamicConfig {
        // Output config moved from `~/.local/state/cosmic-comp/outputs.ron`
        // to `~/.config/arlen/compositor.d/displays.toml`. See
        // `docs/architecture/display-system.md` §A1. On first boot
        // after the change, the legacy RON is converted to TOML and
        // unlinked. The legacy path keeps working for `numlock` and
        // `a11y_screen_filter` because those are dev-time state with
        // no settings UI yet.
        let displays_path = displays_toml_path();
        if let Some(toml_path) = displays_path.as_ref()
            && let Ok(legacy_ron) = xdg.place_state_file("cosmic-comp/outputs.ron")
        {
            cosmic_comp_config::output::displays_toml::migrate_from_ron(&legacy_ron, toml_path);
        }
        let outputs = displays_path
            .as_deref()
            .map(cosmic_comp_config::output::displays_toml::load)
            .unwrap_or_else(|| OutputsConfig {
                config: Default::default(),
            });

        let numlock_path = xdg.place_state_file("cosmic-comp/numlock.ron").ok();
        let numlock = Self::load_numlock(&numlock_path);

        let filter_path = xdg
            .place_state_file("cosmic-comp/a11y_screen_filter.ron")
            .ok();
        let filter = Self::load_filter_state(&filter_path);

        let runtime_state_path = xdg.place_state_file("arlen/compositor/state.toml").ok();
        let runtime_state = Self::load_runtime_state(&runtime_state_path);

        DynamicConfig {
            outputs: (displays_path, outputs),
            numlock: (numlock_path, numlock),
            accessibility_filter: (filter_path, filter),
            runtime_state: (runtime_state_path, runtime_state),
        }
    }

    /// Read `~/.local/state/arlen/compositor/state.toml`. Missing
    /// or unparseable file returns defaults — this state can always
    /// be regenerated from the next user toggle, so a corrupt file
    /// is logged-and-replaced rather than fatal.
    fn load_runtime_state(path: &Option<PathBuf>) -> ArlenRuntimeState {
        let Some(path) = path.as_deref() else {
            return ArlenRuntimeState::default();
        };
        if !path.exists() {
            return ArlenRuntimeState::default();
        }
        match std::fs::read_to_string(path) {
            Ok(content) => match toml::from_str::<ArlenRuntimeState>(&content) {
                Ok(state) => state,
                Err(err) => {
                    warn!(
                        ?err,
                        "Failed to parse arlen compositor runtime state, resetting.."
                    );
                    let _ = std::fs::remove_file(path);
                    ArlenRuntimeState::default()
                }
            },
            Err(err) => {
                warn!(?err, "Failed to read arlen compositor runtime state");
                ArlenRuntimeState::default()
            }
        }
    }

    fn load_numlock(path: &Option<PathBuf>) -> NumlockStateConfig {
        path.as_deref()
            .filter(|path| path.exists())
            .and_then(|path| {
                ron::de::from_reader::<_, NumlockStateConfig>(
                    OpenOptions::new().read(true).open(path).unwrap(),
                )
                .map_err(|err| {
                    warn!(?err, "Failed to read numlock.ron, resetting..");
                    if let Err(err) = std::fs::remove_file(path) {
                        error!(?err, "Failed to remove numlock.ron.");
                    }
                })
                .ok()
            })
            .unwrap_or_default()
    }

    fn load_filter_state(path: &Option<PathBuf>) -> ScreenFilter {
        if let Some(path) = path.as_ref()
            && path.exists()
        {
            match ron::de::from_reader::<_, ScreenFilter>(
                OpenOptions::new().read(true).open(path).unwrap(),
            ) {
                Ok(config) => return config,
                Err(err) => {
                    warn!(?err, "Failed to read screen_filter state, resetting..");
                    if let Err(err) = std::fs::remove_file(path) {
                        error!(?err, "Failed to remove screen_filter state.");
                    }
                }
            };
        }

        ScreenFilter {
            inverted: false,
            color_filter: None,
            night_light_tint: None,
        }
    }

    pub fn shortcut_for_action(&self, action: &shortcuts::Action) -> Option<String> {
        let action_str = cosmic_action_to_action_string(action)?;
        self.toml_keybindings
            .iter()
            .find(|kb| kb.action == action_str)
            .map(|kb| format_keybinding(&kb.modifiers, &kb.key))
    }

    pub fn read_outputs(
        &mut self,
        output_state: &mut OutputConfigurationState<State>,
        backend: &mut BackendData,
        shell: &Arc<parking_lot::RwLock<Shell>>,
        loop_handle: &LoopHandle<'static, State>,
        workspace_state: &mut WorkspaceUpdateGuard<'_, State>,
        xdg_activation_state: &XdgActivationState,
        startup_done: Arc<AtomicBool>,
        clock: &Clock<Monotonic>,
    ) -> anyhow::Result<()> {
        let outputs = output_state.outputs().collect::<Vec<_>>();
        let mut infos = outputs
            .iter()
            .cloned()
            .map(Into::<crate::config::CompOutputInfo>::into)
            .map(|i| i.0)
            .collect::<Vec<_>>();
        infos.sort();

        if let Some(configs) = self
            .dynamic_conf
            .outputs()
            .config
            .get(&infos)
            .filter(|configs| {
                if configs
                    .iter()
                    .all(|config| config.enabled == OutputState::Disabled)
                {
                    if !configs.is_empty() {
                        error!(
                            "Broken config, all outputs disabled. Resetting... {:?}",
                            configs
                        );
                    }
                    false
                } else {
                    true
                }
            })
            .cloned()
        {
            let known_good_configs = outputs
                .iter()
                .map(|output| {
                    output
                        .user_data()
                        .get::<RefCell<OutputConfig>>()
                        .unwrap()
                        .borrow()
                        .clone()
                })
                .collect::<Vec<_>>();

            let mut found_outputs = Vec::new();
            for (name, output_config) in infos.iter().map(|o| &o.connector).zip(configs) {
                let output = outputs.iter().find(|o| &o.name() == name).unwrap().clone();
                let enabled = output_config.enabled.clone();
                *output
                    .user_data()
                    .get::<RefCell<OutputConfig>>()
                    .unwrap()
                    .borrow_mut() = output_config;
                found_outputs.push((output.clone(), enabled));
            }

            let mut backend = backend.lock();
            if let Err(err) = backend.apply_config_for_outputs(
                false,
                loop_handle,
                self.dynamic_conf.screen_filter(),
                shell.clone(),
                workspace_state,
                xdg_activation_state,
                startup_done.clone(),
                clock,
            ) {
                warn!(?err, "Failed to set new config.");
                found_outputs.clear();
                for (output, output_config) in outputs.clone().into_iter().zip(known_good_configs) {
                    let enabled = output_config.enabled.clone();
                    *output
                        .user_data()
                        .get::<RefCell<OutputConfig>>()
                        .unwrap()
                        .borrow_mut() = output_config;
                    found_outputs.push((output.clone(), enabled));
                }

                backend
                    .apply_config_for_outputs(
                        false,
                        loop_handle,
                        self.dynamic_conf.screen_filter(),
                        shell.clone(),
                        workspace_state,
                        xdg_activation_state,
                        startup_done,
                        clock,
                    )
                    .context("Failed to reset config")?;

                for (output, enabled) in found_outputs {
                    if enabled == OutputState::Enabled {
                        output_state.enable_head(&output);
                    } else {
                        output_state.disable_head(&output);
                    }
                }
            } else {
                for (output, enabled) in found_outputs {
                    if enabled == OutputState::Enabled {
                        output_state.enable_head(&output);
                    } else {
                        output_state.disable_head(&output);
                    }
                }
            }

            output_state.update();
            self.write_outputs(output_state.outputs());
        } else {
            if outputs
                .iter()
                .all(|o| o.config().enabled == OutputState::Disabled)
            {
                for output in &outputs {
                    output.config_mut().enabled = OutputState::Enabled;
                }
            }

            // we don't have a config, so lets generate somewhat sane positions
            let mut w = 0;
            if !outputs.iter().any(|o| o.config().xwayland_primary) {
                // if we don't have a primary output for xwayland from a previous config, pick one
                if let Some(primary) = outputs.iter().find(|o| o.mirroring().is_none()) {
                    primary.config_mut().xwayland_primary = true;
                }
            }
            // sort by connector name for a deterministic layout independent of hotplug order
            let mut sorted_outputs = outputs
                .iter()
                .filter(|o| o.mirroring().is_none())
                .collect::<Vec<_>>();
            sorted_outputs.sort_by_key(|o| o.name());
            for output in sorted_outputs {
                {
                    let mut config = output.config_mut();
                    config.position = (w, 0);
                }
                w += output.geometry().size.w as u32;
            }

            let mut backend = backend.lock();
            backend
                .apply_config_for_outputs(
                    false,
                    loop_handle,
                    self.dynamic_conf.screen_filter(),
                    shell.clone(),
                    workspace_state,
                    xdg_activation_state,
                    startup_done.clone(),
                    clock,
                )
                .context("Failed to set new config")?;

            for output in outputs {
                if output
                    .user_data()
                    .get::<RefCell<OutputConfig>>()
                    .unwrap()
                    .borrow()
                    .enabled
                    == OutputState::Enabled
                {
                    output_state.enable_head(&output);
                } else {
                    output_state.disable_head(&output);
                }
            }
            output_state.update();
            self.write_outputs(output_state.outputs());
        }

        Ok(())
    }

    pub fn write_outputs(
        &mut self,
        outputs: impl Iterator<Item = impl std::borrow::Borrow<Output>>,
    ) {
        let mut infos = outputs
            .map(|o| {
                let o = o.borrow();
                (
                    Into::<CompOutputInfo>::into(o.clone()).0,
                    o.user_data()
                        .get::<RefCell<OutputConfig>>()
                        .unwrap()
                        .borrow()
                        .clone(),
                )
            })
            .collect::<Vec<(OutputInfo, OutputConfig)>>();
        infos.sort_by(|(a, _), (b, _)| a.cmp(b));
        let (infos, configs) = infos.into_iter().unzip();
        self.dynamic_conf
            .outputs_mut()
            .config
            .insert(infos, configs);
    }

    pub fn xkb_config(&self) -> XkbConfig {
        let mut cfg = self.cosmic_conf.xkb_config.clone();
        // If the layout is empty (no cosmic-config or TOML config set it),
        // fall back to environment variables, then /etc/vconsole.conf.
        if cfg.layout.is_empty()
            && let Ok(layout) = std::env::var("XKB_DEFAULT_LAYOUT")
        {
            cfg.layout = layout;
        }
        if cfg.variant.is_empty()
            && let Ok(variant) = std::env::var("XKB_DEFAULT_VARIANT")
        {
            cfg.variant = variant;
        }
        if cfg.model.is_empty()
            && let Ok(model) = std::env::var("XKB_DEFAULT_MODEL")
        {
            cfg.model = model;
        }
        if cfg.rules.is_empty()
            && let Ok(rules) = std::env::var("XKB_DEFAULT_RULES")
        {
            cfg.rules = rules;
        }
        if cfg.options.is_none()
            && let Ok(options) = std::env::var("XKB_DEFAULT_OPTIONS")
            && !options.is_empty()
        {
            cfg.options = Some(options);
        }
        // If still empty, try localectl (systemd) which reads the full X11 config.
        if cfg.layout.is_empty() {
            let system = read_system_xkb_layout();
            if !system.layout.is_empty() {
                cfg.layout = system.layout;
            }
            if cfg.variant.is_empty() && !system.variant.is_empty() {
                cfg.variant = system.variant;
            }
            if cfg.model.is_empty() && !system.model.is_empty() {
                cfg.model = system.model;
            }
            if cfg.options.is_none() && !system.options.is_empty() {
                cfg.options = Some(system.options);
            }
        }
        // Last resort: /etc/vconsole.conf KEYMAP field.
        if cfg.layout.is_empty()
            && let Some(vconsole) = parse_vconsole_keymap()
        {
            cfg.layout = vconsole;
        }
        // If even the last-resort chain didn't find anything, the user
        // really has no resolvable XKB layout — warn once per process
        // so keyboard issues have a log trail instead of failing
        // silently. Noisy repeats are suppressed via `XKB_WARNED`.
        if cfg.layout.is_empty() {
            use std::sync::atomic::{AtomicBool, Ordering};
            static XKB_WARNED: AtomicBool = AtomicBool::new(false);
            if !XKB_WARNED.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    "unable to resolve XKB layout: no TOML [xkb_config], \
                     no XKB_DEFAULT_LAYOUT env, no `localectl` entry, \
                     no /etc/vconsole.conf KEYMAP — keyboard input may \
                     fall back to the XKB built-in default"
                );
            }
        } else {
            tracing::debug!(
                "xkb layout resolved: layout={:?} variant={:?} options={:?}",
                cfg.layout,
                cfg.variant,
                cfg.options
            );
        }
        cfg
    }

    pub fn read_device(&self, device: &mut InputDevice) {
        let (device_config, default_config) = self.get_device_config(device);
        input_config::update_device(device, device_config.as_ref(), default_config);
    }

    pub fn scroll_factor(&self, device: &InputDevice) -> f64 {
        let (device_config, default_config) = self.get_device_config(device);
        input_config::get_config(device_config.as_ref(), default_config, |x| {
            x.scroll_config.as_ref()?.scroll_factor
        })
        .map_or(1.0, |x| x.0)
    }

    pub fn map_to_output(&self, device: &InputDevice) -> Option<String> {
        let (device_config, default_config) = self.get_device_config(device);
        Some(
            input_config::get_config(device_config.as_ref(), default_config, |x| {
                x.map_to_output.clone()
            })?
            .0,
        )
    }

    fn get_device_config(&self, device: &InputDevice) -> (Option<InputConfig>, &InputConfig) {
        let is_touchpad = device.config_tap_finger_count() > 0;

        let default_config = if is_touchpad {
            &self.cosmic_conf.input_touchpad
        } else {
            &self.cosmic_conf.input_default
        };

        let mut device_config = self.cosmic_conf.input_devices.get(&*device.name()).cloned();
        if is_touchpad && self.cosmic_conf.input_touchpad_override == TouchpadOverride::ForceDisable
        {
            device_config = Some({
                let mut config = device_config.unwrap_or_default();
                config.state = InputDeviceState::Disabled;
                config
            });
        }

        (device_config, default_config)
    }
}

/// Per-type serializer callback. The original `PersistenceGuard`
/// hardcoded RON for everything; with `outputs.ron` migrating to
/// TOML the format now varies per dynamic-state file. The callback
/// closes over the value and produces the on-disk text.
type Serializer<T> = fn(&T) -> Result<String, String>;

fn ron_serialize<T: Serialize>(value: &T) -> Result<String, String> {
    ron::ser::to_string_pretty(value, Default::default()).map_err(|e| e.to_string())
}

fn outputs_toml_serialize(value: &OutputsConfig) -> Result<String, String> {
    cosmic_comp_config::output::displays_toml::to_toml_string(value)
}

pub struct PersistenceGuard<'a, T: Serialize> {
    path: Option<PathBuf>,
    value: &'a mut T,
    serialize: Serializer<T>,
}

impl<T: Serialize> std::ops::Deref for PersistenceGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value
    }
}

impl<T: Serialize> std::ops::DerefMut for PersistenceGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value
    }
}

impl<T: Serialize> Drop for PersistenceGuard<'_, T> {
    fn drop(&mut self) {
        if let Some(path) = self.path.as_ref() {
            let content = match (self.serialize)(self.value) {
                Ok(content) => content,
                Err(err) => {
                    warn!("Failed to serialize: {err}");
                    return;
                }
            };

            // Make sure the parent directory exists. The TOML
            // outputs path lives under `~/.config/arlen/compositor.d/`
            // which has no other guaranteed creator.
            let Some(parent) = path.parent() else {
                warn!(
                    "PersistenceGuard target path has no parent: {}",
                    path.display()
                );
                return;
            };
            if let Err(err) = std::fs::create_dir_all(parent) {
                warn!(?err, "Failed to create parent dir for {}", path.display());
                return;
            }

            // Atomic write: write to `<file>.tmp`, fsync, rename over
            // the target. Without this a process kill between
            // truncate and write_all leaves the file half-formed —
            // and for `outputs`, that file is the only authoritative
            // copy of the user's display layout.
            let tmp_path = path.with_extension(
                path.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| format!("{e}.tmp"))
                    .unwrap_or_else(|| "tmp".into()),
            );
            let write_result = (|| -> std::io::Result<()> {
                let mut writer = OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(&tmp_path)?;
                writer.write_all(content.as_bytes())?;
                writer.flush()?;
                writer.sync_all()?;
                std::fs::rename(&tmp_path, path)?;
                Ok(())
            })();

            if let Err(err) = write_result {
                warn!(?err, "Failed to persist {} atomically.", path.display());
                let _ = std::fs::remove_file(&tmp_path);
                return;
            }

            // Best-effort directory fsync so the rename itself is
            // durable. Failure is non-fatal.
            if let Ok(dir) = std::fs::File::open(parent) {
                let _ = dir.sync_all();
            }
        }
    }
}

impl DynamicConfig {
    pub fn outputs(&self) -> &OutputsConfig {
        &self.outputs.1
    }

    pub fn outputs_mut(&mut self) -> PersistenceGuard<'_, OutputsConfig> {
        PersistenceGuard {
            path: self.outputs.0.clone(),
            value: &mut self.outputs.1,
            serialize: outputs_toml_serialize,
        }
    }

    pub fn numlock(&self) -> &NumlockStateConfig {
        &self.numlock.1
    }

    pub fn numlock_mut(&mut self) -> PersistenceGuard<'_, NumlockStateConfig> {
        PersistenceGuard {
            path: self.numlock.0.clone(),
            value: &mut self.numlock.1,
            serialize: ron_serialize,
        }
    }

    pub fn screen_filter(&self) -> &ScreenFilter {
        &self.accessibility_filter.1
    }

    pub fn screen_filter_mut(&mut self) -> PersistenceGuard<'_, ScreenFilter> {
        PersistenceGuard {
            path: self.accessibility_filter.0.clone(),
            value: &mut self.accessibility_filter.1,
            serialize: ron_serialize,
        }
    }

    pub fn runtime_state(&self) -> &ArlenRuntimeState {
        &self.runtime_state.1
    }

    /// Mutable handle to the runtime-state file. The returned
    /// guard writes back to `state.toml` atomically (tmp + rename)
    /// when dropped. Use for any field on `ArlenRuntimeState`
    /// that needs to persist across sessions.
    pub fn runtime_state_mut(&mut self) -> PersistenceGuard<'_, ArlenRuntimeState> {
        PersistenceGuard {
            path: self.runtime_state.0.clone(),
            value: &mut self.runtime_state.1,
            serialize: arlen_runtime_serialize,
        }
    }
}

fn arlen_runtime_serialize(value: &ArlenRuntimeState) -> Result<String, String> {
    toml::to_string_pretty(value).map_err(|e| e.to_string())
}

pub fn xkb_config_to_wl(config: &XkbConfig) -> WlXkbConfig<'_> {
    WlXkbConfig {
        rules: &config.rules,
        model: &config.model,
        layout: &config.layout,
        variant: &config.variant,
        options: config.options.clone(),
    }
}

fn update_input(state: &mut State) {
    if let BackendData::Kms(kms_state) = &mut state.backend {
        for device in kms_state.input_devices.values_mut() {
            state.common.config.read_device(device);
        }
    }
}

pub fn change_modifier_state(
    keyboard: &smithay::input::keyboard::KeyboardHandle<State>,
    scan_code: u32,
    state: &mut State,
) {
    /// Offset used to convert Linux scancode to X11 keycode.
    const X11_KEYCODE_OFFSET: u32 = 8;

    let mut input = |key_state, scan_code| {
        let _ = keyboard.input(
            state,
            smithay_input::Keycode::new(scan_code + X11_KEYCODE_OFFSET),
            key_state,
            SERIAL_COUNTER.next_serial(),
            InputTime::now(),
            |_, _, _| smithay::input::keyboard::FilterResult::<()>::Forward,
        );
    };

    input(smithay_input::KeyState::Pressed, scan_code);
    input(smithay_input::KeyState::Released, scan_code);
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CompOutputInfo(OutputInfo);

impl From<Output> for CompOutputInfo {
    fn from(o: Output) -> CompOutputInfo {
        let physical = o.physical_properties();
        CompOutputInfo(OutputInfo {
            connector: o.name(),
            make: physical.make,
            model: physical.model,
        })
    }
}

#[cfg(test)]
mod tests;

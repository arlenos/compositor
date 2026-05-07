//! Lunaris theme integration for the compositor.
//!
//! Resolves `LunarisTheme` from the canonical bundled bytes
//! (`include_str!` cross-crate from desktop-shell/src-tauri/themes/)
//! merged with the user's `~/.config/lunaris/theme.toml` overlay,
//! then layered with `~/.config/lunaris/appearance.toml`
//! preferences (active theme id, accent override, radius
//! intensity, accessibility).
//!
//! The resolved theme is held in a process-wide `RwLock` and re-
//! resolved by the file watcher on any change to either of the
//! source files.
//!
//! See `docs/architecture/theme-system.md` for the SSoT layering
//! contract — this file is the compositor side of that contract.

use calloop::LoopHandle;
use std::sync::RwLock;

use crate::state::State;

/// Bundled bytes — same files the desktop-shell embeds so the two
/// binaries observe identical canonical defaults. Cross-crate
/// `include_str!` is the SSoT mechanism: a refactor that moves
/// the files breaks compile in BOTH crates immediately.
const DARK_TOML: &str =
    include_str!("../../desktop-shell/src-tauri/themes/dark.toml");
const LIGHT_TOML: &str =
    include_str!("../../desktop-shell/src-tauri/themes/light.toml");

static LUNARIS_THEME: RwLock<Option<lunaris_theme::LunarisTheme>> =
    RwLock::new(None);

/// Read the global LunarisTheme. Falls back to a freshly-resolved
/// dark theme if the watcher hasn't run yet (early startup
/// frames).
pub fn lunaris_theme() -> lunaris_theme::LunarisTheme {
    LUNARIS_THEME
        .read()
        .unwrap()
        .clone()
        .unwrap_or_else(default_dark_theme)
}

fn default_dark_theme() -> lunaris_theme::LunarisTheme {
    lunaris_theme::LunarisTheme::from_bundled(DARK_TOML)
        .expect("bundled dark.toml must parse — bundled bytes are static")
}

fn default_light_theme() -> lunaris_theme::LunarisTheme {
    lunaris_theme::LunarisTheme::from_bundled(LIGHT_TOML)
        .expect("bundled light.toml must parse — bundled bytes are static")
}

fn set_lunaris_theme(theme: lunaris_theme::LunarisTheme) {
    *LUNARIS_THEME.write().unwrap() = Some(theme);
}

/// Public setter used by the appearance watcher after composing
/// the effective theme. Kept distinct from the private setter so
/// the call-site intent is explicit.
pub fn replace_lunaris_theme(theme: lunaris_theme::LunarisTheme) {
    set_lunaris_theme(theme);
}

/// Active window hint color from LunarisTheme as `[r, g, b]`.
/// Falls back to the theme's accent if `[wm].window_hint` is unset.
pub(crate) fn lunaris_hint_rgb(lt: &lunaris_theme::LunarisTheme) -> [f32; 3] {
    if let Some(hint) = lt.wm.window_hint {
        [hint[0], hint[1], hint[2]]
    } else {
        lt.accent_rgb()
    }
}

/// Compose the effective theme from bundled bytes + user
/// `theme.toml` + `appearance.toml`. Falls back to bundled
/// defaults when the user files fail to parse — used at
/// startup, where no last-good theme exists yet.
///
/// **For runtime reload (file-watcher path) use
/// `try_recompose_effective_theme` instead** — the watcher must
/// keep the previous good theme on parse error rather than
/// painting the bundled default across every output.
pub fn recompose_effective_theme() -> lunaris_theme::LunarisTheme {
    try_recompose_effective_theme().unwrap_or_else(|err| {
        tracing::warn!(
            "theme: initial compose failed ({err}); using bundled dark default"
        );
        default_dark_theme()
    })
}

/// Like `recompose_effective_theme` but returns `Err` on parse
/// failure instead of falling back. Used by the file watcher so
/// a transiently-invalid save (mid-typing in editor, atomic-rename
/// caught between writes) doesn't blank the desktop's theme.
///
/// On Err, the caller should keep the previously-published global
/// theme and skip render scheduling — the next successful save
/// fires the watcher again.
pub fn try_recompose_effective_theme() -> Result<lunaris_theme::LunarisTheme, String> {
    let appearance = crate::config::appearance::current_appearance();

    // 1. Pick the bundled base from the user's `[theme].active`.
    //    Bundled ids match a TOML directly; non-bundled ids fall
    //    back to dark as the structural base and rely on the
    //    user-installed-theme overlay (step 2) to recolour.
    let active_id: String = appearance
        .as_ref()
        .and_then(|a| a.theme.active.as_deref().or(a.theme.mode.as_deref()))
        .unwrap_or("dark")
        .to_string();
    let bundled = match active_id.as_str() {
        "light" => LIGHT_TOML,
        _ => DARK_TOML,
    };

    // 2. User-installed-theme overlay if `theme.active` names a
    //    non-bundled id. Mirrors `desktop-shell::ThemeLoader::load`
    //    semantics so compositor + shell agree on which file is
    //    the active theme. (Codex post-Sprint review HIGH-2 fix.)
    let user_theme = if active_id != "dark" && active_id != "light" {
        let user_path = lunaris_theme::LunarisTheme::user_theme_path(&active_id);
        match std::fs::read_to_string(&user_path) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!(
                    "theme: active=`{active_id}` user-installed file missing at \
                     {} ({e}); falling back to bundled dark",
                    user_path.display()
                );
                None
            }
        }
    } else {
        None
    };

    // 3. Read user's `~/.config/lunaris/theme.toml` overlay if
    //    present.
    let custom_path = lunaris_theme::LunarisTheme::user_customization_path();
    let customization = std::fs::read_to_string(&custom_path).ok();

    // 4. `LunarisTheme::resolve()` merges bundled + user_theme +
    //    customization. Parse failure propagates as Err — callers
    //    decide how to handle it (startup falls back to bundled,
    //    runtime reload keeps the last-good theme).
    let mut composed = lunaris_theme::LunarisTheme::resolve(
        bundled,
        user_theme.as_deref(),
        customization.as_deref(),
    )
    .map_err(|err| format!("customization parse error: {err}"))?;

    // 4. Apply appearance.toml preferences (accent override,
    //    radius_intensity, accessibility).
    if let Some(overrides) = appearance {
        crate::config::appearance::apply_to_theme(&mut composed, &overrides);
    }

    tracing::info!(
        "theme: composed (active={active_id} variant={:?}) \
         radius.chip={} radius.button={} radius.card={} \
         radius.intensity={} effective_card={} \
         active_hint={} font_sans={:?}",
        composed.meta.variant,
        composed.radius.chip,
        composed.radius.button,
        composed.radius.card,
        composed.radius.intensity,
        composed.effective_card(),
        composed.wm.active_hint,
        composed.typography.font_sans,
    );

    // Suppress unused-warning for default_light_theme during dev;
    // it's a public-API hook for future code paths.
    let _ = default_light_theme;

    Ok(composed)
}

/// Start a file watcher for live theme updates. Watches **only**
/// `~/.config/lunaris/theme.toml` — the user-customisation overlay.
///
/// `appearance.toml` is intentionally NOT watched here: it has its
/// own watcher in `crate::config::appearance::watch()` that loads
/// the file from disk into the cached `current_appearance()` and
/// then recomposes. If this watcher also fired on appearance-file
/// changes, it could run *before* `set_appearance` had updated
/// the cache, recompose against stale appearance, and paint a
/// one-frame flicker of the previous theme on every save. (Codex
/// review HIGH-1.)
///
/// Parse-error handling: a transient bad save (mid-typing, atomic
/// rename caught between writes) returns `Err` from
/// `try_recompose_effective_theme`. The handler keeps the last-
/// good global theme and skips render scheduling, so a malformed
/// file does NOT briefly blank the desktop's customisation. The
/// next successful save fires the watcher again. (Codex review
/// HIGH-2.)
///
/// The **initial** composition is done by the caller before
/// `State::new` (see `lib.rs`) so frame 1 already has the correct
/// theme; this function only registers the runtime-change pipeline.
pub fn watch_theme(handle: LoopHandle<'_, State>) {
    let (lt_ping_tx, lt_ping_rx) = calloop::ping::make_ping().unwrap();
    if let Err(e) = handle.insert_source(lt_ping_rx, move |_, _, state| {
        let lt = match try_recompose_effective_theme() {
            Ok(t) => t,
            Err(err) => {
                tracing::warn!(
                    "theme reload: parse failed, keeping last-good: {err}"
                );
                return;
            }
        };

        set_lunaris_theme(lt.clone());
        state.common.lunaris_theme = lt.clone();
        {
            let mut shell = state.common.shell.write();
            shell.lunaris_theme = lt;
        }
        // Feature 4-C: window-header renderer pulls
        // `lunaris_theme()` directly but caches the rasterised
        // pixmap; bump generation so every window re-rasterises.
        crate::backend::render::window_header::bump_theme_generation();

        // Schedule a render on every output — without this the new
        // theme state sits in memory but no frame actually paints
        // it, so window corner-radii / button radii / accent
        // colours stayed at their pre-change values until some
        // unrelated event happened to dirty an output. The
        // outputs collection is read-locked (cloned) before we
        // dispatch so we don't hold the shell lock across the
        // backend calls.
        let outputs: Vec<_> = state.common.shell.read().outputs().cloned().collect();
        for output in outputs {
            state.backend.schedule_render(&output);
        }
    }) {
        tracing::error!("failed to insert lunaris theme ping source: {e}");
    }
    // Watch ONLY theme.toml (user-customisation). appearance.toml
    // has its own watcher path in `crate::config::appearance::watch()`
    // — see HIGH-1 docstring above for the race that motivated
    // splitting these.
    let theme_path = lunaris_theme::LunarisTheme::user_customization_path();
    let lt_watcher = lunaris_theme::ThemeWatcher::start_at(
        vec![theme_path],
        move || {
            lt_ping_tx.ping();
        },
    );
    match lt_watcher {
        Ok(w) => std::mem::forget(w),
        Err(e) => tracing::warn!("failed to start lunaris theme watcher: {e}"),
    }
}

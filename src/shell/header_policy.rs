// SPDX-License-Identifier: GPL-3.0-only

//! When a window gets the compositor-drawn header, and what desktop-shell is
//! told about the headers it draws itself (stacks).
//!
//! Fork-only: upstream draws its headers with Iced and has none of this. Until
//! 8 October it sat in the middle of upstream's `shell/mod.rs`, where every
//! merge had to step around it. Moved here unchanged.

use super::*;

/// Global mirror of `Shell::tiled_headers_enabled` so the render
/// decision can be made from anywhere without threading a `&Shell`
/// through the call chain. Same pattern as
/// `window_header::theme_generation()`. Updated only via
/// `set_tiled_headers_global` from the canonical
/// `Shell::set_tiled_headers_enabled` method.
pub(super) static TILED_HEADERS_ENABLED: AtomicBool = AtomicBool::new(false);

/// Read the global tiled-headers toggle. Called from
/// `should_render_window_header` and `CosmicWindowInternal::has_ssd`.
pub fn tiled_headers_enabled_global() -> bool {
    TILED_HEADERS_ENABLED.load(Ordering::Relaxed)
}

/// Set the global mirror. Use `Shell::set_tiled_headers_enabled`
/// from outside; this is the low-level setter that ALSO needs to
/// fire from there so render decisions and instance state stay
/// aligned.
pub(crate) fn set_tiled_headers_global(v: bool) {
    TILED_HEADERS_ENABLED.store(v, Ordering::Relaxed);
}

/// General eligibility check — is this window "dressable" at all?
/// Used by both the compositor-rendered header path (Feature 4-C,
/// single non-stacked windows) and the shell-rendered stack header
/// path. Two callers share this so policy stays in one place.
///
/// Order of checks is **load-bearing** (see Tiled-Headers Toggle
/// edge-case analysis):
///   1. Stacks always render — toggle never applies, tab-bar is
///      functional UI.
///   2. Fullscreen never renders — covers entire output, no chrome.
///   3. Tiled + toggle-off never renders — user opted out for
///      single tiled windows. Floating windows in the same
///      workspace are unaffected because `is_tiled()` is per
///      window, not per workspace.
///   4. Otherwise: client-side-decoration check (Wayland) or
///      X11 heuristic.
pub fn should_render_window_header(mapped: &CosmicMapped) -> bool {
    if mapped.is_stack() {
        return true;
    }
    let surface = mapped.active_window();
    let is_fullscreen = surface.is_fullscreen(false);
    // Workspace-level placement flag, NOT the xdg-toplevel tiled state.
    // The latter is set on floating windows too when
    // `clip_floating_windows` is enabled (rectangular clipping intent),
    // so reading it here would suppress floating SSD windows' headers
    // whenever the user toggles `tiled_headers` off.
    let is_tiled = mapped.is_in_tiling_layer();
    let tiled_enabled = tiled_headers_enabled_global();

    let csd_branch_eligible = match surface.0.underlying_surface() {
        smithay::desktop::WindowSurface::Wayland(_) => !surface.is_decorated(false),
        smithay::desktop::WindowSurface::X11(x11) => x11_should_render_header(x11),
    };

    header_eligibility_pure(HeaderEligibilityInputs {
        is_stack: false, // already short-circuited
        is_fullscreen,
        is_tiled,
        tiled_headers_enabled: tiled_enabled,
        csd_branch_eligible,
    })
}

/// Inputs describing one window's state for header-eligibility.
/// Carved out so the policy itself is a pure function with a
/// testable truth table — no need to mock CosmicMapped/CosmicSurface.
#[derive(Debug, Clone, Copy)]
pub struct HeaderEligibilityInputs {
    /// `true` for `CosmicMapped::is_stack()`. When set, the policy
    /// always returns `true` regardless of the other inputs (stacks
    /// own their own tab-bar UI which is functional, not decoration).
    pub is_stack: bool,
    /// `true` if the window is currently fullscreen — never
    /// renders a header (chrome would obscure content the user is
    /// fullscreen-viewing).
    pub is_fullscreen: bool,
    /// `true` if the window is currently placed in the workspace's
    /// tiling layer. False for floating. **Not** the xdg-toplevel
    /// tiled state — that flag is also set on floating windows when
    /// `clip_floating_windows` is enabled (clipping intent), so
    /// reading it here would suppress floating SSD windows' headers
    /// when the user toggles `tiled_headers` off. Use
    /// `CosmicMapped::is_in_tiling_layer()` to populate this.
    pub is_tiled: bool,
    /// Global toggle: `true` means tiled windows DO render a header,
    /// `false` means they don't. Floating windows ignore this.
    pub tiled_headers_enabled: bool,
    /// Result of the per-protocol CSD check: Wayland xdg-decoration
    /// ServerSide / unset → true; X11 motif-hints + heuristic → true
    /// when window is undecorated by the client and eligible for
    /// Arlen chrome.
    pub csd_branch_eligible: bool,
}

/// Pure render-eligibility policy. **Order is load-bearing**:
///   1. Stacks → always true.
///   2. Fullscreen → always false.
///   3. Tiled + toggle off → false (per-window, NOT per-workspace).
///   4. Otherwise: client-side-decoration branch.
pub fn header_eligibility_pure(i: HeaderEligibilityInputs) -> bool {
    if i.is_stack {
        return true;
    }
    if i.is_fullscreen {
        return false;
    }
    if i.is_tiled && !i.tiled_headers_enabled {
        return false;
    }
    i.csd_branch_eligible
}

/// Should this window's header events (`window_header_show` /
/// `window_header_update` / `window_header_hide`) be sent to the
/// desktop-shell process for Svelte rendering? Since Feature 4-C
/// moves single-window headers into the compositor, the shell only
/// needs events for **stacks** — its WindowHeader.svelte is still
/// the thing drawing the tab strip integrated with the window
/// controls (Feature 3). Non-stacked single-window headers are
/// now fully handled by `CosmicWindow::header_render_element`
/// during the GL render pass and the shell never sees them.
pub fn should_emit_shell_header_events(mapped: &CosmicMapped) -> bool {
    mapped.is_stack() && should_render_window_header(mapped)
}

/// Hybrid X11 header-eligibility heuristic. See blueprint Limitation 1
/// analysis — ranks a window against four rules, all must pass:
///
/// 1. Not override-redirect, not popup, not a transient of another
///    window. These are menus, tooltips, DnD feedback, modal dialogs
///    — they get their own handling upstream.
/// 2. `_NET_WM_WINDOW_TYPE` is either `Normal` or unset. Excludes
///    Dialog, Utility, Toolbar, Menu, Dock, Dnd, Splash etc.
/// 3. Content geometry is at least 200×100 logical px. Rules out
///    tiny popups that slipped through the earlier filters.
/// 4. Smithay's `X11Surface::is_decorated()` reports `false` —
///    i.e., the app has NOT set Motif hints to `MWM_DECOR_NONE`.
///    Unset Motif hints default to "please decorate" (the Motif
///    spec default of `MWM_DECOR_ALL`), which is exactly the case
///    where a Arlen header is useful for legacy X11 apps that
///    have no native chrome. Apps that set `MWM_DECOR_NONE`
///    (modern GTK3+, Qt5 X11, Steam) draw their own title bar and
///    MUST NOT get a second one from us.
///
/// The semantics of `is_decorated()` on X11 in the vendored
/// Smithay (see `/src/xwayland/xwm/surface.rs:545`) match Wayland's
/// — returns `true` for self-decorated clients — so the call above
/// can be uniform across both branches at the higher level.
pub(super) fn x11_should_render_header(x11: &smithay::xwayland::X11Surface) -> bool {
    use smithay::xwayland::xwm::WmWindowType;

    if x11.is_override_redirect() {
        tracing::debug!(
            "X11-DEBUG window_id={} skipped: override_redirect",
            x11.window_id()
        );
        return false;
    }
    if x11.is_modal() {
        tracing::debug!("X11-DEBUG window_id={} skipped: popup", x11.window_id());
        return false;
    }
    if x11.is_transient_for().is_some() {
        tracing::debug!(
            "X11-DEBUG window_id={} skipped: transient_for present",
            x11.window_id()
        );
        return false;
    }

    match x11.window_type() {
        None | Some(WmWindowType::Normal) => {}
        Some(other) => {
            tracing::debug!(
                "X11-DEBUG window_id={} skipped: window_type={:?}",
                x11.window_id(),
                other
            );
            return false;
        }
    }

    let geo = x11.geometry();
    if geo.size.w < 200 || geo.size.h < 100 {
        tracing::debug!(
            "X11-DEBUG window_id={} skipped: size {}x{} below 200x100 gate",
            x11.window_id(),
            geo.size.w,
            geo.size.h
        );
        return false;
    }

    if x11.is_decorated() {
        tracing::debug!(
            "X11-DEBUG window_id={} skipped: is_decorated()=true (app set MWM_DECOR_NONE)",
            x11.window_id()
        );
        return false;
    }

    tracing::debug!(
        "X11-DEBUG window_id={} eligible: size={}x{} type=Normal no_motif_none",
        x11.window_id(),
        geo.size.w,
        geo.size.h
    );
    true
}

/// Derive the `window_header_show/update` payload for a window.
/// Returns `None` if the window isn't eligible (see
/// `should_render_window_header`) OR geometry can't be resolved yet
/// (e.g. called before the workspace has placed the window).
///
/// Coordinates are GLOBAL: `output.geometry().loc + workspace_local_loc`.
/// Width is taken from the window's geometry; height is always
/// `SSD_HEIGHT` (36px, matches the compositor's reserved space).
pub struct WindowHeaderPayload {
    pub surface_id: u32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub title: String,
    pub activated: bool,
    /// Non-zero if this header belongs to a CosmicStack. The shell
    /// uses it to correlate with the `tab_added/tab_activated/...`
    /// stream and renders tabs inside the header. See Feature 3
    /// (integrated stack header) for the design.
    pub stack_id: u32,
}

/// Per-frame cache entry for the window-header diff loop. Same data
/// as `WindowHeaderPayload` but without the surface_id (it's the
/// HashMap key) and deriving `Clone` so we can snapshot the last
/// sent values.
#[derive(Clone, Debug)]
pub(crate) struct CachedHeaderPayload {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub title: String,
    pub activated: bool,
    pub stack_id: u32,
}

impl From<&WindowHeaderPayload> for CachedHeaderPayload {
    fn from(p: &WindowHeaderPayload) -> Self {
        Self {
            x: p.x,
            y: p.y,
            width: p.width,
            height: p.height,
            title: p.title.clone(),
            activated: p.activated,
            stack_id: p.stack_id,
        }
    }
}

/// Top bit used as a namespace marker for X11-originated surface ids
/// in the `arlen-shell-overlay::window_header_*` protocol. Wayland
/// protocol ids (client-allocated, start at 2 and increment) never
/// have this bit set in practice; X11 XIDs are 29-bit on all real X
/// servers so masking with `0x7FFF_FFFF` doesn't lose information
/// and shifting `0x8000_0000` in gives us a clean split.
///
/// The shell treats this as an opaque `u32` — it just echoes it back
/// on `window_header_action`. Only the compositor-side resolver
/// (`window_header_action` handler) needs to interpret the top bit to
/// look the window up in the right table.
pub const HEADER_ID_X11_NAMESPACE: u32 = 0x8000_0000;

/// Derive the protocol surface-id for a mapped window's header.
/// Stable across frames for the same window — both the refresh diff
/// and the action resolver rely on this invariant.
pub fn window_header_surface_id(mapped: &CosmicMapped) -> Option<u32> {
    use smithay::reexports::wayland_server::Resource;

    let surface = mapped.active_window();
    match surface.0.underlying_surface() {
        smithay::desktop::WindowSurface::Wayland(_) => {
            // Wayland: use the wl_surface protocol id with the top
            // bit forced clear. In practice client-allocated ids
            // never have it set; the mask is a belt-and-braces.
            let wl = surface.wl_surface()?;
            Some(wl.id().protocol_id() & !HEADER_ID_X11_NAMESPACE)
        }
        smithay::desktop::WindowSurface::X11(x11) => {
            // X11: XID (< 2^29 in practice) with the top bit set to
            // declare the namespace. Keeps the id stable for the
            // window's full lifetime and lets the action resolver
            // pick the right surface family on lookup.
            Some(x11.window_id() | HEADER_ID_X11_NAMESPACE)
        }
    }
}

/// Payload for a window whose position is currently driven by an
/// interactive `MoveGrab` rather than a workspace layout. The
/// per-frame refresh calls this for every active seat's move-grab
/// state so the shell gets live header updates AND the window
/// stays registered in `current` (which prevents the stale check
/// from emitting `window_header_hide` mid-drag and making the
/// header visually disappear — the exact symptom we hit).
///
/// Coordinates come directly from the grab state: `location +
/// window_offset` is already the global-logical origin of the
/// window as rendered (see `MoveGrabState::render`). No workspace
/// lookup, no `space_for()` call — during a drag the window is
/// not in any workspace.
pub fn dragged_window_header_payload(
    grab_state: &crate::shell::grabs::MoveGrabState,
) -> Option<WindowHeaderPayload> {
    let mapped = &grab_state.window;
    if !should_render_window_header(mapped) {
        return None;
    }
    let surface_id = window_header_surface_id(mapped)?;

    let origin = grab_state.location.to_i32_round() + grab_state.window_offset;
    let size = mapped.geometry().size;

    let surface = mapped.active_window();
    let stack_id = mapped
        .stack_ref()
        .map(|stack| stack.stack_id())
        .unwrap_or(0);

    Some(WindowHeaderPayload {
        surface_id,
        x: origin.x,
        y: origin.y,
        width: size.w,
        height: element::window::SSD_HEIGHT,
        title: surface.title(),
        activated: surface.is_activated(false),
        stack_id,
    })
}

pub fn window_header_payload(shell: &Shell, mapped: &CosmicMapped) -> Option<WindowHeaderPayload> {
    if !should_render_window_header(mapped) {
        return None;
    }
    let surface = mapped.active_window();
    let surface_id = window_header_surface_id(mapped)?;

    let workspace = shell.space_for(mapped)?;
    let local_rect = workspace.element_geometry(mapped)?;
    let output_geo = workspace.output().geometry();

    // STACK-DEBUG: if this is a stack, tack its stack_id onto the
    // payload so the shell can render tabs inside the header. 0 for
    // non-stacks.
    let stack_id = mapped
        .stack_ref()
        .map(|stack| stack.stack_id())
        .unwrap_or(0);

    Some(WindowHeaderPayload {
        surface_id,
        x: output_geo.loc.x + local_rect.loc.x,
        // Header sits directly above the window content. The
        // compositor already reserves SSD_HEIGHT pixels via
        // has_ssd → window.rs adds SSD_HEIGHT to total height.
        // The header lives in that reserved top strip.
        y: output_geo.loc.y + local_rect.loc.y,
        width: local_rect.size.w,
        height: element::window::SSD_HEIGHT,
        title: surface.title(),
        activated: surface.is_activated(false),
        stack_id,
    })
}

impl Common {
    /// Per-frame sync for Arlen-rendered window headers.
    ///
    /// Covers three kinds of changes in one pass:
    /// - geometry (position / width) — from set_geometry calls
    /// - title text — from client commits
    /// - activated flag — from set_focus
    ///
    /// Implementation keeps a cache of `last_sent_payload` per
    /// `surface_id`. On each frame we compute the current payload
    /// for every SSD window and diff; only actual changes emit
    /// `window_header_update`. Windows that disappeared between
    /// frames (e.g. closed without the destroyed-handler firing
    /// yet — rare edge case) get a `window_header_hide` and their
    /// cache entry removed.
    ///
    /// One-liner cost per frame is ~one allocation per mapped
    /// window to read geometry + title; cheap relative to the
    /// render pipeline itself.
    /// Synchronously emit a `window_header_update` for exactly one
    /// window, bypassing the full per-frame diff scan. Used by
    /// `MoveGrab::motion` / `ResizeGrab::motion` (Feature 4:
    /// latency-sync) so drag updates cost O(1) instead of O(n) per
    /// pointer-motion event.
    ///
    /// The cache is still checked to avoid emitting redundant
    /// events when sub-pixel motion rounds to the same position.
    /// Silently no-ops if `mapped` is not eligible for a header.
    pub(crate) fn refresh_window_header_for(&mut self, mapped: &crate::shell::CosmicMapped) {
        let shell = self.shell.read();

        // Prefer a drag-state payload if this window is currently
        // being interactively moved: `window_header_payload`
        // resolves the window's position via the workspace, which
        // is `None` during a MoveGrab (the window is not in any
        // workspace). Without this, per-motion updates emit
        // nothing and the shell header freezes at its pre-drag
        // position.
        let mut payload: Option<crate::shell::WindowHeaderPayload> = None;
        for seat in shell.seats.iter() {
            let Some(grab_state_slot) = seat
                .user_data()
                .get::<crate::shell::grabs::SeatMoveGrabState>()
            else {
                continue;
            };
            let guard = grab_state_slot.lock().unwrap();
            if let Some(grab_state) = guard.as_ref()
                && &grab_state.window == mapped
            {
                payload = crate::shell::dragged_window_header_payload(grab_state);
                break;
            }
        }
        if payload.is_none() {
            payload = crate::shell::window_header_payload(&shell, mapped);
        }
        drop(shell);
        let Some(payload) = payload else {
            return;
        };
        let cache = &mut self.window_header_cache;
        let changed = match cache.get(&payload.surface_id) {
            None => true,
            Some(prev) => {
                prev.x != payload.x
                    || prev.y != payload.y
                    || prev.width != payload.width
                    || prev.height != payload.height
                    || prev.title != payload.title
                    || prev.activated != payload.activated
                    || prev.stack_id != payload.stack_id
            }
        };
        if !changed {
            return;
        }
        if cache.contains_key(&payload.surface_id) {
            self.shell_overlay_state.send_window_header_update(
                payload.surface_id,
                payload.x,
                payload.y,
                payload.width,
                payload.height,
                payload.title.clone(),
                payload.activated,
                payload.stack_id,
            );
        } else {
            self.shell_overlay_state.send_window_header_show(
                payload.surface_id,
                payload.x,
                payload.y,
                payload.width,
                payload.height,
                payload.title.clone(),
                payload.activated,
                true,
                true,
                payload.stack_id,
            );
        }
        cache.insert(
            payload.surface_id,
            crate::shell::CachedHeaderPayload::from(&payload),
        );
    }

    pub(crate) fn refresh_window_headers(&mut self) {
        use crate::shell::{should_emit_shell_header_events, window_header_payload};

        let shell = self.shell.read();
        // Build current snapshot for all eligible windows. Feature
        // 4-C switched non-stacked single-window headers over to
        // compositor rendering (`CosmicWindow::header_render_element`),
        // so we only emit shell-bound events for STACKS now; the
        // tab strip still needs the shell to paint tabs + buttons.
        let mut current: std::collections::HashMap<u32, crate::shell::WindowHeaderPayload> =
            std::collections::HashMap::new();
        for space in shell.workspaces.spaces() {
            for mapped in space.mapped() {
                if !should_emit_shell_header_events(mapped) {
                    continue;
                }
                if let Some(payload) = window_header_payload(&shell, mapped) {
                    current.insert(payload.surface_id, payload);
                }
            }
        }
        // Also scan sticky layers across all sets so sticky windows
        // get headers too.
        for set in shell.workspaces.sets.values() {
            for mapped in set.sticky_layer.mapped() {
                if !should_emit_shell_header_events(mapped) {
                    continue;
                }
                if let Some(payload) = window_header_payload(&shell, mapped) {
                    current.insert(payload.surface_id, payload);
                }
            }
        }
        // Feature 4 fix: windows inside an interactive MoveGrab are
        // NOT in any workspace for the grab's lifetime — their
        // rendering is driven from `MoveGrabState` on the seat's
        // user-data. If we skipped them here the stale-check below
        // would emit `window_header_hide` every frame during a
        // drag and the Arlen header would visually disappear
        // until release. Poll each seat's grab state and overlay
        // a live payload so (a) the dragged window stays in
        // `current` (no hide), and (b) its position is updated
        // once per frame from the same grab state the compositor
        // is rendering from.
        // Only surface drag-state headers for STACKS — Feature 4-C
        // renders non-stacked single-window headers compositor-
        // side, so the shell doesn't need updates for those.
        for seat in shell.seats.iter() {
            let Some(grab_state_slot) = seat
                .user_data()
                .get::<crate::shell::grabs::SeatMoveGrabState>()
            else {
                continue;
            };
            let guard = grab_state_slot.lock().unwrap();
            if let Some(grab_state) = guard.as_ref()
                && should_emit_shell_header_events(&grab_state.window)
                && let Some(payload) = crate::shell::dragged_window_header_payload(grab_state)
            {
                current.insert(payload.surface_id, payload);
            }
        }
        drop(shell);

        // Diff against last-sent cache.
        let cache = &mut self.window_header_cache;
        // Emit updates for changed entries.
        for (id, payload) in &current {
            let changed = match cache.get(id) {
                None => true,
                Some(prev) => {
                    prev.x != payload.x
                        || prev.y != payload.y
                        || prev.width != payload.width
                        || prev.height != payload.height
                        || prev.title != payload.title
                        || prev.activated != payload.activated
                        || prev.stack_id != payload.stack_id
                }
            };
            if changed {
                // new entries get `show` (first time), existing get
                // `update`. No-op if the shell's store already has it
                // from the map-time show call.
                if cache.contains_key(id) {
                    self.shell_overlay_state.send_window_header_update(
                        payload.surface_id,
                        payload.x,
                        payload.y,
                        payload.width,
                        payload.height,
                        payload.title.clone(),
                        payload.activated,
                        payload.stack_id,
                    );
                } else {
                    self.shell_overlay_state.send_window_header_show(
                        payload.surface_id,
                        payload.x,
                        payload.y,
                        payload.width,
                        payload.height,
                        payload.title.clone(),
                        payload.activated,
                        true,
                        true,
                        payload.stack_id,
                    );
                }
                // Cache the clone-able fields. The cache struct is a
                // stripped-down copy of WindowHeaderPayload that
                // implements Clone — see Common::window_header_cache.
                cache.insert(*id, CachedHeaderPayload::from(payload));
            }
        }
        // Hide stale entries (window gone between frames).
        let stale: Vec<u32> = cache
            .keys()
            .filter(|id| !current.contains_key(id))
            .copied()
            .collect();
        for id in stale {
            self.shell_overlay_state.send_window_header_hide(id);
            // Feature 4 (attach): notify any bound shell
            // attachments that their target window is gone. The
            // attachments themselves are still alive until the
            // shell destroys them; they just stop tracking a
            // window.
            self.window_attach_state.unbind_window(id);
            cache.remove(&id);
        }
    }

    /// Check all surfaces with active titlebar bindings and send
    /// `mode_changed` events if the window mode has changed.
    pub fn refresh_titlebar_modes(&mut self) {
        use crate::wayland::handlers::titlebar::state_to_json;
        use smithay::reexports::wayland_server::Resource;

        // Collect surface IDs with active titlebar bindings.
        let surface_ids: Vec<u64> = self.titlebar_manager_state.active_surface_ids().collect();

        if surface_ids.is_empty() {
            return;
        }

        let shell = self.shell.read();

        for sid in surface_ids {
            // Determine current window state by searching workspaces.
            let mut is_fullscreen = false;
            let mut is_tiled = false;
            let mut found = false;

            'outer: for workspace in shell.workspaces.spaces() {
                // Check fullscreen surfaces.
                for fs in &workspace.fullscreen_surfaces {
                    if let Some(wl) = fs.surface.wl_surface()
                        && wl.id().protocol_id() as u64 == sid
                    {
                        is_fullscreen = true;
                        found = true;
                        break 'outer;
                    }
                }
                // Check tiling layer.
                for (mapped, _) in workspace.tiling_layer.mapped() {
                    for (surface, _) in mapped.windows() {
                        if let Some(wl) = surface.wl_surface()
                            && wl.id().protocol_id() as u64 == sid
                        {
                            is_tiled = true;
                            found = true;
                            break 'outer;
                        }
                    }
                }
                // Check floating layer.
                for mapped in workspace.floating_layer.mapped() {
                    for (surface, _) in mapped.windows() {
                        if let Some(wl) = surface.wl_surface()
                            && wl.id().protocol_id() as u64 == sid
                        {
                            // Floating: is_tiled = false, is_fullscreen = false.
                            found = true;
                            break 'outer;
                        }
                    }
                }
            }

            if !found {
                continue;
            }

            let mode = if is_fullscreen {
                crate::wayland::protocols::titlebar::TitlebarMode::Fullscreen
            } else if is_tiled {
                crate::wayland::protocols::titlebar::TitlebarMode::Tiled
            } else {
                crate::wayland::protocols::titlebar::TitlebarMode::Floating
            };

            if self.titlebar_manager_state.send_mode_changed(sid, mode) {
                // Mode changed; also notify the shell.
                if let Some(tb) = self.titlebar_manager_state.get(sid) {
                    let json = state_to_json(tb);
                    self.shell_overlay_state
                        .send_window_header_content(sid as u32, json);
                }
            }
        }
    }

    /// Check the fullscreen reveal hide timer even when no pointer events
    /// arrive. Without this, a stationary pointer would leave the state
    /// machine stuck in `HidePending` forever.
    pub fn tick_fullscreen_reveal_timer(&mut self) {
        use crate::shell::fullscreen_reveal::{RevealAction, RevealPhase};

        if self.fullscreen_reveal.phase != RevealPhase::HidePending {
            return;
        }

        // Tick with the same pointer_y that caused the leave (outside
        // titlebar). The exact value does not matter as long as it is
        // above the titlebar threshold (36px).
        let prev_sid = self.fullscreen_reveal.surface_id;
        let action = self.fullscreen_reveal.update(100.0, true, prev_sid);
        if action == RevealAction::Hide {
            let hide_sid = if prev_sid != 0 { prev_sid } else { 0 };
            self.shell_overlay_state
                .send_fullscreen_titlebar_hide(hide_sid);
        }
    }
}

impl Shell {
    /// Set the tiled-headers enabled state on the shell AND on the
    /// global atomic mirror so render-decision callsites (which
    /// don't have access to a `Shell` borrow) can read it. When
    /// the value actually changes, every tiled mapped window gets
    /// a `configure()` so its `has_ssd` answer flips and the
    /// xdg-toplevel reports the new size to the client. Stack
    /// children are skipped — stacks always have their tab-bar.
    ///
    /// `bump_theme_generation()` is also called so the
    /// per-window header pixmap cache (Feature 4-C) invalidates.
    /// On the next frame `should_render_window_header` returns
    /// the new decision and the window-header element either
    /// appears or disappears atomically with the geometry update.
    pub fn set_tiled_headers_enabled(&mut self, enabled: bool) {
        if self.tiled_headers_enabled == enabled {
            return;
        }
        let old = self.tiled_headers_enabled;
        self.tiled_headers_enabled = enabled;
        crate::shell::set_tiled_headers_global(enabled);
        tracing::info!("TILE-DEBUG tiled_headers_enabled: {} -> {}", old, enabled);

        // Find every tiled, non-stack mapped window. We collect
        // first to release the iterator borrow on `self` before
        // mutating per-window. Stacks are excluded because they
        // always render their tab-bar header — toggle is a no-op
        // for them.
        // Reconfigure windows actually placed in a tiling layer —
        // floating windows are unaffected by the toggle (their header
        // is gated by CSD only), so reconfiguring them would just
        // cause an unnecessary buffer churn.
        let to_reconfigure: Vec<CosmicMapped> = self
            .mapped()
            .filter(|m| !m.is_stack())
            .filter(|m| m.is_in_tiling_layer())
            .cloned()
            .collect();
        let n = to_reconfigure.len();
        for mapped in &to_reconfigure {
            // `configure()` re-emits xdg_toplevel.configure with
            // current geometry. The client recomputes its content
            // size (now with or without the 36-px SSD strip) and
            // commits a new buffer. The compositor render path
            // re-evaluates `has_ssd` on the next frame.
            mapped.configure();
        }
        tracing::info!(
            "TILE-DEBUG reconfigure storm: {} tiled windows asked to refresh size",
            n
        );

        // Invalidate per-window header pixmap caches so the cached
        // pre-toggle pixmap doesn't paint over the new state.
        crate::backend::render::window_header::bump_theme_generation();
    }
}

#[cfg(test)]
mod tiled_headers_policy_tests {
    use super::{
        HeaderEligibilityInputs, TILED_HEADERS_ENABLED, header_eligibility_pure,
        set_tiled_headers_global, tiled_headers_enabled_global,
    };
    use std::sync::atomic::Ordering;

    fn inputs(
        is_stack: bool,
        is_fullscreen: bool,
        is_tiled: bool,
        tiled_headers_enabled: bool,
        csd: bool,
    ) -> HeaderEligibilityInputs {
        HeaderEligibilityInputs {
            is_stack,
            is_fullscreen,
            is_tiled,
            tiled_headers_enabled,
            csd_branch_eligible: csd,
        }
    }

    #[test]
    fn stack_always_renders_regardless_of_other_inputs() {
        // Stack short-circuits everything — fullscreen, tiled+off,
        // CSD-only client. Tab-bar is functional UI, must always be
        // visible while the stack exists.
        for fs in [true, false] {
            for tiled in [true, false] {
                for toggle in [true, false] {
                    for csd in [true, false] {
                        let i = inputs(true, fs, tiled, toggle, csd);
                        assert!(header_eligibility_pure(i), "stack should render: {:?}", i);
                    }
                }
            }
        }
    }

    #[test]
    fn fullscreen_never_renders() {
        // Outside the stack short-circuit, fullscreen always wins.
        for tiled in [true, false] {
            for toggle in [true, false] {
                for csd in [true, false] {
                    let i = inputs(false, true, tiled, toggle, csd);
                    assert!(!header_eligibility_pure(i), "fullscreen: {:?}", i);
                }
            }
        }
    }

    #[test]
    fn tiled_with_toggle_off_does_not_render() {
        // Default scenario the toggle was added for: tiled SSD
        // window (Kitty in tiling mode) gets no header.
        let i = inputs(false, false, true, false, true);
        assert!(!header_eligibility_pure(i));
    }

    #[test]
    fn tiled_with_toggle_on_renders_when_csd_branch_says_yes() {
        let i = inputs(false, false, true, true, true);
        assert!(header_eligibility_pure(i));
    }

    #[test]
    fn floating_renders_regardless_of_toggle() {
        // Floating windows IGNORE the toggle — they always show
        // their decoration if the CSD branch says so. This is
        // critical for "floating window in tiling workspace via
        // window-rule" scenarios.
        for toggle in [true, false] {
            let i = inputs(false, false, false, toggle, true);
            assert!(
                header_eligibility_pure(i),
                "floating ignores toggle: toggle={toggle}"
            );
        }
    }

    #[test]
    fn floating_with_csd_client_never_renders() {
        // Client-side-decorated app (Firefox, GTK4) — Arlen
        // never paints chrome regardless of toggle.
        for toggle in [true, false] {
            let i = inputs(false, false, false, toggle, false);
            assert!(
                !header_eligibility_pure(i),
                "csd app never gets header: toggle={toggle}"
            );
        }
    }

    #[test]
    fn tiled_with_csd_client_never_renders_either() {
        // Tiled CSD app: header not from us regardless. Toggle off
        // is the relevant case here — even if toggle were on, no
        // header (because csd_branch_eligible=false).
        let i = inputs(false, false, true, true, false);
        assert!(!header_eligibility_pure(i));
        let i = inputs(false, false, true, false, false);
        assert!(!header_eligibility_pure(i));
    }

    // ── Global atomic mirror ──

    #[test]
    fn global_toggle_defaults_to_false() {
        // Reading the static at any time before the first set
        // should give us false. (This test sets+resets to be
        // robust against test-runner ordering.)
        let saved = TILED_HEADERS_ENABLED.load(Ordering::Relaxed);
        TILED_HEADERS_ENABLED.store(false, Ordering::Relaxed);
        assert!(!tiled_headers_enabled_global());
        // Restore so other tests don't see stale state.
        TILED_HEADERS_ENABLED.store(saved, Ordering::Relaxed);
    }

    #[test]
    fn set_tiled_headers_global_round_trip() {
        let saved = TILED_HEADERS_ENABLED.load(Ordering::Relaxed);
        set_tiled_headers_global(true);
        assert!(tiled_headers_enabled_global());
        set_tiled_headers_global(false);
        assert!(!tiled_headers_enabled_global());
        TILED_HEADERS_ENABLED.store(saved, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod header_id_tests {
    use super::HEADER_ID_X11_NAMESPACE;

    /// Simulate the Wayland encoding: take a protocol id and mask the
    /// top bit off. Real Wayland client-allocated ids never use the
    /// top bit anyway, but the mask is our safety net.
    fn encode_wayland(protocol_id: u32) -> u32 {
        protocol_id & !HEADER_ID_X11_NAMESPACE
    }

    /// Simulate the X11 encoding: take an XID and set the top bit.
    fn encode_x11(xid: u32) -> u32 {
        xid | HEADER_ID_X11_NAMESPACE
    }

    #[test]
    fn wayland_encoding_clears_top_bit() {
        assert_eq!(encode_wayland(0x0000_0002), 0x0000_0002);
        assert_eq!(encode_wayland(0x007F_FFFF), 0x007F_FFFF);
        assert_eq!(encode_wayland(0x0123_4567), 0x0123_4567);
    }

    #[test]
    fn x11_encoding_sets_top_bit() {
        assert_eq!(encode_x11(0x0020_0000), 0x8020_0000);
        assert_eq!(encode_x11(0x0040_0042), 0x8040_0042);
    }

    #[test]
    fn namespace_top_bit_is_reserved_for_x11() {
        // Real protocol ids and real X11 XIDs, encoded, must land in
        // distinct halves. Verify on a handful of realistic values
        // (Wayland starts at 2, X11 XIDs begin around 0x200000).
        let wl_ids: [u32; 4] = [0x02, 0xff, 0x00f0_0001, 0x1234];
        let x11_ids: [u32; 4] = [0x0020_0000, 0x0040_0002, 0x00a0_0010, 0x0f00_0001];
        for id in wl_ids.iter().copied() {
            assert!(encode_wayland(id) & HEADER_ID_X11_NAMESPACE == 0);
        }
        for id in x11_ids.iter().copied() {
            assert!(encode_x11(id) & HEADER_ID_X11_NAMESPACE != 0);
        }
    }

    #[test]
    fn encodings_are_round_trippable() {
        // Given an encoded id, we can tell whether it's Wayland or
        // X11 by inspecting the top bit — that's how the action
        // resolver will dispatch.
        let wl_encoded = encode_wayland(0x1234);
        let x11_encoded = encode_x11(0x0020_1234);
        assert_eq!(wl_encoded & HEADER_ID_X11_NAMESPACE, 0);
        assert_eq!(
            x11_encoded & HEADER_ID_X11_NAMESPACE,
            HEADER_ID_X11_NAMESPACE
        );
        // Recover the X11 XID by masking.
        assert_eq!(x11_encoded & !HEADER_ID_X11_NAMESPACE, 0x0020_1234);
    }
}

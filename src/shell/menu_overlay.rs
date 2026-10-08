// SPDX-License-Identifier: GPL-3.0-only

//! Handing the window context menu to desktop-shell, which draws it.
//!
//! Upstream draws the menu itself with Iced; this fork serialises it over the
//! shell overlay protocol instead. These two helpers lived as functions nested
//! inside upstream's `Shell::menu_request` until 8 October and moved here
//! unchanged, so the merge surface of that method is the call, not the body.

use super::*;

/// Convert items to protocol representation.
///
/// Walks the tree recursively. `Item::Submenu` becomes a real
/// `ContextMenuItem::Submenu` (rendered as a fly-out by the shell).
/// Entries without a `WindowAction` tag fall back to a `Separator`
/// placeholder so indices in `pending_callbacks` stay aligned with
/// the DFS order of the protocol stream.
pub(super) fn items_to_protocol(items: &[Item]) -> Vec<ContextMenuItem> {
    items
        .iter()
        .map(|item| match item {
            Item::Separator => ContextMenuItem::Separator,
            Item::Entry {
                title,
                action: Some(action),
                toggled,
                disabled,
                shortcut,
                ..
            } => ContextMenuItem::Entry {
                action: *action,
                toggled: *toggled,
                disabled: *disabled,
                shortcut: shortcut.clone(),
                // Forward the compositor-side localized title so
                // the shell doesn't have to re-derive it from
                // `action`. Non-negotiable for items where many
                // entries share one `WindowAction` (e.g. the
                // workspace picker inside "Move to Workspace").
                label: Some(title.clone()),
            },
            Item::Submenu {
                title,
                items: children,
            } => ContextMenuItem::Submenu {
                label: title.clone(),
                disabled: false,
                items: items_to_protocol(children),
            },
            Item::Entry { action: None, .. } => {
                tracing::warn!(
                    "menu_request: item without WindowAction in overlay path; \
                     using Separator placeholder to preserve index alignment"
                );
                ContextMenuItem::Separator
            }
        })
        .collect()
}

// `flatten_callbacks` lives in `shell/grabs/menu/mod.rs` so it's
// covered by unit tests that lock the DFS-index invariant against
// the Wayland serializer in `ShellOverlayState::send_context_menu`.

/// Find the desktop-shell's wl_surface by matching the overlay client.
/// Returns the `PointerFocusTarget` and the surface origin in logical
/// coordinates (used as pointer-event offset in the grab).
pub(super) fn find_shell_focus(
    shell: &Shell,
    shell_overlay_state: &ShellOverlayState,
    dh: &DisplayHandle,
) -> Option<(focus::target::PointerFocusTarget, Point<f64, Logical>)> {
    use smithay::reexports::wayland_server::Resource;
    tracing::info!("find_shell_focus: called");
    let Some(instance) = shell_overlay_state.overlay_instance() else {
        tracing::info!("find_shell_focus: no overlay instance, returning None");
        return None;
    };
    let client = instance.client();
    tracing::info!("find_shell_focus: instance client={}", client.is_some());
    let creds = client.and_then(|c| c.get_credentials(dh).ok());
    tracing::info!("find_shell_focus: credentials={creds:?}");
    let Some(overlay_pid) = creds.map(|c| c.pid) else {
        tracing::info!("find_shell_focus: no PID, returning None");
        return None;
    };

    // Desktop-shell is a layer-shell client, not an XDG toplevel.
    // Search the layer map on each output for a surface whose client
    // PID matches the overlay instance PID.
    tracing::info!("find_shell_focus: overlay_pid={overlay_pid}");
    for output in shell.outputs() {
        let layer_map = layer_map_for_output(output);
        let layer_count = layer_map.layers().count();
        tracing::info!(
            "find_shell_focus: output={} layer_count={layer_count}",
            output.name()
        );
        for layer in layer_map.layers() {
            let surface = layer.wl_surface();
            let surface_pid = surface
                .client()
                .and_then(|c| c.get_credentials(dh).ok())
                .map(|creds| creds.pid);
            tracing::info!(
                "find_shell_focus: layer ns={} pid={surface_pid:?}",
                layer.namespace()
            );
            if surface_pid == Some(overlay_pid) {
                tracing::info!("find_shell_focus: MATCH on layer ns={}", layer.namespace());
                let target = focus::target::PointerFocusTarget::WlSurface {
                    surface: surface.clone(),
                    toplevel: None,
                };
                return Some((target, Point::from((0., 0.))));
            }
        }
    }
    tracing::info!("find_shell_focus: NO match found");
    None
}

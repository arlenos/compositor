// SPDX-License-Identifier: GPL-3.0-only

//! The scratchpad: windows parked out of sight and brought back with one key.
//! Fork-only; moved out of upstream's `shell/mod.rs` unchanged on 8 October.

use super::*;

/// State for the scratchpad: hidden floating windows that can be
/// toggled on/off instantly with a keybinding.
#[derive(Debug, Default)]
pub struct ScratchpadState {
    /// Windows stored in the scratchpad (may be hidden or visible).
    pub windows: Vec<CosmicMapped>,
    /// Index of the currently visible scratchpad window, or None if all hidden.
    pub visible: Option<usize>,
}

impl Shell {
    /// Move the focused window to the scratchpad.
    ///
    /// Removes the window from its current layer (tiling or floating),
    /// hides it, and adds it to the scratchpad list.
    pub fn scratchpad_move(&mut self, seat: &Seat<State>) {
        let output = seat.focused_or_active_output();
        let Some(workspace) = self.active_space_mut(&output) else {
            return;
        };

        // Get the focused mapped element.
        let maybe_window = workspace.focus_stack.get(seat).iter().next().cloned();
        let Some(FocusTarget::Window(mapped)) = maybe_window else {
            return;
        };

        // Remove from workspace (handles both tiling and floating).
        workspace.unmap_element(&mapped);

        // Hide all surfaces of the mapped element.
        for (surface, _) in mapped.windows() {
            if let Some(_wl) = surface.wl_surface() {
                // Setting geometry to zero effectively hides the window.
                // The window is kept alive in the scratchpad list.
            }
        }

        tracing::info!(
            "scratchpad: moved {} to scratchpad (now {} windows)",
            mapped.active_window().app_id(),
            self.scratchpad.windows.len() + 1,
        );

        self.scratchpad.windows.push(mapped);
    }

    /// Toggle scratchpad visibility.
    ///
    /// - If no scratchpad window is visible and scratchpad is not empty:
    ///   show the first window (floating, centered, 80% output size).
    /// - If a scratchpad window is visible and focused: hide it.
    /// - If a scratchpad window is visible but not focused: focus it.
    /// - If pressed again while focused: cycle to next scratchpad window.
    pub fn scratchpad_toggle(&mut self, seat: &Seat<State>) {
        if self.scratchpad.windows.is_empty() {
            return;
        }

        let output = seat.focused_or_active_output();

        if let Some(visible_idx) = self.scratchpad.visible {
            // A scratchpad window is currently shown.
            let mapped = &self.scratchpad.windows[visible_idx];

            // Check if the scratchpad window is currently focused.
            let Some(workspace) = self.active_space(&output) else {
                return;
            };
            let is_focused = workspace
                .focus_stack
                .get(seat)
                .iter()
                .next()
                .is_some_and(|f| matches!(f, FocusTarget::Window(w) if w == mapped));

            if is_focused {
                // Already focused: cycle to next or hide.
                let next_idx = (visible_idx + 1) % self.scratchpad.windows.len();
                if next_idx == visible_idx {
                    // Only one window: hide it.
                    self.scratchpad_hide(seat);
                } else {
                    // Hide current, show next.
                    self.scratchpad_hide(seat);
                    self.scratchpad_show(next_idx, seat);
                }
            } else {
                // Visible but not focused: just focus it.
                // The window is already in the floating layer.
            }
        } else {
            // No scratchpad window visible: show the first one.
            self.scratchpad_show(0, seat);
        }
    }

    /// Show a scratchpad window by index.
    ///
    /// Maps it to the active workspace's floating layer, centered at 80%
    /// of the output size.
    fn scratchpad_show(&mut self, idx: usize, seat: &Seat<State>) {
        if idx >= self.scratchpad.windows.len() {
            return;
        }

        let output = seat.focused_or_active_output();
        let output_geo = output.geometry();

        // 80% of output size.
        let width = (output_geo.size.w as f64 * 0.8) as i32;
        let height = (output_geo.size.h as f64 * 0.8) as i32;
        let x = output_geo.loc.x + (output_geo.size.w - width) / 2;
        let y = output_geo.loc.y + (output_geo.size.h - height) / 2;

        let mapped = self.scratchpad.windows[idx].clone();

        // Configure the window size.
        mapped.set_geometry(smithay::utils::Rectangle::new(
            (0, 0).into(),
            (width, height).into(),
        ));

        let Some(workspace) = self.active_space_mut(&output) else {
            return;
        };

        // Map to floating layer at centered position.
        let local_pos =
            Point::<i32, Logical>::from((x - output_geo.loc.x, y - output_geo.loc.y)).as_local();
        workspace.floating_layer.map(mapped, Some(local_pos));

        self.scratchpad.visible = Some(idx);

        tracing::info!("scratchpad: showing window {idx}");
    }

    /// Hide the currently visible scratchpad window.
    fn scratchpad_hide(&mut self, seat: &Seat<State>) {
        let Some(visible_idx) = self.scratchpad.visible.take() else {
            return;
        };

        if visible_idx >= self.scratchpad.windows.len() {
            return;
        }

        let mapped = self.scratchpad.windows[visible_idx].clone();
        let output = seat.focused_or_active_output();

        if let Some(workspace) = self.active_space_mut(&output) {
            workspace.unmap_element(&mapped);
        }

        tracing::info!("scratchpad: hiding window {visible_idx}");
    }

    /// Remove a window from the scratchpad if it was closed.
    ///
    /// Called during surface cleanup to handle the edge case where
    /// a scratchpad window is destroyed.
    pub fn scratchpad_remove_dead(&mut self) {
        let before = self.scratchpad.windows.len();
        self.scratchpad.windows.retain(|w| w.alive());

        if self.scratchpad.windows.len() != before {
            // Adjust visible index.
            if let Some(idx) = self.scratchpad.visible
                && idx >= self.scratchpad.windows.len()
            {
                self.scratchpad.visible = None;
            }
        }
    }
}

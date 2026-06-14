/// Handler for the `arlen-grid-v1` Wayland protocol.
///
/// Receives grid back-pane declarations from the terminal (the grid
/// subsurface, its region, cell metrics, and the DOM holes) and updates the
/// per-surface [`GridState`]. The actual compositing of the grid under the app
/// window is the render half (TM-R1 render), wired separately.
///
/// The state-mutating logic lives in pure helper functions so it can be
/// unit-tested without a live Wayland client, the same shape as
/// `handlers/titlebar.rs`.
///
/// See `docs/architecture/terminal.md` §2.2.

use crate::{
    delegate_grid,
    state::State,
    wayland::protocols::grid::{CellSize, GridHandler, GridManagerState, GridRect, GridState},
};

impl GridHandler for State {
    fn grid_manager_state(&mut self) -> &mut GridManagerState {
        &mut self.common.grid_manager_state
    }

    fn notify_grid_changed(&mut self, _surface_id: u64) {
        // The render half (TM-R1 render) recomputes the grid back-pane
        // compositing from the per-surface GridState on the next frame. The
        // state is already updated by the pure helpers below; nothing else is
        // pushed here today.
    }

    fn notify_grid_removed(&mut self, _surface_id: u64) {
        // The render half drops the back pane once the GridState is gone.
    }
}

delegate_grid!(State);

/// Set the grid region, clamping a negative width or height to zero (a
/// degenerate region paints nothing rather than wrapping into a huge rect).
pub fn set_region(state: &mut GridState, x: i32, y: i32, width: i32, height: i32) {
    state.region = Some(GridRect {
        x,
        y,
        width: width.max(0),
        height: height.max(0),
    });
}

/// Set the cell metrics, clamping to non-negative. A zero dimension disables
/// cell alignment for that axis (see [`cell_aligned_size`]).
pub fn set_cell_size(state: &mut GridState, width: i32, height: i32) {
    state.cell = Some(CellSize {
        width: width.max(0),
        height: height.max(0),
    });
}

/// Replace the DOM holes from a JSON array of rectangles. Fail-closed: an
/// unparseable payload clears the holes (the grid covers its whole region)
/// rather than keeping a stale set the front pane no longer matches.
pub fn set_dom_holes(state: &mut GridState, holes_json: &str) {
    match serde_json::from_str::<Vec<GridRect>>(holes_json) {
        Ok(holes) => {
            state.dom_holes = holes
                .into_iter()
                .map(|h| GridRect {
                    x: h.x,
                    y: h.y,
                    width: h.width.max(0),
                    height: h.height.max(0),
                })
                .collect();
        }
        Err(_) => {
            state.dom_holes.clear();
        }
    }
}

/// Record an app's acknowledgement of a `configure` serial. A stale ack (a
/// serial the compositor never sent, i.e. ahead of the latest configure) is
/// ignored, so `acked_serial` never runs ahead of `configure_serial`.
pub fn ack_configure(state: &mut GridState, serial: u32) {
    if serial <= state.configure_serial {
        state.acked_serial = serial;
    }
}

/// The grid region size snapped down to whole cells, so the compositor never
/// composites a partial cell at the trailing edge. A zero cell dimension (or
/// no cell metrics) leaves that axis unchanged.
pub fn cell_aligned_size(region: GridRect, cell: CellSize) -> (i32, i32) {
    let w = if cell.width > 0 {
        (region.width / cell.width) * cell.width
    } else {
        region.width
    };
    let h = if cell.height > 0 {
        (region.height / cell.height) * cell.height
    } else {
        region.height
    };
    (w, h)
}

/// The cell-aligned grid rectangle the compositor should composite, if both a
/// region and cell metrics are declared. `None` until the app has declared
/// its region (nothing to paint yet).
pub fn composited_rect(state: &GridState) -> Option<GridRect> {
    let region = state.region?;
    let (width, height) = match state.cell {
        Some(cell) => cell_aligned_size(region, cell),
        None => (region.width, region.height),
    };
    Some(GridRect {
        x: region.x,
        y: region.y,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> GridState {
        GridState::default()
    }

    #[test]
    fn set_region_records_the_rect() {
        let mut s = empty();
        set_region(&mut s, 0, 36, 800, 600);
        assert_eq!(
            s.region,
            Some(GridRect {
                x: 0,
                y: 36,
                width: 800,
                height: 600
            })
        );
    }

    #[test]
    fn set_region_clamps_negative_extent_to_zero() {
        let mut s = empty();
        set_region(&mut s, 10, 10, -5, -1);
        let r = s.region.unwrap();
        assert_eq!((r.width, r.height), (0, 0));
    }

    #[test]
    fn set_cell_size_clamps_negative() {
        let mut s = empty();
        set_cell_size(&mut s, -8, 18);
        assert_eq!(s.cell, Some(CellSize { width: 0, height: 18 }));
    }

    #[test]
    fn dom_holes_parse_round_trips() {
        let mut s = empty();
        let json = serde_json::to_string(&vec![
            GridRect { x: 0, y: 0, width: 100, height: 40 },
            GridRect { x: 0, y: 200, width: 300, height: 80 },
        ])
        .unwrap();
        set_dom_holes(&mut s, &json);
        assert_eq!(s.dom_holes.len(), 2);
        assert_eq!(s.dom_holes[1].height, 80);
    }

    #[test]
    fn dom_holes_empty_array_clears() {
        let mut s = empty();
        s.dom_holes = vec![GridRect { x: 1, y: 2, width: 3, height: 4 }];
        set_dom_holes(&mut s, "[]");
        assert!(s.dom_holes.is_empty());
    }

    #[test]
    fn dom_holes_invalid_json_fails_closed_to_empty() {
        let mut s = empty();
        s.dom_holes = vec![GridRect { x: 1, y: 2, width: 3, height: 4 }];
        set_dom_holes(&mut s, "not json {{{");
        assert!(s.dom_holes.is_empty());
    }

    #[test]
    fn dom_holes_clamp_negative_extent() {
        let mut s = empty();
        set_dom_holes(
            &mut s,
            r#"[{"x":5,"y":5,"width":-10,"height":-2}]"#,
        );
        assert_eq!((s.dom_holes[0].width, s.dom_holes[0].height), (0, 0));
    }

    #[test]
    fn ack_records_a_valid_serial() {
        let mut s = empty();
        s.configure_serial = 3;
        ack_configure(&mut s, 3);
        assert_eq!(s.acked_serial, 3);
    }

    #[test]
    fn ack_ignores_a_serial_the_compositor_never_sent() {
        let mut s = empty();
        s.configure_serial = 1;
        ack_configure(&mut s, 5);
        assert_eq!(s.acked_serial, 0);
    }

    #[test]
    fn cell_aligned_size_snaps_down_to_whole_cells() {
        let region = GridRect { x: 0, y: 0, width: 805, height: 607 };
        let cell = CellSize { width: 8, height: 18 };
        // 805 / 8 = 100 cells -> 800; 607 / 18 = 33 cells -> 594.
        assert_eq!(cell_aligned_size(region, cell), (800, 594));
    }

    #[test]
    fn cell_aligned_size_zero_cell_leaves_axis_unchanged() {
        let region = GridRect { x: 0, y: 0, width: 805, height: 607 };
        let cell = CellSize { width: 0, height: 18 };
        assert_eq!(cell_aligned_size(region, cell), (805, 594));
    }

    #[test]
    fn composited_rect_is_none_until_a_region_is_set() {
        let mut s = empty();
        assert!(composited_rect(&s).is_none());
        set_cell_size(&mut s, 8, 18);
        assert!(composited_rect(&s).is_none());
    }

    #[test]
    fn composited_rect_aligns_to_cells_when_both_declared() {
        let mut s = empty();
        set_region(&mut s, 0, 36, 805, 607);
        set_cell_size(&mut s, 8, 18);
        assert_eq!(
            composited_rect(&s),
            Some(GridRect { x: 0, y: 36, width: 800, height: 594 })
        );
    }

    #[test]
    fn composited_rect_uses_raw_region_without_cell_metrics() {
        let mut s = empty();
        set_region(&mut s, 0, 0, 805, 607);
        assert_eq!(
            composited_rect(&s),
            Some(GridRect { x: 0, y: 0, width: 805, height: 607 })
        );
    }
}

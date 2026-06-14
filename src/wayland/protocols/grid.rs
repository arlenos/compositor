/// Server-side implementation of the `arlen-grid-v1` Wayland protocol.
///
/// The Arlen terminal declares a text-grid back pane (a subsurface, its
/// region, cell metrics, and the DOM holes the front pane owns); the
/// compositor composites the grid under the app window and drives a resize
/// handshake. Sibling of `arlen-titlebar-v1`: the app declares WHAT, the
/// compositor decides HOW.
///
/// See `docs/architecture/terminal.md` §2.2.

use std::collections::HashMap;

use smithay::reexports::wayland_server::{
    backend::GlobalId, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

pub use generated::arlen_grid_manager_v1;
pub use generated::arlen_grid_v1;

// ---------------------------------------------------------------------------
// Scanner bindings
// ---------------------------------------------------------------------------

#[allow(non_snake_case, non_upper_case_globals, non_camel_case_types)]
mod generated {
    use smithay::reexports::wayland_server::{self, protocol::*};

    pub mod __interfaces {
        use smithay::reexports::wayland_server::protocol::__interfaces::*;
        use wayland_backend;
        wayland_scanner::generate_interfaces!("resources/protocols/arlen-grid-v1.xml");
    }

    use self::__interfaces::*;

    wayland_scanner::generate_server_code!("resources/protocols/arlen-grid-v1.xml");
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// A toplevel-relative logical-pixel rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct GridRect {
    /// X origin, relative to the toplevel surface.
    pub x: i32,
    /// Y origin, relative to the toplevel surface.
    pub y: i32,
    /// Width in logical pixels.
    pub width: i32,
    /// Height in logical pixels.
    pub height: i32,
}

/// Logical-pixel metrics of one grid cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellSize {
    /// Cell width in logical pixels.
    pub width: i32,
    /// Cell height in logical pixels.
    pub height: i32,
}

/// Per-window grid back-pane state tracked by the compositor.
#[derive(Debug, Clone, Default)]
pub struct GridState {
    /// `wl_surface` protocol id of the subsurface carrying the grid, if set.
    pub grid_surface_id: Option<u64>,
    /// The toplevel-relative region the grid fills, if declared.
    pub region: Option<GridRect>,
    /// Cell metrics, if declared.
    pub cell: Option<CellSize>,
    /// Opaque DOM-block rectangles the front pane owns.
    pub dom_holes: Vec<GridRect>,
    /// Serial of the most recent `configure` the compositor sent.
    pub configure_serial: u32,
    /// Serial of the most recent configure the app acknowledged.
    pub acked_serial: u32,
}

/// Global state for the grid protocol.
#[derive(Debug)]
pub struct GridManagerState {
    #[allow(dead_code)]
    global: GlobalId,
    /// Per-surface grid state, keyed by the toplevel `wl_surface` protocol id.
    pub surfaces: HashMap<u64, GridState>,
    /// Per-surface protocol resources for sending events back to the client.
    resources: HashMap<u64, generated::arlen_grid_v1::ArlenGridV1>,
}

impl GridManagerState {
    /// Register the global and return the state.
    pub fn new(display: &DisplayHandle) -> Self
    where
        crate::state::State: GlobalDispatch<generated::arlen_grid_manager_v1::ArlenGridManagerV1, ()>
            + Dispatch<generated::arlen_grid_manager_v1::ArlenGridManagerV1, ()>
            + Dispatch<generated::arlen_grid_v1::ArlenGridV1, u64>
            + 'static,
    {
        let global = display
            .create_global::<crate::state::State, generated::arlen_grid_manager_v1::ArlenGridManagerV1, ()>(
                1,
                (),
            );
        Self {
            global,
            surfaces: HashMap::new(),
            resources: HashMap::new(),
        }
    }

    /// Get grid state for a surface (by toplevel surface id).
    pub fn get(&self, surface_id: u64) -> Option<&GridState> {
        self.surfaces.get(&surface_id)
    }

    /// Get mutable grid state, creating a default if missing.
    pub fn get_or_create(&mut self, surface_id: u64) -> &mut GridState {
        self.surfaces.entry(surface_id).or_default()
    }

    /// Store the per-surface protocol resource for sending events.
    pub fn register_resource(
        &mut self,
        surface_id: u64,
        resource: generated::arlen_grid_v1::ArlenGridV1,
    ) {
        self.resources.insert(surface_id, resource);
    }

    /// Remove a per-surface resource (called on destroy).
    pub fn unregister_resource(&mut self, surface_id: u64) {
        self.resources.remove(&surface_id);
    }

    /// Send a `configure` event offering the grid its available pixel size,
    /// bumping and recording the serial. Returns the serial sent, or `None`
    /// if the surface has no active grid binding.
    ///
    /// The compositor only composites the grid at a size the app has acked
    /// (see [`GridManagerState::is_configure_acked`]), so resize stays
    /// glitch-free.
    pub fn send_configure(&mut self, surface_id: u64, width: i32, height: i32) -> Option<u32> {
        let resource = self.resources.get(&surface_id)?.clone();
        let st = self.surfaces.entry(surface_id).or_default();
        st.configure_serial = st.configure_serial.wrapping_add(1);
        let serial = st.configure_serial;
        resource.configure(serial, width, height);
        Some(serial)
    }

    /// Whether the app has acknowledged the latest `configure` for a surface.
    /// True when there is no outstanding configure (a fresh surface that has
    /// never been configured counts as acked: both serials are 0).
    pub fn is_configure_acked(&self, surface_id: u64) -> bool {
        self.surfaces
            .get(&surface_id)
            .map(|st| st.acked_serial == st.configure_serial)
            .unwrap_or(true)
    }

    /// Check whether a surface has an active grid binding.
    pub fn has_grid(&self, surface_id: u64) -> bool {
        self.resources.contains_key(&surface_id)
    }

    /// Iterate over all surface IDs with active grid bindings.
    pub fn active_surface_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.resources.keys().copied()
    }
}

// ---------------------------------------------------------------------------
// Handler trait
// ---------------------------------------------------------------------------

/// Trait the compositor State must implement to handle grid requests.
pub trait GridHandler {
    /// Access the grid manager state.
    fn grid_manager_state(&mut self) -> &mut GridManagerState;

    /// Called after every state-mutating grid request.
    ///
    /// The implementation should recompute the grid back-pane compositing for
    /// the given surface (its clipped region minus the DOM holes).
    fn notify_grid_changed(&mut self, surface_id: u64);

    /// Called when a grid object is destroyed.
    ///
    /// The implementation should drop the grid back pane for this surface.
    fn notify_grid_removed(&mut self, surface_id: u64);
}

// ---------------------------------------------------------------------------
// GlobalDispatch: arlen_grid_manager_v1
// ---------------------------------------------------------------------------

impl<D> GlobalDispatch<generated::arlen_grid_manager_v1::ArlenGridManagerV1, (), D>
    for GridManagerState
where
    D: GlobalDispatch<generated::arlen_grid_manager_v1::ArlenGridManagerV1, ()>
        + Dispatch<generated::arlen_grid_manager_v1::ArlenGridManagerV1, ()>
        + Dispatch<generated::arlen_grid_v1::ArlenGridV1, u64>
        + GridHandler
        + 'static,
{
    fn bind(
        _state: &mut D,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<generated::arlen_grid_manager_v1::ArlenGridManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(resource, ());
    }
}

// ---------------------------------------------------------------------------
// Dispatch: arlen_grid_manager_v1 (factory requests)
// ---------------------------------------------------------------------------

impl<D> Dispatch<generated::arlen_grid_manager_v1::ArlenGridManagerV1, (), D> for GridManagerState
where
    D: Dispatch<generated::arlen_grid_manager_v1::ArlenGridManagerV1, ()>
        + Dispatch<generated::arlen_grid_v1::ArlenGridV1, u64>
        + GridHandler
        + 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        _resource: &generated::arlen_grid_manager_v1::ArlenGridManagerV1,
        request: generated::arlen_grid_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            generated::arlen_grid_manager_v1::Request::GetGrid { id, surface } => {
                // Use the toplevel wl_surface ID as our surface key.
                let surface_id = surface.id().protocol_id() as u64;
                let mgr = state.grid_manager_state();
                mgr.get_or_create(surface_id);
                let resource = data_init.init(id, surface_id);
                state
                    .grid_manager_state()
                    .register_resource(surface_id, resource);
            }
            generated::arlen_grid_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Dispatch: arlen_grid_v1 (per-surface requests)
// ---------------------------------------------------------------------------

impl<D> Dispatch<generated::arlen_grid_v1::ArlenGridV1, u64, D> for GridManagerState
where
    D: Dispatch<generated::arlen_grid_v1::ArlenGridV1, u64> + GridHandler + 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        _resource: &generated::arlen_grid_v1::ArlenGridV1,
        request: generated::arlen_grid_v1::Request,
        surface_id: &u64,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        use crate::wayland::handlers::grid::*;

        let sid = *surface_id;
        let mut changed = true;

        {
            let st = state.grid_manager_state().get_or_create(sid);

            match request {
                generated::arlen_grid_v1::Request::SetGridSurface { surface } => {
                    st.grid_surface_id = Some(surface.id().protocol_id() as u64);
                }
                generated::arlen_grid_v1::Request::SetRegion {
                    x,
                    y,
                    width,
                    height,
                } => {
                    set_region(st, x, y, width, height);
                }
                generated::arlen_grid_v1::Request::SetCellSize { width, height } => {
                    set_cell_size(st, width, height);
                }
                generated::arlen_grid_v1::Request::SetDomHoles { holes_json } => {
                    set_dom_holes(st, &holes_json);
                }
                generated::arlen_grid_v1::Request::AckConfigure { serial } => {
                    ack_configure(st, serial);
                    // An ack carries no new compositing geometry by itself.
                    changed = false;
                }
                generated::arlen_grid_v1::Request::Destroy => {
                    changed = false;
                }
                _ => {
                    changed = false;
                }
            }
        }

        // Borrow on grid_manager_state is dropped; safe to notify now.
        if changed {
            state.notify_grid_changed(sid);
        }
    }

    fn destroyed(
        state: &mut D,
        _client: wayland_backend::server::ClientId,
        _resource: &generated::arlen_grid_v1::ArlenGridV1,
        surface_id: &u64,
    ) {
        let sid = *surface_id;
        let mgr = state.grid_manager_state();
        mgr.surfaces.remove(&sid);
        mgr.unregister_resource(sid);
        state.notify_grid_removed(sid);
    }
}

// ---------------------------------------------------------------------------
// Delegate macro
// ---------------------------------------------------------------------------

/// Delegates `GlobalDispatch` and `Dispatch` for the grid protocol to
/// `GridManagerState`.
#[macro_export]
macro_rules! delegate_grid {
    ($ty:ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($ty: [
            $crate::wayland::protocols::grid::arlen_grid_manager_v1::ArlenGridManagerV1: ()
        ] => $crate::wayland::protocols::grid::GridManagerState);
        smithay::reexports::wayland_server::delegate_dispatch!($ty: [
            $crate::wayland::protocols::grid::arlen_grid_manager_v1::ArlenGridManagerV1: ()
        ] => $crate::wayland::protocols::grid::GridManagerState);
        smithay::reexports::wayland_server::delegate_dispatch!($ty: [
            $crate::wayland::protocols::grid::arlen_grid_v1::ArlenGridV1: u64
        ] => $crate::wayland::protocols::grid::GridManagerState);
    };
}

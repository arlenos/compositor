// SPDX-License-Identifier: GPL-3.0-only

//! Server side of `arlen-wallpaper-pause-v1`: tells a background-layer client
//! whether it is running, covered, or paused because the session is locked.
//!
//! The pause itself is the frame callbacks the compositor withholds; this only
//! says why. The state is worked out in `State::send_frames`, which runs for
//! every layer surface of an output on every frame whether or not the surface
//! is visible, so a change is reported on the frame it happens.

use std::sync::Mutex;

use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
    protocol::wl_surface::WlSurface,
};
use wayland_backend::server::GlobalId;

pub use generated::arlen_wallpaper_pause_manager_v1;
use generated::arlen_wallpaper_pause_manager_v1::{
    ArlenWallpaperPauseManagerV1, Request as ManagerRequest,
};
pub use generated::arlen_wallpaper_pause_v1;
use generated::arlen_wallpaper_pause_v1::{
    ArlenWallpaperPauseV1, Request as PauseRequest, State as PauseKind,
};

#[allow(non_snake_case, non_upper_case_globals, non_camel_case_types)]
mod generated {
    use smithay::reexports::wayland_server::{self, protocol::*};

    pub mod __interfaces {
        use smithay::reexports::wayland_server::protocol::__interfaces::*;
        use wayland_backend;
        wayland_scanner::generate_interfaces!("resources/protocols/arlen-wallpaper-pause-v1.xml");
    }

    use self::__interfaces::*;

    wayland_scanner::generate_server_code!("resources/protocols/arlen-wallpaper-pause-v1.xml");
}

/// Why a watched surface is or is not being shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Running,
    Covered,
    Locked,
}

impl Visibility {
    /// Locked wins over covered: a locked session covers the wallpaper too,
    /// and the lock is the reason a client acts on.
    pub fn of(locked: bool, visible: bool) -> Self {
        if locked {
            Visibility::Locked
        } else if visible {
            Visibility::Running
        } else {
            Visibility::Covered
        }
    }

    fn wire(self) -> PauseKind {
        match self {
            Visibility::Running => PauseKind::Running,
            Visibility::Covered => PauseKind::Covered,
            Visibility::Locked => PauseKind::Locked,
        }
    }
}

/// Per pause object: the surface it describes and what it was last told.
#[derive(Debug)]
pub struct PauseData {
    surface: WlSurface,
    last: Mutex<Option<Visibility>>,
}

#[derive(Debug)]
pub struct WallpaperPauseState {
    pauses: Vec<ArlenWallpaperPauseV1>,
    /// Held to keep the global registered. Never read.
    _global: GlobalId,
}

impl WallpaperPauseState {
    pub fn new<D>(dh: &DisplayHandle) -> Self
    where
        D: GlobalDispatch<ArlenWallpaperPauseManagerV1, ()>
            + Dispatch<ArlenWallpaperPauseManagerV1, ()>
            + Dispatch<ArlenWallpaperPauseV1, PauseData>
            + WallpaperPauseHandler
            + 'static,
    {
        let global = dh.create_global::<D, ArlenWallpaperPauseManagerV1, _>(1, ());
        Self {
            pauses: Vec::new(),
            _global: global,
        }
    }

    /// Report `visibility` for `surface` to every pause object watching it,
    /// if it changed since the last report.
    pub fn report(&self, surface: &WlSurface, visibility: Visibility) {
        for pause in &self.pauses {
            let Some(data) = pause.data::<PauseData>() else {
                continue;
            };
            if &data.surface != surface || !pause.is_alive() {
                continue;
            }
            let mut last = data.last.lock().unwrap();
            if *last != Some(visibility) {
                pause.state(visibility.wire());
                *last = Some(visibility);
            }
        }
    }

    /// Whether anything watches `surface` at all - lets the frame loop skip the
    /// work for the common case of nobody asking.
    pub fn watches(&self, surface: &WlSurface) -> bool {
        self.pauses
            .iter()
            .any(|p| p.data::<PauseData>().is_some_and(|d| &d.surface == surface))
    }
}

pub trait WallpaperPauseHandler {
    fn wallpaper_pause_state(&mut self) -> &mut WallpaperPauseState;
}

impl<D> GlobalDispatch<ArlenWallpaperPauseManagerV1, (), D> for WallpaperPauseState
where
    D: GlobalDispatch<ArlenWallpaperPauseManagerV1, ()>
        + Dispatch<ArlenWallpaperPauseManagerV1, ()>
        + Dispatch<ArlenWallpaperPauseV1, PauseData>
        + WallpaperPauseHandler
        + 'static,
{
    fn bind(
        _state: &mut D,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ArlenWallpaperPauseManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(resource, ());
    }
}

impl<D> Dispatch<ArlenWallpaperPauseManagerV1, (), D> for WallpaperPauseState
where
    D: Dispatch<ArlenWallpaperPauseManagerV1, ()>
        + Dispatch<ArlenWallpaperPauseV1, PauseData>
        + WallpaperPauseHandler
        + 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        _resource: &ArlenWallpaperPauseManagerV1,
        request: ManagerRequest,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ManagerRequest::GetPause { id, surface } => {
                // A client can only name its own wl_surface - object ids are
                // per client - so no ownership check is needed here.
                let pause = data_init.init(
                    id,
                    PauseData {
                        surface,
                        last: Mutex::new(None),
                    },
                );
                state.wallpaper_pause_state().pauses.push(pause);
            }
            ManagerRequest::Destroy => {}
        }
    }
}

impl<D> Dispatch<ArlenWallpaperPauseV1, PauseData, D> for WallpaperPauseState
where
    D: Dispatch<ArlenWallpaperPauseV1, PauseData> + WallpaperPauseHandler + 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        resource: &ArlenWallpaperPauseV1,
        request: PauseRequest,
        _data: &PauseData,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            PauseRequest::Destroy => {
                state
                    .wallpaper_pause_state()
                    .pauses
                    .retain(|p| p != resource);
            }
        }
    }

    fn destroyed(
        state: &mut D,
        _client: wayland_backend::server::ClientId,
        resource: &ArlenWallpaperPauseV1,
        _data: &PauseData,
    ) {
        // Also reached when the client disconnects without destroying.
        state
            .wallpaper_pause_state()
            .pauses
            .retain(|p| p != resource);
    }
}

#[macro_export]
macro_rules! delegate_wallpaper_pause {
    ($ty:ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($ty: [
            $crate::wayland::protocols::wallpaper_pause::arlen_wallpaper_pause_manager_v1::ArlenWallpaperPauseManagerV1: ()
        ] => $crate::wayland::protocols::wallpaper_pause::WallpaperPauseState);
        smithay::reexports::wayland_server::delegate_dispatch!($ty: [
            $crate::wayland::protocols::wallpaper_pause::arlen_wallpaper_pause_manager_v1::ArlenWallpaperPauseManagerV1: ()
        ] => $crate::wayland::protocols::wallpaper_pause::WallpaperPauseState);
        smithay::reexports::wayland_server::delegate_dispatch!($ty: [
            $crate::wayland::protocols::wallpaper_pause::arlen_wallpaper_pause_v1::ArlenWallpaperPauseV1:
                $crate::wayland::protocols::wallpaper_pause::PauseData
        ] => $crate::wayland::protocols::wallpaper_pause::WallpaperPauseState);
    };
}

#[cfg(test)]
mod tests {
    use super::Visibility;

    #[test]
    fn locked_wins_over_covered() {
        assert_eq!(Visibility::of(true, false), Visibility::Locked);
        assert_eq!(Visibility::of(true, true), Visibility::Locked);
        assert_eq!(Visibility::of(false, false), Visibility::Covered);
        assert_eq!(Visibility::of(false, true), Visibility::Running);
    }
}

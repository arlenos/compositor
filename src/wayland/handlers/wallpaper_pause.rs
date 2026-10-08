// SPDX-License-Identifier: GPL-3.0-only

//! Ties `arlen-wallpaper-pause-v1` to `State`. The reporting happens in
//! `State::send_frames`.

use crate::{
    delegate_wallpaper_pause,
    state::State,
    wayland::protocols::wallpaper_pause::{WallpaperPauseHandler, WallpaperPauseState},
};

impl WallpaperPauseHandler for State {
    fn wallpaper_pause_state(&mut self) -> &mut WallpaperPauseState {
        &mut self.common.wallpaper_pause_state
    }
}

delegate_wallpaper_pause!(State);

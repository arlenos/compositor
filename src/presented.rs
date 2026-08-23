// SPDX-License-Identifier: GPL-3.0-only

//! Which surfaces have actually reached the screen.
//!
//! A surface that has never been presented receives no input. Not a rule about
//! dialogs or about the shell: a property of the window system, so every future
//! modal inherits it without knowing it exists.
//!
//! WHY THIS LIVES HERE AND NOT IN THE APP. A window that is mapped but has not
//! yet painted still occupies its place in the input region, so a click that
//! lands in the moment between the two is delivered to a surface the person
//! could not see. The consent dialog made that concrete: measured on the real
//! machine, the card reached the screen up to five seconds after the window was
//! raised, and a click driven into that gap was answered by a dialog nobody had
//! read yet. Every candidate signal inside the app was tried first and none of
//! them can mean "on screen" - GTK ticks a frame clock, WebKit composites
//! later, and neither process is where the pixels are decided. This one is.
//!
//! WHAT COUNTS AS PRESENTED. The compositor sends a surface its frame callback
//! after the frame that contained it was submitted, so the first callback is
//! the first moment its buffer was on its way to the display. That is the
//! signal marked here. It is per-surface and it never goes back to false: the
//! question is "has this ever been shown", not "is it visible right now" - a
//! window that has been read once and is then covered is still a window the
//! person has seen.

use std::cell::Cell;

use smithay::{
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    wayland::{
        compositor::{with_states, SurfaceData},
        shell::xdg::XdgToplevelSurfaceData,
    },
};

/// Set once the surface's first frame callback goes out, and never cleared.
#[derive(Default)]
struct EverPresented(Cell<bool>);

/// Record that this surface was part of a frame the compositor submitted.
///
/// Called from the frame-callback path, which is per-surface and already walks
/// the whole tree, so subsurfaces and popups are marked in their own right
/// rather than inheriting their parent's answer.
///
/// The first presentation is logged, once per surface. Without it the rule is
/// invisible from outside: a boot where it never fires and a boot where it
/// silently passes everything look identical, and the question this exists to
/// answer - how long is the gap between a window being raised and being on the
/// screen - can only be answered by something that sits where the frames are.
pub fn mark_presented(states: &SurfaceData) {
    let flag = states.data_map.get_or_insert(EverPresented::default);
    if flag.0.replace(true) {
        return;
    }
    // Named where the surface can name itself. The first cut logged ONLY
    // toplevels carrying an app_id and printed nothing at all on a real boot,
    // which read as "the rule never runs" when the rule was running fine - the
    // surfaces going through here are mostly not toplevels, and a toplevel's
    // app_id can still be unset the first time it is drawn. A log that is silent
    // in the interesting case is worse than none: it invents a second question.
    let what = states
        .data_map
        .get::<XdgToplevelSurfaceData>()
        .and_then(|d| d.lock().unwrap().app_id.clone())
        .unwrap_or_else(|| {
            states
                .role
                .map(|r| format!("<{r}>"))
                .unwrap_or_else(|| "<no role>".into())
        });
    tracing::info!("presented: {what} reached the screen for the first time");
}

/// Has this surface ever been on screen?
///
/// A surface the compositor has never drawn answers `false`, which is the whole
/// point; so does a destroyed one, which is the safe answer for input routing.
pub fn has_been_presented(surface: &WlSurface) -> bool {
    if !surface.is_alive() {
        return false;
    }
    with_states(surface, |states| {
        states
            .data_map
            .get::<EverPresented>()
            .is_some_and(|p| p.0.get())
    })
}

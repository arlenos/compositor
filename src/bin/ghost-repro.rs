//! Reproduce the stale-frame ghost headlessly, with no input injection.
//!
//! Every shell overlay leaves its last delivered frame on screen when it closes,
//! and the finding has been expensive: it made a stale waypointer frame look like
//! a duplicate-rendering bug convincingly enough to be filed as one. Chasing it
//! through the VM costs a boot per attempt and needs a click, because pointer
//! motion alone does not recomposite a stale scene.
//!
//! This client needs neither. It paints a large opaque block, waits, then shrinks
//! or unmaps itself on its own timer, and holds the connection open so a capture
//! lands after the change. Whatever is left in the region it vacated is the answer:
//! the desktop background if compositing is correct, the block's own colour if the
//! ghost is real.
//!
//!     WAYLAND_DISPLAY=wayland-1 ghost-repro          # shrink: 600x600 -> 200x200
//!     WAYLAND_DISPLAY=wayland-1 ghost-repro unmap    # map, then attach a null buffer
//!
//! The colour is deliberately saturated magenta, which appears nowhere in the
//! theme, so a pixel readback can attribute it without ambiguity.

use std::io::Write;
use std::os::unix::io::AsFd;
use std::time::Duration;

use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{
        wl_buffer::WlBuffer,
        wl_compositor::WlCompositor,
        wl_registry::{self, WlRegistry},
        wl_shm::{self, WlShm},
        wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
};
use wayland_protocols::xdg::shell::client::{
    xdg_surface::{self, XdgSurface},
    xdg_toplevel::{self, XdgToplevel},
    xdg_wm_base::{self, XdgWmBase},
};

const BIG: i32 = 600;
const SMALL: i32 = 200;
/// Opaque magenta in ARGB8888, a colour the theme never produces.
const FILL: u32 = 0xFFFF00FF;

struct App {
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    wm_base: Option<XdgWmBase>,
    configured: bool,
}

impl Dispatch<WlRegistry, ()> for App {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_compositor" => state.compositor = Some(registry.bind(name, version.min(4), qh, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, version.min(1), qh, ())),
                "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(2), qh, ())),
                _ => {}
            }
        }
    }
}

macro_rules! ignore_events {
    ($($ty:ty => $ev:ty),* $(,)?) => {
        $(impl Dispatch<$ty, ()> for App {
            fn event(_: &mut Self, _: &$ty, _: $ev, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
        })*
    };
}

ignore_events! {
    WlCompositor => wayland_client::protocol::wl_compositor::Event,
    WlShm => wl_shm::Event,
    WlShmPool => wayland_client::protocol::wl_shm_pool::Event,
    WlBuffer => wayland_client::protocol::wl_buffer::Event,
    WlSurface => wayland_client::protocol::wl_surface::Event,
}

impl Dispatch<XdgWmBase, ()> for App {
    fn event(
        _: &mut Self,
        wm_base: &XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<XdgSurface, ()> for App {
    fn event(
        state: &mut Self,
        xdg_surface: &XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg_surface.ack_configure(serial);
            state.configured = true;
        }
    }
}

impl Dispatch<XdgToplevel, ()> for App {
    fn event(
        _: &mut Self,
        _: &XdgToplevel,
        _: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// A wl_shm buffer of one flat colour. Each call takes its own pool, because the
/// two sizes here are not worth the arithmetic of sharing one.
fn solid_buffer(
    shm: &WlShm,
    qh: &QueueHandle<App>,
    width: i32,
    height: i32,
    argb: u32,
) -> Result<WlBuffer, Box<dyn std::error::Error>> {
    let stride = width * 4;
    let size = (stride * height) as usize;

    let mut file = tempfile::tempfile()?;
    let row = argb.to_ne_bytes().repeat(width as usize);
    for _ in 0..height {
        file.write_all(&row)?;
    }
    file.flush()?;

    let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
    Ok(pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, qh, ()))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let unmap = std::env::args().nth(1).as_deref() == Some("unmap");

    let conn = Connection::connect_to_env()?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();

    let mut state = App { compositor: None, shm: None, wm_base: None, configured: false };
    let _registry = conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut state)?;

    let compositor = state.compositor.clone().ok_or("no wl_compositor")?;
    let shm = state.shm.clone().ok_or("no wl_shm")?;
    let wm_base = state.wm_base.clone().ok_or("no xdg_wm_base")?;

    let surface = compositor.create_surface(&qh, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg_surface.get_toplevel(&qh, ());
    toplevel.set_title("ghost-repro".to_owned());
    toplevel.set_app_id("arlen.ghost-repro".to_owned());
    surface.commit();

    while !state.configured {
        queue.roundtrip(&mut state)?;
    }

    let big = solid_buffer(&shm, &qh, BIG, BIG, FILL)?;
    surface.attach(Some(&big), 0, 0);
    surface.damage_buffer(0, 0, BIG, BIG);
    surface.commit();
    queue.roundtrip(&mut state)?;

    // Long enough that the big frame is unambiguously on screen before anything
    // changes: a capture racing the first paint would show an empty region and
    // read as a pass.
    std::thread::sleep(Duration::from_millis(1500));
    eprintln!("ghost-repro: {BIG}x{BIG} painted");

    if unmap {
        // The overlay case: the surface stays alive and keeps its role, it just
        // stops presenting. This is what a hidden shell window does.
        surface.attach(None, 0, 0);
        surface.commit();
        eprintln!("ghost-repro: unmapped, region {BIG}x{BIG} should be background");
    } else {
        let small = solid_buffer(&shm, &qh, SMALL, SMALL, FILL)?;
        surface.attach(Some(&small), 0, 0);
        surface.damage_buffer(0, 0, SMALL, SMALL);
        surface.commit();
        eprintln!("ghost-repro: shrunk to {SMALL}x{SMALL}, the rest should be background");
    }
    queue.roundtrip(&mut state)?;

    // Hold the connection so the capture lands after the change. Dropping it here
    // would destroy the surface and take the ghost with it.
    std::thread::sleep(Duration::from_secs(20));
    Ok(())
}

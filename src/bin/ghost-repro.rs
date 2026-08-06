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
//!     ghost-repro                 # layer surface, then a null buffer (the default)
//!     ghost-repro layer 60000     # the same, held up: the positive control
//!     ghost-repro toplevel        # xdg toplevel, then a null buffer
//!     ghost-repro shrink          # xdg toplevel, 600x600 -> 200x200
//!     ghost-repro partial         # layer surface, repaint half, damage half
//!
//! The `layer` mode is the one that matches the report, which says "every shell
//! **overlay**" - overlays are layer surfaces. The toplevel modes came first and
//! answered a narrower question: `unmap` comes back clean, so the generic "a
//! surface stops presenting and the compositor repaints" path is not at fault, and
//! `shrink` turns out to test nothing here because this is a tiling compositor and
//! the layout owns toplevel geometry, so a smaller buffer vacates no region.
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
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};

const BIG: i32 = 600;
const SMALL: i32 = 200;
/// Opaque magenta in ARGB8888, a colour the theme never produces.
const FILL: u32 = 0xFFFF00FF;
/// The colour the second frame puts where the first had magenta. Distinct from both
/// the fill and the desktop, so the readback says which frame the pixels came from
/// rather than merely that they changed.
/// Pure green: it has to be far from the desktop grey as well as from the fill,
/// because a readback that tolerates a few levels will otherwise classify the
/// background as the new content and report a pass. That happened once already.
const DARK: u32 = 0xFF00FF00;
/// What the second frame paints in the half it DOES damage.
const LIVE: u32 = 0xFF0080FF;

struct App {
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    wm_base: Option<XdgWmBase>,
    layer_shell: Option<ZwlrLayerShellV1>,
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
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()))
                }
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
    ZwlrLayerShellV1 => wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::Event,
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for App {
    fn event(
        state: &mut Self,
        layer_surface: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure { serial, .. } = event {
            layer_surface.ack_configure(serial);
            state.configured = true;
        }
    }
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

/// What the client does after its first frame is on screen.
#[derive(PartialEq)]
enum Mode {
    /// Toplevel, then a smaller buffer. Tests nothing here; kept because someone
    /// will otherwise rebuild it to find that out again.
    Shrink,
    /// Toplevel, then a null buffer.
    Unmap,
    /// Layer surface, then a null buffer. The overlay case.
    Layer,
    /// Layer surface of a fixed size that repaints only part of itself and reports
    /// damage for only that part. This is the shape the waypointer ghost actually
    /// has: a panel whose content shrank between two frames, leaving the strip it
    /// vacated showing the earlier frame.
    ///
    /// Read the result carefully, because the obvious reading is backwards. A
    /// compositor is entitled to re-upload only the damaged region; the protocol
    /// puts the duty to report damage on the client. So a stale strip here means the
    /// compositor is behaving correctly and an under-reporting client is the fault,
    /// while a cleared strip means damage is not what governs the region and the
    /// ghost has some other cause.
    Partial,
}

/// A buffer whose top `split` rows are `top` and whose remainder is `bottom`. This
/// models a panel that shrank: the same surface, the same size, new content in part
/// of it.
fn split_buffer(
    shm: &WlShm,
    qh: &QueueHandle<App>,
    width: i32,
    height: i32,
    split: i32,
    top: u32,
    bottom: u32,
) -> Result<WlBuffer, Box<dyn std::error::Error>> {
    let stride = width * 4;
    let size = (stride * height) as usize;

    let mut file = tempfile::tempfile()?;
    let top_row = top.to_ne_bytes().repeat(width as usize);
    let bottom_row = bottom.to_ne_bytes().repeat(width as usize);
    for y in 0..height {
        file.write_all(if y < split { &top_row } else { &bottom_row })?;
    }
    file.flush()?;

    let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
    Ok(pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, qh, ()))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Layer is the default because of how this reaches the VM: the boot harness
    // launches a verify app through the SMBIOS SKU, which is sanitised to a bare
    // binary name and deliberately carries no arguments, so whatever runs with no
    // arguments is what gets tested against KMS. That should be the case under
    // suspicion, not the one documented above as testing nothing.
    let mode = match std::env::args().nth(1).as_deref() {
        Some("shrink") => Mode::Shrink,
        Some("partial") => Mode::Partial,
        Some("toplevel") | Some("unmap") => Mode::Unmap,
        _ => Mode::Layer,
    };

    let conn = Connection::connect_to_env()?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();

    let mut state =
        App { compositor: None, shm: None, wm_base: None, layer_shell: None, configured: false };
    let _registry = conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut state)?;

    let compositor = state.compositor.clone().ok_or("no wl_compositor")?;
    let shm = state.shm.clone().ok_or("no wl_shm")?;

    let surface = compositor.create_surface(&qh, ());

    // The role object has to outlive the run: dropping it destroys the role, which
    // tears the surface down and takes the very thing under test with it. The two
    // roles have no common trait worth inventing, so each arm keeps its own binding.
    let mut _layer_role = None;
    let mut _xdg_role = None;
    if mode == Mode::Layer || mode == Mode::Partial {
        let layer_shell = state.layer_shell.clone().ok_or("no zwlr_layer_shell_v1")?;
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            None,
            Layer::Overlay,
            "ghost-repro".to_owned(),
            &qh,
            (),
        );
        // Sized rather than anchored to all four edges, so the region it vacates is
        // a definite rectangle instead of the whole output.
        layer_surface.set_size(BIG as u32, BIG as u32);
        _layer_role = Some(layer_surface);
    } else {
        let wm_base = state.wm_base.clone().ok_or("no xdg_wm_base")?;
        let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());
        let toplevel = xdg_surface.get_toplevel(&qh, ());
        toplevel.set_title("ghost-repro".to_owned());
        toplevel.set_app_id("arlen.ghost-repro".to_owned());
        _xdg_role = Some((xdg_surface, toplevel));
    }
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
    //
    // The second argument overrides the wait, which is how the positive control is
    // run: hold long enough that the capture lands while the block is still up. A
    // clean shot after the change proves nothing unless the same harness has been
    // shown to photograph the block when it IS there, and this test is otherwise
    // one that cannot fail.
    // The no-argument default is long on purpose, for the same reason: in the VM the
    // only control the harness has is WHEN it screenshots. A 25s paint followed by an
    // unmap means one boot at --wait 20 photographs the block (the control) and one at
    // --wait 40 photographs what is left (the test), from the same binary, without ever
    // passing it an argument. On the host, where arguments work, pass one.
    let hold_ms: u64 = std::env::args()
        .nth(2)
        .and_then(|a| a.parse().ok())
        .unwrap_or(25_000);
    std::thread::sleep(Duration::from_millis(hold_ms));
    eprintln!("ghost-repro: {BIG}x{BIG} painted, held {hold_ms}ms");

    if mode == Mode::Partial {
        // Same surface, same size, new buffer: magenta on top, dark below. Damage
        // covers only the top half, which is the under-report being modelled.
        let split = BIG / 2;
        // Three colours, not two. If the damaged half also came up magenta there
        // would be no way to tell "the compositor kept the old pixels below" from
        // "the compositor ignored the commit entirely", and those have different
        // culprits. Blue on top proves the new buffer was taken.
        let next = split_buffer(&shm, &qh, BIG, BIG, split, LIVE, DARK)?;
        surface.attach(Some(&next), 0, 0);
        surface.damage_buffer(0, 0, BIG, split);
        surface.commit();
        eprintln!(
            "ghost-repro: frame 2 is blue above {split} and green below, damage covers \
             only the top; blue on screen means the buffer was taken, and magenta \
             below means the undamaged half kept frame 1"
        );
    } else if mode != Mode::Shrink {
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

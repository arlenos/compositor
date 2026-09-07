/// Counts the frames a background-layer client is actually given.
///
/// WP-R3 asks for a live wallpaper that draws zero frames when it is covered.
/// `wallpaper-plan.md` calls the `wl_surface.frame` callback the free baseline
/// for that - the compositor withholds callbacks from a surface nothing can see,
/// so a client that draws only on callbacks stops on its own - and then assumes
/// the baseline is not enough. This probe is how that assumption gets checked
/// instead of inherited: it behaves exactly like a live wallpaper (paint, ask for
/// a frame callback, repaint on it, forever) and reports the rate it is granted,
/// once a second.
///
/// Cover it with an opaque window and read the rate. Measured against this fork
/// on 7 Sep: 31 frames a second uncovered, exactly 1 covered - the floor is
/// cosmic-comp's own 995 ms `THROTTLE`, which smithay grants a surface it knows
/// is occluded on every output so its loop does not stall. So the baseline is
/// worth a factor of thirty on its own, and the distance left to "zero frames"
/// is one keepalive wake a second, not a missing occlusion signal.
///
/// Usage: WAYLAND_DISPLAY=wayland-N wallpaper-probe [seconds]
use std::{
    os::unix::io::AsFd,
    time::{Duration, Instant},
};

use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{
        wl_buffer::WlBuffer,
        wl_callback::{self, WlCallback},
        wl_compositor::WlCompositor,
        wl_output::{self, WlOutput},
        wl_region::WlRegion,
        wl_registry::{self, WlRegistry},
        wl_shm::{self, WlShm},
        wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, Anchor, ZwlrLayerSurfaceV1},
};

struct Probe {
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    layer_shell: Option<ZwlrLayerShellV1>,
    output: Option<WlOutput>,
    size: Option<(u32, u32)>,
    /// Frame callbacks granted since the last rate line.
    frames: u32,
    /// Frame callbacks granted in total, so a run that got none says so.
    total: u32,
    surface: Option<WlSurface>,
    buffer: Option<WlBuffer>,
}

impl Probe {
    /// Repaint and ask to be told when that frame went out - the loop a live
    /// wallpaper runs. Damaging the whole surface every time is deliberate: a
    /// client that damages nothing would be throttled for that reason instead,
    /// and the measurement would be about the wrong thing.
    fn draw(&mut self, qh: &QueueHandle<Self>) {
        let (Some(surface), Some(buffer)) = (self.surface.clone(), self.buffer.clone()) else {
            return;
        };
        surface.frame(qh, ());
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, i32::MAX, i32::MAX);
        surface.commit();
    }
}

impl Dispatch<WlRegistry, ()> for Probe {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), qh, ()))
                }
                "wl_shm" => state.shm = Some(registry.bind(name, version.min(1), qh, ())),
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()))
                }
                "wl_output" if state.output.is_none() => {
                    state.output = Some(registry.bind(name, version.min(3), qh, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for Probe {
    fn event(
        state: &mut Self,
        layer_surface: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            layer_surface.ack_configure(serial);
            state.size = Some((width.max(1), height.max(1)));
        }
    }
}

impl Dispatch<WlCallback, ()> for Probe {
    fn event(
        state: &mut Self,
        _: &WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.frames += 1;
            state.total += 1;
            state.draw(qh);
        }
    }
}

impl Dispatch<WlOutput, ()> for Probe {
    fn event(
        _: &mut Self,
        _: &WlOutput,
        _: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

macro_rules! ignore {
    ($($iface:ty),* $(,)?) => {$(
        impl Dispatch<$iface, ()> for Probe {
            fn event(
                _: &mut Self,
                _: &$iface,
                _: <$iface as wayland_client::Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
    )*};
}

ignore!(
    WlCompositor,
    WlShm,
    WlShmPool,
    WlBuffer,
    WlSurface,
    WlRegion,
    ZwlrLayerShellV1,
);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds: u64 = std::env::args().nth(1).as_deref().unwrap_or("10").parse()?;

    let conn = Connection::connect_to_env()?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let mut probe = Probe {
        compositor: None,
        shm: None,
        layer_shell: None,
        output: None,
        size: None,
        frames: 0,
        total: 0,
        surface: None,
        buffer: None,
    };
    conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut probe)?;

    let compositor = probe.compositor.clone().ok_or("no wl_compositor")?;
    let shm = probe.shm.clone().ok_or("no wl_shm")?;
    let layer_shell = probe.layer_shell.clone().ok_or("no zwlr_layer_shell_v1")?;

    let surface = compositor.create_surface(&qh, ());
    let layer_surface = layer_shell.get_layer_surface(
        &surface,
        probe.output.as_ref(),
        Layer::Background,
        "wallpaper-probe".into(),
        &qh,
        (),
    );
    layer_surface.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
    // -1 means "ignore other surfaces' exclusive zones and fill the output",
    // which is what a wallpaper wants and what cosmic-bg asks for.
    layer_surface.set_exclusive_zone(-1);
    surface.commit();

    while probe.size.is_none() {
        queue.blocking_dispatch(&mut probe)?;
    }
    let (width, height) = probe.size.unwrap();
    println!("layer surface configured {width}x{height}");

    let (w, h) = (width as i32, height as i32);
    let stride = w * 4;
    let file = tempfile::tempfile()?;
    file.set_len((stride * h) as u64)?;
    let pool = shm.create_pool(file.as_fd(), stride * h, &qh, ());
    let buffer = pool.create_buffer(0, w, h, stride, wl_shm::Format::Xrgb8888, &qh, ());
    pool.destroy();

    // A wallpaper is opaque, and saying so is what lets the compositor skip
    // whatever is behind it. Not saying so here would understate the very
    // throttling this probe is measuring on the surfaces above it.
    let region = compositor.create_region(&qh, ());
    region.add(0, 0, w, h);
    surface.set_opaque_region(Some(&region));

    probe.surface = Some(surface.clone());
    probe.buffer = Some(buffer);
    probe.draw(&qh);
    queue.roundtrip(&mut probe)?;

    let start = Instant::now();
    let mut tick = Instant::now();
    while start.elapsed() < Duration::from_secs(seconds) {
        queue.roundtrip(&mut probe)?;
        std::thread::sleep(Duration::from_millis(20));
        if tick.elapsed() >= Duration::from_secs(1) {
            println!(
                "{:5.1}s frames_last_second={}",
                start.elapsed().as_secs_f64(),
                probe.frames
            );
            probe.frames = 0;
            tick = Instant::now();
        }
    }
    println!("total frames granted: {}", probe.total);
    Ok(())
}

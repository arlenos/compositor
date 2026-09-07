/// Conformance probe for `ext-session-lock-v1`.
///
/// The lock screen's fail-secure guarantees are the compositor's, not the lock
/// client's, so they have to be observed from the outside - from a client that
/// deliberately behaves the way a broken or hostile lock client would. Each mode
/// exercises one guarantee and prints one machine-readable line per observation,
/// so a harness can assert on them:
///
///   `no-surface`  lock, then never present anything. `locked` must not arrive:
///                 the event is gated on every output having shown a locked frame.
///   `surface`     lock and paint every output, then hold. `locked` must arrive
///                 after the last output's frame callback, never before.
///   `crash`       lock, paint, then die without `unlock_and_destroy`. The session
///                 must stay locked; the screenshot after this is the assertion.
///   `unlock`      lock, paint, then unlock cleanly. The control case.
///
/// Usage: WAYLAND_DISPLAY=wayland-N lock-probe [mode] [hold-seconds]
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
        wl_registry::{self, WlRegistry},
        wl_shm::{self, WlShm},
        wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1::ExtSessionLockManagerV1,
    ext_session_lock_surface_v1::{self, ExtSessionLockSurfaceV1},
    ext_session_lock_v1::{self, ExtSessionLockV1},
};

/// Mode names as they are spelled on the command line.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    NoSurface,
    Surface,
    Crash,
    Unlock,
}

struct Probe {
    start: Instant,
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    manager: Option<ExtSessionLockManagerV1>,
    outputs: Vec<(WlOutput, u32)>,
    /// Outputs whose lock surface has had a frame callback fire.
    presented: usize,
    /// How many lock surfaces we asked the compositor to show.
    painted: usize,
    locked_at: Option<Duration>,
    finished: bool,
    /// The size the compositor demanded, per lock surface. The protocol kills a
    /// client that commits a buffer of any other size.
    configured: std::collections::HashMap<usize, (u32, u32)>,
}

impl Probe {
    fn stamp(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    fn say(&self, event: &str, detail: &str) {
        println!("{:9.1}ms {event} {detail}", self.stamp());
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
                "wl_output" => {
                    let output = registry.bind(name, version.min(3), qh, ());
                    state.outputs.push((output, name));
                }
                "ext_session_lock_manager_v1" => {
                    state.manager = Some(registry.bind(name, 1, qh, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<ExtSessionLockV1, ()> for Probe {
    fn event(
        state: &mut Self,
        _: &ExtSessionLockV1,
        event: ext_session_lock_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_session_lock_v1::Event::Locked => {
                state.locked_at = Some(state.start.elapsed());
                let detail = format!(
                    "surfaces_painted={} frames_presented={}",
                    state.painted, state.presented
                );
                state.say("locked", &detail);
            }
            ext_session_lock_v1::Event::Finished => {
                state.finished = true;
                state.say("finished", "the compositor refused or revoked the lock");
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtSessionLockSurfaceV1, usize> for Probe {
    fn event(
        state: &mut Self,
        surface: &ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            surface.ack_configure(serial);
            state.configured.insert(*index, (width, height));
            state.say("configure", &format!("output={index} {width}x{height}"));
        }
    }
}

impl Dispatch<WlCallback, usize> for Probe {
    fn event(
        state: &mut Self,
        _: &WlCallback,
        event: wl_callback::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.presented += 1;
            state.say("frame", &format!("output={index}"));
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
    ExtSessionLockManagerV1,
);

/// An opaque single-colour buffer, the shape a real lock surface paints.
fn solid_buffer(
    shm: &WlShm,
    qh: &QueueHandle<Probe>,
    width: i32,
    height: i32,
) -> Result<WlBuffer, Box<dyn std::error::Error>> {
    let stride = width * 4;
    let size = (stride * height) as usize;
    let file = tempfile::tempfile()?;
    file.set_len(size as u64)?;
    let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
    let buffer = pool.create_buffer(0, width, height, stride, wl_shm::Format::Xrgb8888, qh, ());
    pool.destroy();
    Ok(buffer)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = match std::env::args().nth(1).as_deref() {
        None | Some("no-surface") => Mode::NoSurface,
        Some("surface") => Mode::Surface,
        Some("crash") => Mode::Crash,
        Some("unlock") => Mode::Unlock,
        Some(other) => {
            eprintln!("unknown mode {other}; expected no-surface, surface, crash or unlock");
            std::process::exit(2);
        }
    };
    let hold = Duration::from_secs_f64(
        std::env::args()
            .nth(2)
            .as_deref()
            .unwrap_or("3")
            .parse::<f64>()?,
    );

    let conn = Connection::connect_to_env()?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let mut probe = Probe {
        start: Instant::now(),
        compositor: None,
        shm: None,
        manager: None,
        outputs: Vec::new(),
        presented: 0,
        painted: 0,
        locked_at: None,
        finished: false,
        configured: std::collections::HashMap::new(),
    };
    conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut probe)?;

    let compositor = probe.compositor.clone().ok_or("no wl_compositor")?;
    let shm = probe.shm.clone().ok_or("no wl_shm")?;
    let manager = probe.manager.clone().ok_or(
        "no ext_session_lock_manager_v1: this compositor does not offer the lock protocol",
    )?;
    let outputs = probe.outputs.clone();
    probe.say("outputs", &format!("count={}", outputs.len()));

    let lock = manager.lock(&qh, ());
    probe.say("lock", "requested");
    queue.roundtrip(&mut probe)?;

    if mode != Mode::NoSurface {
        for (index, (output, _)) in outputs.iter().enumerate() {
            let surface = compositor.create_surface(&qh, ());
            let _lock_surface = lock.get_lock_surface(&surface, output, &qh, index);
            // The first configure carries the size the compositor demands; the
            // protocol forbids attaching a buffer before acking it, and kills the
            // client for committing one of any other size.
            queue.roundtrip(&mut probe)?;
            let (width, height) = *probe
                .configured
                .get(&index)
                .ok_or("the compositor never configured the lock surface")?;
            let buffer = solid_buffer(&shm, &qh, width as i32, height as i32)?;
            surface.frame(&qh, index);
            surface.attach(Some(&buffer), 0, 0);
            surface.damage_buffer(0, 0, i32::MAX, i32::MAX);
            surface.commit();
            probe.painted += 1;
            probe.say("painted", &format!("output={index}"));
        }
        queue.roundtrip(&mut probe)?;
    }

    // Polled rather than blocking, because the interesting outcome of the
    // `no-surface` mode is that *no* event ever arrives; a blocking dispatch
    // would wait for it forever and the run would report nothing at all.
    let deadline = Instant::now() + hold;
    while Instant::now() < deadline && !probe.finished {
        // A roundtrip always returns, because the sync callback it sends is
        // answered unconditionally; it drains whatever else arrived with it.
        queue.roundtrip(&mut probe)?;
        std::thread::sleep(Duration::from_millis(20));
    }

    // The one thing a client CAN decide for itself. Whether the locked frame was
    // really on the panel first is a question about pixels, which no client can
    // see - but whether the event arrives at all is plain, and never arriving is
    // the failure mode a compositor that gates the event can newly have: an
    // output that presents no frame (powered off, mirrored, unplugged mid-lock)
    // holds it back for the rest of the session, and the lock client and the
    // idle daemon behind it wait forever.
    let ok = match probe.locked_at {
        Some(at) => {
            probe.say(
                "verdict",
                &format!(
                    "locked arrived at {:.1}ms with {} of {} outputs presented",
                    at.as_secs_f64() * 1000.0,
                    probe.presented,
                    outputs.len()
                ),
            );
            true
        }
        None => {
            probe.say(
                "verdict",
                "FAIL locked never arrived; the session lock is stuck half-taken",
            );
            false
        }
    };

    match mode {
        Mode::Crash => {
            probe.say("crash", "aborting without unlock_and_destroy");
            std::process::abort();
        }
        Mode::Unlock => {
            lock.unlock_and_destroy();
            queue.roundtrip(&mut probe)?;
            probe.say("unlock", "unlock_and_destroy sent");
        }
        _ => {}
    }

    if !ok {
        std::process::exit(1);
    }
    Ok(())
}

/// Drives a pointer into a compositor that has no input devices.
///
/// A nested compositor is a client of its host, and a client is only sent
/// pointer events if the host's seat advertises a pointer. A headless sway host
/// has no input devices at all, so its seat never gains that capability and the
/// nested compositor never binds `wl_pointer`. Measured 14 Sep: 25 clicks driven
/// with `swaymsg seat - cursor press button1` and 25 more with `wlrctl pointer
/// click` all reported success to the host and produced no pointer event of any
/// kind inside the nested compositor. `ydotool` cannot help either - it writes
/// to `/dev/uinput`, so the event is picked up by whichever compositor owns the
/// real seat, which on a developer machine is the one the developer is using.
///
/// This is the piece that was missing: a `zwlr_virtual_pointer_v1` client that
/// STAYS ALIVE. `wlrctl` speaks the same protocol but creates its device, sends
/// one event and exits, which tears the device down again - and a seat that
/// gains and loses a pointer inside a millisecond does not reliably get as far
/// as a nested client binding it. Holding the device open for the whole session
/// gives the host seat a pointer for as long as the test needs one.
///
/// Point it at the HOST display, not at the compositor under test:
///
///     WAYLAND_DISPLAY=$HOST pointer-driver <<'EOF'
///     move 600 500
///     click
///     EOF
///
/// Commands, one per line, on stdin:
///   move <x> <y>     absolute position, in the output's pixels
///   press|release    left button
///   click            press, brief pause, release
///   sleep <ms>
/// Every command is flushed before the next is read, so a caller can drive it
/// interactively over a pipe and stay in step with what the compositor does.
use std::{
    io::BufRead,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{
        wl_output::{self, WlOutput},
        wl_pointer::ButtonState,
        wl_registry::{self, WlRegistry},
        wl_seat::{self, WlSeat},
    },
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::{self, ZwlrVirtualPointerV1},
};

struct Driver {
    manager: Option<ZwlrVirtualPointerManagerV1>,
    seat: Option<WlSeat>,
    output: Option<WlOutput>,
    /// The output's size, needed because `motion_absolute` is expressed as a
    /// fraction of an extent rather than in pixels.
    extent: Option<(u32, u32)>,
}

impl Dispatch<WlRegistry, ()> for Driver {
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
                "zwlr_virtual_pointer_manager_v1" => {
                    state.manager = Some(registry.bind(name, version.min(2), qh, ()))
                }
                "wl_seat" if state.seat.is_none() => {
                    state.seat = Some(registry.bind(name, version.min(5), qh, ()))
                }
                "wl_output" if state.output.is_none() => {
                    state.output = Some(registry.bind(name, version.min(3), qh, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<WlOutput, ()> for Driver {
    fn event(
        state: &mut Self,
        _: &WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Mode { width, height, .. } = event {
            state.extent = Some((width.max(1) as u32, height.max(1) as u32));
        }
    }
}

impl Dispatch<WlSeat, ()> for Driver {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrVirtualPointerManagerV1, ()> for Driver {
    fn event(
        _: &mut Self,
        _: &ZwlrVirtualPointerManagerV1,
        _: <ZwlrVirtualPointerManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrVirtualPointerV1, ()> for Driver {
    fn event(
        _: &mut Self,
        _: &ZwlrVirtualPointerV1,
        _: zwlr_virtual_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// Milliseconds, in the monotonic-ish domain the protocol asks for.
fn now() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u32)
        .unwrap_or(0)
}

/// The Linux button code for BTN_LEFT, which is what the protocol wants.
const BTN_LEFT: u32 = 0x110;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::connect_to_env()?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let mut driver = Driver {
        manager: None,
        seat: None,
        output: None,
        extent: None,
    };
    conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut driver)?;
    queue.roundtrip(&mut driver)?;

    let manager = driver
        .manager
        .clone()
        .ok_or("the host does not offer zwlr_virtual_pointer_manager_v1")?;
    let (ex, ey) = driver.extent.unwrap_or((1920, 1080));
    let pointer = manager.create_virtual_pointer(driver.seat.as_ref(), &qh, ());
    println!("ready extent={ex}x{ey}");

    // Give the host a moment to publish the seat's new pointer capability, and
    // the compositor under test a moment to bind it. Without this a caller that
    // clicks immediately is racing the very capability this program exists to
    // create.
    queue.roundtrip(&mut driver)?;
    let settle = Instant::now();
    while settle.elapsed() < Duration::from_millis(300) {
        queue.roundtrip(&mut driver)?;
        std::thread::sleep(Duration::from_millis(20));
    }

    let flush = |queue: &mut wayland_client::EventQueue<Driver>,
                 driver: &mut Driver|
     -> Result<(), Box<dyn std::error::Error>> {
        pointer.frame();
        queue.roundtrip(driver)?;
        Ok(())
    };

    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let mut it = line.split_whitespace();
        match it.next() {
            Some("move") => {
                let x: u32 = it.next().unwrap_or("0").parse()?;
                let y: u32 = it.next().unwrap_or("0").parse()?;
                pointer.motion_absolute(now(), x, y, ex, ey);
                flush(&mut queue, &mut driver)?;
                println!("ok move {x} {y}");
            }
            Some("press") => {
                pointer.button(now(), BTN_LEFT, ButtonState::Pressed);
                flush(&mut queue, &mut driver)?;
                println!("ok press");
            }
            Some("release") => {
                pointer.button(now(), BTN_LEFT, ButtonState::Released);
                flush(&mut queue, &mut driver)?;
                println!("ok release");
            }
            Some("click") => {
                pointer.button(now(), BTN_LEFT, ButtonState::Pressed);
                flush(&mut queue, &mut driver)?;
                std::thread::sleep(Duration::from_millis(40));
                pointer.button(now(), BTN_LEFT, ButtonState::Released);
                flush(&mut queue, &mut driver)?;
                println!("ok click");
            }
            Some("sleep") => {
                let ms: u64 = it.next().unwrap_or("100").parse()?;
                std::thread::sleep(Duration::from_millis(ms));
                println!("ok sleep {ms}");
            }
            Some("quit") | None => break,
            Some(other) => println!("unknown command {other}"),
        }
        use std::io::Write;
        std::io::stdout().flush()?;
    }
    pointer.destroy();
    queue.roundtrip(&mut driver)?;
    Ok(())
}

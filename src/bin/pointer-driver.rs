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
///   chord <code> ...  press evdev keycodes in order, release them in reverse,
///                     e.g. `chord 125 56 1` for Super+Alt+Escape
///
/// The keys go through a `zwp_virtual_keyboard_v1` on the host with a plain US
/// keymap (`xkbcli compile-keymap`). `wtype` cannot do this job: it uploads a
/// keymap of its own with keycodes numbered by the characters it types, and a
/// nested compositor reads keycodes with ITS keymap, so "HOSTKEYS" arrived as
/// "34562". Evdev codes for Super, Alt and Escape mean the same in every layout.
/// Typed at the nested compositor instead, `wtype` reaches clients but never the
/// compositor's own shortcuts, because a virtual keyboard there is forwarded
/// straight to the focused client.
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
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::{self, ZwlrVirtualPointerV1},
};

struct Driver {
    manager: Option<ZwlrVirtualPointerManagerV1>,
    keyboards: Option<ZwpVirtualKeyboardManagerV1>,
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
                "zwp_virtual_keyboard_manager_v1" => {
                    state.keyboards = Some(registry.bind(name, 1, qh, ()))
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

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for Driver {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardManagerV1,
        _: <ZwpVirtualKeyboardManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for Driver {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardV1,
        _: <ZwpVirtualKeyboardV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// A keyboard on the host seat with a plain US keymap, made on first use.
fn host_keyboard(
    driver: &Driver,
    qh: &QueueHandle<Driver>,
) -> Result<ZwpVirtualKeyboardV1, Box<dyn std::error::Error>> {
    let manager = driver
        .keyboards
        .as_ref()
        .ok_or("the host offers no zwp_virtual_keyboard_manager_v1")?;
    let seat = driver.seat.as_ref().ok_or("the host has no seat")?;
    let keymap = std::process::Command::new("xkbcli")
        .args(["compile-keymap", "--layout", "us"])
        .output()?
        .stdout;
    let mut file = tempfile::tempfile()?;
    std::io::Write::write_all(&mut file, &keymap)?;
    std::io::Write::write_all(&mut file, &[0])?;
    let keyboard = manager.create_virtual_keyboard(seat, qh, ());
    // 1 is WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1; the size counts the trailing NUL.
    keyboard.keymap(
        1,
        std::os::unix::io::AsFd::as_fd(&file),
        keymap.len() as u32 + 1,
    );
    Ok(keyboard)
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
        keyboards: None,
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

    let mut keyboard: Option<ZwpVirtualKeyboardV1> = None;

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
            Some("chord") => {
                let codes: Vec<u32> = it.map(str::parse).collect::<Result<_, _>>()?;
                if keyboard.is_none() {
                    keyboard = Some(host_keyboard(&driver, &qh)?);
                    // As with the pointer: let the host publish the capability
                    // and the nested compositor take the keymap.
                    for _ in 0..15 {
                        queue.roundtrip(&mut driver)?;
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
                let kb = keyboard.as_ref().expect("made above");
                for code in &codes {
                    kb.key(now(), *code, 1);
                    queue.roundtrip(&mut driver)?;
                    std::thread::sleep(Duration::from_millis(30));
                }
                for code in codes.iter().rev() {
                    kb.key(now(), *code, 0);
                    queue.roundtrip(&mut driver)?;
                    std::thread::sleep(Duration::from_millis(30));
                }
                println!("ok chord {codes:?}");
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

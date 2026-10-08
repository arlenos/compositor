/// Listens on `arlen_shell_overlay_v1` the way desktop-shell does, and prints
/// the events a harness wants to assert on.
///
/// The overlay protocol is how the compositor tells the shell about things the
/// shell then draws - so a compositor-side change to it cannot be checked by
/// looking at the screen without the shell running. This client is the shell's
/// ear without the shell: it binds the global at the version it was built for
/// and prints one line per event it was asked to watch.
///
/// Usage: WAYLAND_DISPLAY=wayland-N overlay-probe [seconds] [version]
/// Prints `bound version=N`, then e.g. `workspace_move_refused output=WINIT-0 target=1`
/// or `context_menu items=12 separators=3`, or
/// `capture_started id=1 app=grim source=output` and `capture_stopped id=1`.
use std::time::{Duration, Instant};

use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::wl_registry::{self, WlRegistry},
};

#[allow(non_upper_case_globals, non_camel_case_types, dead_code, clippy::all)]
mod protocol {
    use wayland_client;

    pub mod __interfaces {
        wayland_scanner::generate_interfaces!("resources/protocols/arlen-shell-overlay.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("resources/protocols/arlen-shell-overlay.xml");
}

use protocol::arlen_shell_overlay_v1::{self, ArlenShellOverlayV1};

struct Probe {
    want_version: u32,
    overlay: Option<ArlenShellOverlayV1>,
    /// Items and separators of the context menu being received.
    menu: (u32, u32),
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
            && interface == "arlen_shell_overlay_v1"
        {
            let v = version.min(state.want_version);
            state.overlay = Some(registry.bind(name, v, qh, ()));
            println!("bound version={v}");
        }
    }
}

impl Dispatch<ArlenShellOverlayV1, ()> for Probe {
    fn event(
        state: &mut Self,
        _: &ArlenShellOverlayV1,
        event: arlen_shell_overlay_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            arlen_shell_overlay_v1::Event::WorkspaceMoveRefused { output, target } => {
                println!("workspace_move_refused output={output} target={target}");
            }
            // A context menu arrives as begin, one event per entry, done. Counting
            // entries against separators is what shows an entry that went out as a
            // separator - "Move" did, until 14 September.
            arlen_shell_overlay_v1::Event::ContextMenuBegin { .. } => state.menu = (0, 0),
            arlen_shell_overlay_v1::Event::ContextMenuItem { .. } => state.menu.0 += 1,
            arlen_shell_overlay_v1::Event::ContextMenuSeparator { .. } => state.menu.1 += 1,
            arlen_shell_overlay_v1::Event::CaptureStarted {
                capture_id,
                app_id,
                source,
            } => {
                let source = match source {
                    wayland_client::WEnum::Value(s) => format!("{s:?}").to_lowercase(),
                    wayland_client::WEnum::Unknown(n) => format!("unknown-{n}"),
                };
                println!("capture_started id={capture_id} app={app_id} source={source}");
            }
            arlen_shell_overlay_v1::Event::CaptureStopped { capture_id } => {
                println!("capture_stopped id={capture_id}");
            }
            arlen_shell_overlay_v1::Event::ContextMenuDone { .. } => {
                println!(
                    "context_menu items={} separators={}",
                    state.menu.0, state.menu.1
                );
            }
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let seconds: u64 = args.next().as_deref().unwrap_or("10").parse()?;
    let want_version: u32 = args.next().as_deref().unwrap_or("3").parse()?;

    let conn = Connection::connect_to_env()?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let mut probe = Probe {
        want_version,
        overlay: None,
        menu: (0, 0),
    };
    conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut probe)?;
    if probe.overlay.is_none() {
        return Err("the compositor offers no arlen_shell_overlay_v1".into());
    }

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(seconds) {
        queue.roundtrip(&mut probe)?;
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("still connected");
    Ok(())
}

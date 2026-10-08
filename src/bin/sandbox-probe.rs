/// Which globals a client sees with and without a security context.
///
/// The compositor gates capture and the shell's own protocols on
/// `wp_security_context_v1`: a connection that carries a context from any sandbox
/// engine but desktop-shell's is treated as sandboxed and does not see them. A
/// connection without one sees everything. So whether an app can capture the
/// screen without the portal depends entirely on how its launcher connected it.
///
/// This connects twice and prints what each connection is offered:
///
///   `plain <global>`      a direct connection to `$WAYLAND_DISPLAY`, which is what
///                         an app gets when the raw socket is bound into its sandbox
///   `sandboxed <global>`  a connection through a security context with the given
///                         engine, which is what a launcher should hand an app
///
/// Only the globals that matter are printed: capture, its sources, the shell's
/// protocols and the security context manager itself.
///
/// Usage: WAYLAND_DISPLAY=wayland-N sandbox-probe [sandbox-engine]
use std::{
    os::unix::{
        io::AsFd,
        net::{UnixListener, UnixStream},
    },
    time::Duration,
};

use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::wl_registry::{self, WlRegistry},
};
use wayland_protocols::wp::security_context::v1::client::{
    wp_security_context_manager_v1::WpSecurityContextManagerV1,
    wp_security_context_v1::WpSecurityContextV1,
};

/// The globals this probe reports. Everything else a client is offered is
/// ordinary and the same either way.
const WATCHED: &[&str] = &[
    "ext_image_copy_capture_manager_v1",
    "ext_output_image_capture_source_manager_v1",
    "ext_foreign_toplevel_image_capture_source_manager_v1",
    "zcosmic_workspace_image_capture_source_manager_v1",
    "arlen_shell_overlay_v1",
    "zcosmic_overlap_notify_v1",
    "zwlr_output_power_manager_v1",
    "wp_security_context_manager_v1",
];

#[derive(Default)]
struct Globals {
    names: Vec<String>,
    manager: Option<WpSecurityContextManagerV1>,
}

impl Dispatch<WlRegistry, ()> for Globals {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            if interface == "wp_security_context_manager_v1" {
                state.manager = Some(registry.bind(name, 1, qh, ()));
            }
            state.names.push(interface);
        }
    }
}

impl Dispatch<WpSecurityContextManagerV1, ()> for Globals {
    fn event(
        _: &mut Self,
        _: &WpSecurityContextManagerV1,
        _: <WpSecurityContextManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpSecurityContextV1, ()> for Globals {
    fn event(
        _: &mut Self,
        _: &WpSecurityContextV1,
        _: <WpSecurityContextV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

fn globals_of(conn: &Connection) -> Result<Globals, Box<dyn std::error::Error>> {
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut globals = Globals::default();
    queue.roundtrip(&mut globals)?;
    queue.roundtrip(&mut globals)?;
    Ok(globals)
}

fn report(label: &str, globals: &Globals) {
    for watched in WATCHED {
        if globals.names.iter().any(|n| n == watched) {
            println!("{label} {watched}");
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "dev.arlen.run".to_string());

    let plain = Connection::connect_to_env()?;
    let mut queue = plain.new_event_queue();
    let qh = queue.handle();
    plain.display().get_registry(&qh, ());
    let mut globals = Globals::default();
    queue.roundtrip(&mut globals)?;
    report("plain", &globals);

    let Some(manager) = globals.manager.clone() else {
        println!("verdict no security context manager offered; nothing can be told apart");
        return Ok(());
    };

    // The listening socket the compositor will accept the sandboxed connection on,
    // and a pipe whose other end closing tells it to stop. The write end stays
    // open for as long as this probe runs.
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("sandboxed");
    let listener = UnixListener::bind(&path)?;
    let (close_read, _close_write) = std::io::pipe()?;
    let context = manager.create_listener(listener.as_fd(), close_read.as_fd(), &qh, ());
    context.set_sandbox_engine(engine.clone());
    context.set_app_id("dev.arlen.sandbox-probe".to_string());
    context.set_instance_id("1".to_string());
    context.commit();
    queue.roundtrip(&mut globals)?;

    let stream = UnixStream::connect(&path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let sandboxed = Connection::from_socket(stream)?;
    let inside = globals_of(&sandboxed)?;
    report("sandboxed", &inside);
    println!("engine {engine}");

    let capture = "ext_image_copy_capture_manager_v1";
    let plain_can = globals.names.iter().any(|n| n == capture);
    let inside_can = inside.names.iter().any(|n| n == capture);
    println!(
        "verdict plain capture={} sandboxed capture={}",
        if plain_can { "offered" } else { "withheld" },
        if inside_can { "offered" } else { "withheld" },
    );
    Ok(())
}

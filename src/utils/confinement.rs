// SPDX-License-Identifier: GPL-3.0-only

//! Which Arlen app, if any, a client is confined as.
//!
//! The compositor tells sandboxed clients apart by `wp_security_context_v1`, and
//! flatpak connects its apps through one. Arlen's own launcher, `arlen-run`, does
//! not: it binds the compositor's socket into the app's namespace, so a confined
//! Arlen app connects with no context and is offered everything an unconfined
//! process is, the capture protocols included. Measured 8 October with
//! `sandbox-probe`: a plain connection is offered `ext_image_copy_capture_manager_v1`,
//! the same connection through a `dev.arlen.run` context is not.
//!
//! What `arlen-run` does do is put every launch in its own cgroup leaf,
//! `app-arlen-<app_id>-<pid>.scope`, parallel to flatpak's `app-flatpak-…`. So the
//! connecting process's cgroup says which app it is, and that is read here once,
//! when the client connects. The cgroup is set by the launcher before the app runs
//! and the app cannot leave it without the write access to its parent that the
//! user's delegated slice does not give it.

use std::{os::unix::net::UnixStream, path::Path};

/// Apps confined by `arlen-run` that may still capture the screen directly,
/// because capturing is what they are for. Everything else goes through the
/// portal, which asks the user.
pub const MAY_CAPTURE: &[&str] = &["dev.arlen.screenshot"];

/// The process on the other end of `stream`.
pub fn peer_pid(stream: &UnixStream) -> Option<u32> {
    let cred = rustix::net::sockopt::socket_peercred(stream).ok()?;
    u32::try_from(cred.pid.as_raw_nonzero().get()).ok()
}

/// The app id of the Arlen app the peer of `stream` is confined as, or `None` for
/// a client `arlen-run` did not launch.
pub fn arlen_app_of(stream: &UnixStream) -> Option<String> {
    let pid = peer_pid(stream)?;
    let cgroup = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    arlen_app_in(&cgroup)
}

/// The app id named by the `app-arlen-…` leaf in a `/proc/<pid>/cgroup` text.
///
/// The inverse of `arlen-run`'s `scope_name`: strip `app-arlen-` and `.scope`,
/// then split off the trailing `-<pid>`. App ids contain dots and may contain
/// dashes, so the split is at the LAST dash.
pub fn arlen_app_in(cgroup: &str) -> Option<String> {
    cgroup.lines().find_map(|line| {
        // cgroup v2: "0::/user.slice/.../app-arlen-<id>-<pid>.scope"
        let path = line.rsplit_once(':')?.1;
        let leaf = Path::new(path).file_name()?.to_str()?;
        let inner = leaf.strip_suffix(".scope")?.strip_prefix("app-arlen-")?;
        let (app_id, pid) = inner.rsplit_once('-')?;
        (!app_id.is_empty() && pid.parse::<u32>().is_ok()).then(|| app_id.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_app_from_an_arlen_scope() {
        let cgroup = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/\
                      app-arlen-dev.arlen.files-4321.scope\n";
        assert_eq!(arlen_app_in(cgroup).as_deref(), Some("dev.arlen.files"));
    }

    #[test]
    fn keeps_dashes_inside_the_app_id() {
        let cgroup = "0::/user.slice/app.slice/app-arlen-com.example.my-app-77.scope\n";
        assert_eq!(arlen_app_in(cgroup).as_deref(), Some("com.example.my-app"));
    }

    #[test]
    fn ignores_everything_else() {
        for cgroup in [
            "0::/user.slice/user-1000.slice/session-2.scope\n",
            "0::/user.slice/app.slice/app-flatpak-org.gnome.Calculator-1.scope\n",
            "0::/user.slice/app.slice/app-arlen-.scope\n",
            "0::/user.slice/app.slice/app-arlen-dev.arlen.files-notapid.scope\n",
            "",
        ] {
            assert_eq!(arlen_app_in(cgroup), None, "{cgroup:?}");
        }
    }
}

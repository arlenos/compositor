// SPDX-License-Identifier: GPL-3.0-only

//! Who opened each capture session, told to the shell for its capture indicator.
//!
//! The portal and PipeWire only see captures that go through the portal. The
//! compositor sees every one, so this is where an indicator can be complete:
//! each session that opens is announced with `capture_started` on the shell
//! overlay, each that closes with `capture_stopped`.
//!
//! Smithay hands `new_session` the session and nothing about the client that
//! asked for it. So the `ext_image_copy_capture_manager_v1` global the clients
//! see is this module's: its requests note the client, then go to smithay's own
//! handling unchanged, which calls `new_session` synchronously while the note is
//! still there. Smithay's global still exists, because its state keeps the
//! session list, but nobody can see it.

use std::{
    cell::RefCell,
    sync::atomic::{AtomicU32, Ordering},
};

use smithay::{
    reexports::{
        wayland_protocols::ext::image_copy_capture::v1::server::ext_image_copy_capture_manager_v1::{
            self, ExtImageCopyCaptureManagerV1,
        },
        wayland_server::{Client, DataInit, DisplayHandle, New, backend::GlobalId},
    },
    wayland::{
        Dispatch2, GlobalData, GlobalDispatch2,
        image_copy_capture::{Session, SessionRef},

    },
};

use smithay::xwayland::XWaylandClientData;

use crate::state::{ClientState, Common, State};
pub use crate::wayland::protocols::shell_overlay::CaptureSource;

thread_local! {
    /// The client whose `create_session` is being handled right now.
    static REQUESTER: RefCell<Option<Client>> = const { RefCell::new(None) };
}

static NEXT_CAPTURE_ID: AtomicU32 = AtomicU32::new(1);

/// Stored on a session that was announced, so its end can be.
struct CaptureTag(u32);

/// The global clients bind instead of smithay's.
pub struct CaptureManagerGlobal {
    filter: Box<dyn Fn(&Client) -> bool + Send + Sync>,
}

/// What each bound manager carries: nothing but the route to this module.
pub struct CaptureManagerData;

/// Create the visible `ext_image_copy_capture_manager_v1` global, version 1 like
/// smithay's, offered to the clients `filter` lets see it.
pub fn create_global(
    dh: &DisplayHandle,
    filter: impl Fn(&Client) -> bool + Send + Sync + 'static,
) -> GlobalId {
    dh.create_global::<State, ExtImageCopyCaptureManagerV1, _>(
        1,
        CaptureManagerGlobal {
            filter: Box::new(filter),
        },
    )
}

impl GlobalDispatch2<ExtImageCopyCaptureManagerV1, State> for CaptureManagerGlobal {
    fn bind(
        &self,
        _state: &mut State,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ExtImageCopyCaptureManagerV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        data_init.init(resource, CaptureManagerData);
    }

    fn can_view(&self, client: &Client) -> bool {
        (self.filter)(client)
    }
}

impl Dispatch2<ExtImageCopyCaptureManagerV1, State> for CaptureManagerData {
    fn request(
        &self,
        state: &mut State,
        client: &Client,
        resource: &ExtImageCopyCaptureManagerV1,
        request: ext_image_copy_capture_manager_v1::Request,
        dh: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        REQUESTER.with(|r| *r.borrow_mut() = Some(client.clone()));
        <GlobalData as Dispatch2<ExtImageCopyCaptureManagerV1, State>>::request(
            &GlobalData,
            state,
            client,
            resource,
            request,
            dh,
            data_init,
        );
        REQUESTER.with(|r| r.borrow_mut().take());
    }
}

/// Announce a session that has just been accepted for `source`.
pub fn started(common: &Common, session: &Session, source: CaptureSource) {
    let Some(client) = REQUESTER.with(|r| r.borrow().clone()) else {
        return;
    };
    let id = NEXT_CAPTURE_ID.fetch_add(1, Ordering::Relaxed);
    session
        .user_data()
        .insert_if_missing_threadsafe(|| CaptureTag(id));
    common
        .shell_overlay_state
        .send_capture_started(id, &app_id_of(&client), source);
}

/// Announce the end of a session that `started` announced.
pub fn stopped(common: &Common, session: &SessionRef) {
    if let Some(CaptureTag(id)) = session.user_data().get::<CaptureTag>() {
        common.shell_overlay_state.send_capture_stopped(*id);
    }
}

/// The name a capture is shown under: the security context's app id, else the
/// Arlen app the launcher confined the client as, else its executable.
fn app_id_of(client: &Client) -> String {
    if client.get_data::<XWaylandClientData>().is_some() {
        return "xwayland".to_string();
    }
    let Some(state) = client.get_data::<ClientState>() else {
        return "unknown".to_string();
    };
    if let Some(app_id) = state
        .security_context
        .as_ref()
        .and_then(|context| context.app_id.clone())
    {
        return app_id;
    }
    if let Some(app) = &state.arlen_app {
        return app.clone();
    }
    state
        .pid
        .and_then(|pid| std::fs::read_to_string(format!("/proc/{pid}/comm")).ok())
        .map(|comm| comm.trim().to_string())
        .filter(|comm| !comm.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

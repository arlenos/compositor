// SPDX-License-Identifier: GPL-3.0-only

use crate::{shell::SessionLock, state::State, utils::prelude::*};
use smithay::{
    output::Output,
    reexports::wayland_server::{Resource, protocol::wl_output::WlOutput},
    utils::Size,
    wayland::session_lock::{
        LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker,
    },
};
use std::collections::HashMap;

impl SessionLockHandler for State {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.common.session_lock_manager_state
    }

    fn lock(&mut self, locker: SessionLocker) {
        let mut shell = self.common.shell.write();

        // Reject lock if sesion lock exists and is still valid
        if let Some(session_lock) = shell.session_lock.as_ref()
            && self
                .common
                .display_handle
                .get_client(session_lock.ext_session_lock.id())
                .is_ok()
        {
            return;
        }

        let ext_session_lock = locker.ext_session_lock().clone();

        // Every output has to show a locked frame before the client is told the
        // session is locked. A mirrored output is left out because it never gets
        // frame callbacks of its own (the KMS surface thread skips them), so
        // waiting for one would hold `locked` back forever; it shows whatever its
        // source output shows, which is the locked frame.
        let unpresented = shell
            .outputs()
            .filter(|output| output.mirroring().is_none())
            .cloned()
            .collect();
        let mut session_lock = SessionLock {
            ext_session_lock,
            surfaces: HashMap::new(),
            locker: Some(locker),
            unpresented,
        };

        // With no output to wait for there is nothing that could still be
        // showing unlocked content, so the event is already due.
        if session_lock.unpresented.is_empty()
            && let Some(locker) = session_lock.locker.take()
        {
            locker.lock();
        }
        shell.session_lock = Some(session_lock);

        for output in shell.outputs() {
            self.backend.schedule_render(output);
        }
    }

    fn unlock(&mut self) {
        let mut shell = self.common.shell.write();
        shell.session_lock = None;

        let seats = shell.seats.iter().cloned().collect::<Vec<_>>();
        for seat in &seats {
            self.common.idle_notifier_state.notify_activity(seat);
        }

        for output in shell.outputs() {
            self.backend.schedule_render(output);
        }
    }

    fn new_surface(&mut self, lock_surface: LockSurface, wl_output: WlOutput) {
        let mut shell = self.common.shell.write();
        if let Some(session_lock) = &mut shell.session_lock
            && let Some(output) = Output::from_resource(&wl_output)
        {
            lock_surface.with_pending_state(|states| {
                let size = output.geometry().size;
                states.size = Some(Size::from((size.w as u32, size.h as u32)));
            });
            lock_surface.send_configure();
            session_lock
                .surfaces
                .insert(output.clone(), lock_surface.clone());
        }
    }
}

// SPDX-License-Identifier: GPL-3.0-only

//! Which zoom-toolbar event to send desktop-shell for a change in zoom.
//! Fork-only; moved out of upstream's `shell/mod.rs` unchanged on 8 October.

use super::*;

/// Which zoom event a level change should send.
///
/// `show` and `update` are not interchangeable, and the protocol this fork
/// publishes says so: `zoom_toolbar_show` is "sent when accessibility zoom is
/// activated", `zoom_toolbar_update` "when the zoom level changes while the
/// toolbar is visible". Sending `show` for both - which the compositor did
/// until 8 September - re-announces the toolbar on every step of a zoom, so a
/// shell that treats `show` as "appear" replays its entrance on each notch of
/// the scroll wheel.
///
/// `show` also carries the increment and movement mode, which `update` does
/// not, so a settings change while zoom is open goes out as a `show` even
/// though the toolbar is already visible.
pub(crate) fn zoom_event_for(
    previous: Option<(u32, ZoomMovement)>,
    level: f64,
    increment: u32,
    movement: ZoomMovement,
    movement_wire: u32,
) -> ZoomProtocolEvent {
    match previous {
        Some((prev_increment, prev_movement))
            if prev_increment == increment && prev_movement == movement =>
        {
            ZoomProtocolEvent::Update { level }
        }
        _ => ZoomProtocolEvent::Show {
            level,
            increment,
            movement: movement_wire,
        },
    }
}

/// Protocol events queued by Shell, drained by Common::refresh().
#[derive(Debug, PartialEq)]
pub(crate) enum ZoomProtocolEvent {
    Show {
        level: f64,
        increment: u32,
        movement: u32,
    },
    /// A level change while the toolbar is already up. Carries only the level:
    /// increment and movement mode travel on `Show`, so a config change while
    /// zoom is open goes out as a `Show` instead.
    Update {
        level: f64,
    },
    Hide,
}

#[cfg(test)]
mod zoom_event_tests {
    use super::*;

    const CONTINUOUS_WIRE: u32 = 1;

    #[test]
    fn activation_sends_show() {
        // Nothing was visible, so this is an appearance and the shell needs
        // the increment and movement mode as well as the level.
        let event = zoom_event_for(None, 1.25, 25, ZoomMovement::Continuously, CONTINUOUS_WIRE);
        assert_eq!(
            event,
            ZoomProtocolEvent::Show {
                level: 1.25,
                increment: 25,
                movement: CONTINUOUS_WIRE,
            }
        );
    }

    #[test]
    fn a_further_step_sends_update() {
        // The toolbar is already up and only the level moved. This is the case
        // that used to send `show` and made a shell replay its entrance on
        // every notch of the scroll wheel.
        let event = zoom_event_for(
            Some((25, ZoomMovement::Continuously)),
            1.5,
            25,
            ZoomMovement::Continuously,
            CONTINUOUS_WIRE,
        );
        assert_eq!(event, ZoomProtocolEvent::Update { level: 1.5 });
    }

    #[test]
    fn a_settings_change_while_open_sends_show() {
        // `update` carries only the level, so a changed increment or movement
        // mode has to travel on a `show` even though the toolbar is visible -
        // otherwise the shell keeps displaying the old step size.
        let changed_increment = zoom_event_for(
            Some((25, ZoomMovement::Continuously)),
            1.5,
            10,
            ZoomMovement::Continuously,
            CONTINUOUS_WIRE,
        );
        assert!(matches!(changed_increment, ZoomProtocolEvent::Show { .. }));

        let changed_movement = zoom_event_for(
            Some((25, ZoomMovement::Continuously)),
            1.5,
            25,
            ZoomMovement::Centered,
            3,
        );
        assert!(matches!(changed_movement, ZoomProtocolEvent::Show { .. }));
    }
}

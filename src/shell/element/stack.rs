use super::window::{Focus, RESIZE_BORDER};
use crate::{
    backend::render::{
        cursor::CursorState, element::AsGlowRenderer, shadow::ShadowShader,
        wayland::SurfaceRenderElement,
    },
    shell::{
        ModalBehavior,
        element::{
            CosmicMappedKey, CosmicMappedKeyInner,
            surface::{CosmicSurface, WeakCosmicSurface},
        },
        focus::target::PointerFocusTarget,
        grabs::{GrabType, ReleaseMode, ResizeEdge},
        layout::tiling::NodeDesc,
    },
    state::State,
    theme::corner,
    utils::prelude::*,
};
use calloop::LoopHandle;
<<<<<<< HEAD
=======
use cosmic::{
    Apply, Element as CosmicElement, Theme,
    iced::{
        Alignment,
        core::{Background, Border, Color, Length, border::Radius},
        id::Id,
        runtime::Task,
        widget::scrollable::AbsoluteOffset,
    },
    theme, widget,
};
>>>>>>> upstream/master
use cosmic_comp_config::AppearanceConfig;
use cosmic_settings_config::shortcuts;
use shortcuts::action::{Direction, FocusDirection};
use smithay::{
    backend::{
        drm::DrmNode,
        input::{InputTime, KeyState, TabletToolDescriptor},
        renderer::{
            ImportAll, ImportMem, Renderer,
            element::{
                Element, Id as RendererId, Kind, RenderElement, UnderlyingStorage,
                memory::MemoryRenderBufferRenderElement,
            },
            gles::element::PixelShaderElement,
            glow::GlowRenderer,
            utils::{CommitCounter, DamageSet, OpaqueRegions},
        },
    },
    desktop::{WindowSurfaceType, space::SpaceElement},
    input::{
        Seat,
        keyboard::{KeyboardTarget, KeysymHandle, ModifiersState},
        pointer::{
            AxisFrame as PointerAxisFrame, ButtonEvent as PointerButtonEvent, CursorImageStatus,
            GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
            GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
            GestureSwipeEndEvent, GestureSwipeUpdateEvent, MotionEvent as PointerMotionEvent,
            PointerTarget, RelativeMotionEvent,
        },
        tablet::{
            Tablet, TabletSeatTrait,
            tool::{
                AxisFrame as ToolAxisFrame, ButtonEvent as ToolButtonEvent,
                DownEvent as ToolDownEvent, MotionEvent as ToolMotionEvent, TabletToolTarget,
                UpEvent as ToolUpEvent,
            },
        },
        touch::{
            DownEvent as TouchDownEvent, FrameMarker, MotionEvent as TouchMotionEvent,
            OrientationEvent, ShapeEvent, TouchTarget, UpEvent as TouchUpEvent,
        },
    },
    output::Output,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{
        Buffer, IsAlive, Logical, Physical, Point, Rectangle, Scale, Serial, Size, Transform,
        user_data::UserDataMap,
    },
    wayland::seat::WaylandFocus,
};
use std::{
    borrow::Cow,
    fmt,
    hash::Hash,
<<<<<<< HEAD
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicUsize, Ordering},
    },
=======
    mem,
    sync::{Arc, LazyLock},
>>>>>>> upstream/master
};

static NEXT_STACK_ID: AtomicU32 = AtomicU32::new(1);

/// A stack of windows displayed with a tab bar, managed by the shell overlay protocol.
#[derive(Clone)]
pub struct CosmicStack {
    pub(super) inner: Arc<Mutex<CosmicStackInternal>>,
    handle: LoopHandle<'static, crate::state::State>,
}

// SAFETY: LoopHandle contains an Rc internally, making CosmicStack !Send/!Sync by
// default. All LoopHandle usage is on the main thread (calloop event loop), and the
// interior state is serialised through the Mutex. This mirrors IcedElementInternal.
unsafe impl Send for CosmicStack {}
unsafe impl Sync for CosmicStack {}

impl PartialEq for CosmicStack {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for CosmicStack {}

impl Hash for CosmicStack {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.inner).hash(state);
    }
}

impl fmt::Debug for CosmicStack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CosmicStack")
            .field("inner", &Arc::as_ptr(&self.inner))
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct CosmicStackInternal {
<<<<<<< HEAD
    /// Unique identifier for this stack, used in the shell overlay protocol.
    stack_id: u32,
    windows: Mutex<Vec<CosmicSurface>>,
    active: AtomicUsize,
    activated: AtomicBool,
    group_focused: AtomicBool,
    previous_index: Mutex<Option<(Serial, usize)>>,
    scroll_to_focus: AtomicBool,
    previous_keyboard: AtomicUsize,
    pointer_entered: AtomicU8,
    touch_serial: AtomicU32,
    tablet_serial: AtomicU32,
    reenter: AtomicBool,
    potential_drag: Mutex<Option<usize>>,
    /// Serial of the touch-down that a later touch-motion turns into a drag.
    ///
    /// smithay stopped passing the serial to every `TouchTarget` method, so the
    /// one `start_drag` needs has to be carried from `down` to `motion` by hand.
    override_alive: AtomicBool,
    geometry: Mutex<Option<Rectangle<i32, Global>>>,
    mask: Mutex<Option<tiny_skia::Mask>>,
    tiled: AtomicBool,
    appearance_conf: Mutex<AppearanceConfig>,
=======
    windows: Vec<CosmicSurface>,
    // Index of active window
    active: usize,
    activated: bool,
    group_focused: bool,
    previous_index: Option<(Serial, usize)>,
    scroll_to_focus: bool,
    // Child window with keyboard focus
    previous_keyboard: Option<WeakCosmicSurface>,
    pointer_entered: Option<Focus>,
    touch_serial: u32,
    tablet_serial: u32,
    potential_drag: Option<usize>,
    override_alive: bool,
    geometry: Option<Rectangle<i32, Global>>,
    mask: Option<tiny_skia::Mask>,
    tiled: bool,
    theme: cosmic::Theme,
    appearance_conf: AppearanceConfig,
>>>>>>> upstream/master
}

impl CosmicStackInternal {
    pub fn swap_focus(&mut self, focus: Option<Focus>) -> Option<Focus> {
        mem::replace(&mut self.pointer_entered, focus)
    }

    pub fn current_focus(&self) -> Option<Focus> {
        self.pointer_entered
    }

    fn keyboard_leave_if_previous(&mut self, seat: &Seat<State>, data: &mut State, serial: Serial) {
        if self
            .previous_keyboard
            .as_ref()
            .is_none_or(|previous| self.windows[self.active] != *previous)
        {
            if let Some(window) = self
                .previous_keyboard
                .take()
                .and_then(|prev| self.windows.iter().find(|w| **w == prev))
            {
                KeyboardTarget::leave(window, seat, data, serial);
            }
            KeyboardTarget::enter(
                &self.windows[self.active],
                seat,
                data,
                Vec::new(), /* TODO */
                serial,
            );
            self.previous_keyboard = Some(self.windows[self.active].downgrade());
        }
    }
}

/// Vertical space a CosmicStack reserves at the top of its
/// bounding box for the integrated Arlen window header (tabs +
/// min/max/close buttons, see Feature 3).
///
/// This MUST match `crate::shell::element::window::SSD_HEIGHT`
/// (the same 36-px strip non-stacked SSD windows reserve). Keeping
/// the two values aligned is how the shell can render a stacked
/// Kitty identically to a non-stacked one: same height, same
/// button positions, tabs instead of a plain title.
pub const TAB_HEIGHT: i32 = crate::shell::element::window::SSD_HEIGHT;

#[derive(Debug, Clone)]
pub enum MoveResult {
    Handled,
    MoveOut(CosmicSurface, LoopHandle<'static, crate::state::State>),
    Default,
}

impl CosmicStack {
    /// Helper to lock the internal state.
    fn p(&self) -> MutexGuard<'_, CosmicStackInternal> {
        self.inner.lock().unwrap()
    }

    pub fn new<I: Into<CosmicSurface>>(
        windows: impl Iterator<Item = I>,
        handle: LoopHandle<'static, crate::state::State>,
        appearance: AppearanceConfig,
    ) -> CosmicStack {
        let windows = windows.map(Into::into).collect::<Vec<_>>();
        assert!(!windows.is_empty());

        for window in &windows {
            window.try_force_undecorated(true);
            window.set_tiled(true);
            window.send_configure();
        }

<<<<<<< HEAD
        let stack_id = NEXT_STACK_ID.fetch_add(1, Ordering::Relaxed);
        // Notify shell about the initial tab list.
        let initial_tabs: Vec<_> = windows
            .iter()
            .enumerate()
            .map(|(i, w)| (i as u32, w.title(), w.app_id()))
            .collect();
        handle.insert_idle(move |state| {
            for (index, title, app_id) in initial_tabs {
                state.common.shell_overlay_state.send_tab_added(
                    stack_id,
                    index,
                    title,
                    app_id,
                    index == 0,
                );
            }
        });

        CosmicStack {
            inner: Arc::new(Mutex::new(CosmicStackInternal {
                stack_id,
                windows: Mutex::new(windows),
                active: AtomicUsize::new(0),
                activated: AtomicBool::new(false),
                group_focused: AtomicBool::new(false),
                previous_index: Mutex::new(None),
                touch_serial: AtomicU32::new(0),
                scroll_to_focus: AtomicBool::new(false),
                previous_keyboard: AtomicUsize::new(0),
                pointer_entered: AtomicU8::new(0),
                tablet_serial: AtomicU32::new(0),
                reenter: AtomicBool::new(false),
                potential_drag: Mutex::new(None),
                override_alive: AtomicBool::new(true),
                geometry: Mutex::new(None),
                mask: Mutex::new(None),
                tiled: AtomicBool::new(false),
                appearance_conf: Mutex::new(appearance),
            })),
=======
        let width = windows[0].geometry().size.w;
        CosmicStack(IcedElement::new(
            CosmicStackInternal {
                windows,
                active: 0,
                activated: false,
                group_focused: false,
                previous_index: None,
                scroll_to_focus: false,
                previous_keyboard: None,
                pointer_entered: None,
                touch_serial: 0,
                tablet_serial: 0,
                potential_drag: None,
                override_alive: true,
                geometry: None,
                mask: None,
                tiled: false,
                theme: theme.clone(),
                appearance_conf: appearance,
            },
            (width, TAB_HEIGHT),
>>>>>>> upstream/master
            handle,
        }
    }

    pub fn add_window(
        &self,
        window: impl Into<CosmicSurface>,
        idx: Option<usize>,
        moved_into: Option<&Seat<State>>,
    ) {
        let window = window.into();
        window.try_force_undecorated(true);
        window.set_tiled(true);
        let tab_event = {
            let p = self.p();
            let last_mod_serial = moved_into.and_then(|seat| seat.last_modifier_change());
            if !p
                .previous_index
                .is_some_and(|(serial, _)| Some(serial) == last_mod_serial)
            {
                p.previous_index = last_mod_serial.map(|s| (s, p.active));
            }

            if let Some(geo) = &mut p.geometry {
                geo.loc.y += TAB_HEIGHT;
                geo.size.h -= TAB_HEIGHT;
                window.set_geometry(*geo, TAB_HEIGHT as u32);
            }
            window.send_configure();
<<<<<<< HEAD
            let (final_idx, is_active) = if let Some(idx) = idx {
                p.windows.lock().unwrap().insert(idx, window.clone());
                let old_idx = p.active.swap(idx, Ordering::SeqCst);
                if old_idx == idx {
                    p.reenter.store(true, Ordering::SeqCst);
                    p.previous_keyboard.store(old_idx, Ordering::SeqCst);
                }
                (idx, true)
            } else {
                let mut windows = p.windows.lock().unwrap();
                windows.push(window.clone());
                let new_idx = windows.len() - 1;
                p.active.store(new_idx, Ordering::SeqCst);
                (new_idx, true)
            };
            p.scroll_to_focus.store(true, Ordering::SeqCst);
            (
                p.stack_id,
                final_idx as u32,
                window.title(),
                window.app_id(),
                is_active,
            )
        };
        // Notify shell about the new tab.
        let (stack_id, index, title, app_id, active) = tab_event;
        self.handle.insert_idle(move |state| {
            state
                .common
                .shell_overlay_state
                .send_tab_added(stack_id, index, title, app_id, active);
=======
            if let Some(idx) = idx {
                p.windows.insert(idx, window);
                p.active = idx;
            } else {
                p.windows.push(window);
                p.active = p.windows.len() - 1;
            }
            p.scroll_to_focus = true;
>>>>>>> upstream/master
        });
    }

    pub fn remove_window(&self, window: &CosmicSurface) {
<<<<<<< HEAD
        let tab_event = {
            let p = self.p();
            let mut windows = p.windows.lock().unwrap();
            if windows.len() == 1 {
                p.override_alive.store(false, Ordering::SeqCst);
                let window = windows.first().unwrap();
=======
        self.0.with_program(|p| {
            if p.windows.len() == 1 {
                p.override_alive = false;
                let window = p.windows.first().unwrap();
>>>>>>> upstream/master
                window.try_force_undecorated(false);
                window.set_tiled(false);
                None
            } else {
                let Some(idx) = windows.iter().position(|w| w == window) else {
                    return;
                };
                if idx == p.active.load(Ordering::SeqCst) {
                    p.reenter.store(true, Ordering::SeqCst);
                }
                let window = windows.remove(idx);
                window.try_force_undecorated(false);
                window.set_tiled(false);

<<<<<<< HEAD
                p.active.fetch_min(windows.len() - 1, Ordering::SeqCst);
                p.previous_index
                    .lock()
                    .unwrap()
                    .take_if(|(_, idx)| *idx >= windows.len());
                Some((p.stack_id, idx as u32))
            }
        };
        if let Some((stack_id, index)) = tab_event {
            self.handle.insert_idle(move |state| {
                state
                    .common
                    .shell_overlay_state
                    .send_tab_removed(stack_id, index);
            });
        }
    }

    pub fn remove_idx(&self, idx: usize) -> Option<CosmicSurface> {
        let (window, tab_event) = {
            let p = self.p();
            let mut windows = p.windows.lock().unwrap();
            if windows.len() == 1 {
                p.override_alive.store(false, Ordering::SeqCst);
                let window = windows.first().unwrap();
                window.try_force_undecorated(false);
                window.set_tiled(false);
                (Some(window.clone()), None)
            } else if windows.len() <= idx {
                (None, None)
            } else {
                if idx == p.active.load(Ordering::SeqCst) {
                    p.reenter.store(true, Ordering::SeqCst);
                }
                let window = windows.remove(idx);
                window.try_force_undecorated(false);
                window.set_tiled(false);

                p.active.fetch_min(windows.len() - 1, Ordering::SeqCst);
                p.previous_index
                    .lock()
                    .unwrap()
                    .take_if(|(_, idx)| *idx >= windows.len());
=======
            let Some(idx) = p.windows.iter().position(|w| w == window) else {
                return;
            };
            let window = p.windows.remove(idx);
            // TODO Send `leave` if window matches `p.previous_keyboard`?
            window.try_force_undecorated(false);
            window.set_tiled(false);

            p.active = p.active.min(p.windows.len() - 1);
            p.previous_index.take_if(|(_, idx)| *idx >= p.windows.len());
        });
        self.0
            .resize(Size::from((self.active().geometry().size.w, TAB_HEIGHT)));
        self.0.force_redraw()
    }

    pub fn remove_idx(&self, idx: usize) -> Option<CosmicSurface> {
        let window = self.0.with_program(|p| {
            if p.windows.len() == 1 {
                p.override_alive = false;
                let window = p.windows.first().unwrap();
                window.try_force_undecorated(false);
                window.set_tiled(false);
                return Some(window.clone());
            }
            if p.windows.len() <= idx {
                return None;
            }
            let window = p.windows.remove(idx);
            // TODO Send `leave` if window matches `p.previous_keyboard`?
            window.try_force_undecorated(false);
            window.set_tiled(false);

            p.active = p.active.min(p.windows.len() - 1);
            p.previous_index.take_if(|(_, idx)| *idx >= p.windows.len());
>>>>>>> upstream/master

                (Some(window), Some((p.stack_id, idx as u32)))
            }
        };
        if let Some((stack_id, index)) = tab_event {
            self.handle.insert_idle(move |state| {
                state
                    .common
                    .shell_overlay_state
                    .send_tab_removed(stack_id, index);
            });
        }
        window
    }

    /// Returns the unique stack identifier used in the shell overlay protocol.
    pub fn stack_id(&self) -> u32 {
        let p = self.p();
        p.stack_id
    }

    pub fn len(&self) -> usize {
<<<<<<< HEAD
        let p = self.p();
        p.windows.lock().unwrap().len()
=======
        self.0.with_program(|p| p.windows.len())
>>>>>>> upstream/master
    }

    pub fn handle_focus(
        &self,
        seat: &Seat<State>,
        direction: FocusDirection,
        swap: Option<NodeDesc>,
    ) -> bool {
        let (result, _update) = {
            let p = self.p();
            let last_mod_serial = seat.last_modifier_change();
            if !p
                .previous_index
                .is_some_and(|(serial, _)| Some(serial) == last_mod_serial)
            {
                p.previous_index = last_mod_serial.map(|s| (s, p.active));
            }

            match direction {
                FocusDirection::Left => {
                    if !p.group_focused {
                        if let Some(new) = p.active.checked_sub(1) {
                            p.scroll_to_focus = true;
                            p.active = new;
                            (true, true)
                        } else {
                            let old = p.active;
                            p.active = p.previous_index.unwrap().1;
                            if p.active != old {
                                p.scroll_to_focus = true;
                                (false, true)
                            } else {
                                (false, false)
                            }
                        }
                    } else {
                        (false, false)
                    }
                }
                FocusDirection::Right => {
                    if !p.group_focused {
                        let max = p.windows.len();
                        if p.active < max - 1 {
                            p.scroll_to_focus = true;
                            p.active += 1;
                            (true, true)
                        } else {
                            let old = p.active;
                            p.active = p.previous_index.unwrap().1;
                            if old != p.active {
                                p.scroll_to_focus = true;
                                (false, true)
                            } else {
                                (false, false)
                            }
                        }
                    } else {
                        (false, false)
                    }
                }
                FocusDirection::Out if swap.is_none() => {
                    if !mem::replace(&mut p.group_focused, true) {
                        p.windows.iter().for_each(|w| {
                            w.set_activated(false);
                            w.send_configure();
                        });
                        (true, true)
                    } else {
                        (false, false)
                    }
                }
                FocusDirection::In if swap.is_none() => {
                    if !mem::replace(&mut p.group_focused, false) {
                        p.windows.iter().enumerate().for_each(|(i, w)| {
                            w.set_activated(p.active == i);
                            w.send_configure();
                        });

                        (true, true)
                    } else {
                        (false, false)
                    }
                }
                FocusDirection::Up | FocusDirection::Down => {
                    if !p.group_focused {
                        let old = p.active;
                        p.active = p.previous_index.unwrap().1;
                        if old != p.active {
                            p.scroll_to_focus = true;
                        }
                        (false, true)
                    } else {
                        (false, false)
                    }
                }
                _ => (false, false),
            }
        };

        result
    }

    pub fn handle_move(&self, direction: Direction) -> MoveResult {
<<<<<<< HEAD
        let loop_handle = self.handle.clone();

        {
            let p = self.p();
            let prev_idx = p.previous_index.lock().unwrap();

            if p.group_focused.load(Ordering::SeqCst) {
                MoveResult::Default
            } else {
                let active = p.active.load(Ordering::SeqCst);
                let mut windows = p.windows.lock().unwrap();
=======
        let loop_handle = self.0.loop_handle();
        let result = self.0.with_program(|p| {
            if p.group_focused {
                return MoveResult::Default;
            }

            let next = match direction {
                Direction::Left => p.active.checked_sub(1),
                Direction::Right => (p.active + 1 < p.windows.len()).then_some(p.active + 1),
                Direction::Down | Direction::Up => None,
            };

            if let Some(val) = next {
                let old = p.active;
                p.active = val;
                p.windows.swap(old, val);
                p.scroll_to_focus = true;
                MoveResult::Handled
            } else {
                if p.windows.len() == 1 {
                    return MoveResult::Default;
                }
                let window = p.windows.remove(p.active);
                // TODO Send `leave` if window matches `p.previous_keyboard`?
                if let Some(prev_idx) = p
                    .previous_index
                    .map(|(_, idx)| idx)
                    .filter(|idx| *idx < p.windows.len())
                {
                    p.active = prev_idx;
                    p.scroll_to_focus = true;
                } else if p.active == p.windows.len() {
                    p.active -= 1;
                    p.scroll_to_focus = true;
                }
                window.try_force_undecorated(false);
                window.set_tiled(false);
>>>>>>> upstream/master

                let next = match direction {
                    Direction::Left => active.checked_sub(1),
                    Direction::Right => (active + 1 < windows.len()).then_some(active + 1),
                    Direction::Down | Direction::Up => None,
                };

                if let Some(val) = next {
                    let old = p.active.swap(val, Ordering::SeqCst);
                    windows.swap(old, val);
                    p.previous_keyboard.store(old, Ordering::SeqCst);
                    p.scroll_to_focus.store(true, Ordering::SeqCst);
                    MoveResult::Handled
                } else if windows.len() == 1 {
                    MoveResult::Default
                } else {
                    let window = windows.remove(active);
                    if let Some(prev_idx) = prev_idx
                        .map(|(_, idx)| idx)
                        .filter(|idx| *idx < windows.len())
                    {
                        p.active.store(prev_idx, Ordering::SeqCst);
                        p.scroll_to_focus.store(true, Ordering::SeqCst);
                    } else if active == windows.len() {
                        p.active.store(active - 1, Ordering::SeqCst);
                        p.scroll_to_focus.store(true, Ordering::SeqCst);
                    }
                    window.try_force_undecorated(false);
                    window.set_tiled(false);

                    MoveResult::MoveOut(window, loop_handle)
                }
            }
        }
    }

    pub fn active(&self) -> CosmicSurface {
<<<<<<< HEAD
        let p = self.p();
        p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)].clone()
    }

    pub fn has_active(&self, window: &CosmicSurface) -> bool {
        let p = self.p();
        &p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)] == window
    }

    pub fn whole_stack_focused(&self) -> bool {
        let p = self.p();
        p.group_focused.load(Ordering::SeqCst)
=======
        self.0.with_program(|p| p.windows[p.active].clone())
    }

    pub fn has_active(&self, window: &CosmicSurface) -> bool {
        self.0.with_program(|p| &p.windows[p.active] == window)
    }

    pub fn whole_stack_focused(&self) -> bool {
        self.0.with_program(|p| p.group_focused)
>>>>>>> upstream/master
    }

    pub fn set_active<S>(&self, window: &S)
    where
        CosmicSurface: PartialEq<S>,
    {
<<<<<<< HEAD
        let tab_event = {
            let p = self.p();
            if let Some(val) = p.windows.lock().unwrap().iter().position(|w| w == window) {
                let old = p.active.swap(val, Ordering::SeqCst);
                if old != val {
                    p.previous_keyboard.store(old, Ordering::SeqCst);
                    Some((p.stack_id, val as u32))
                } else {
                    None
                }
            } else {
                None
=======
        self.0.with_program(|p| {
            if let Some(val) = p.windows.iter().position(|w| w == window) {
                p.active = val;
>>>>>>> upstream/master
            }
        };
        if let Some((stack_id, index)) = tab_event {
            self.handle.insert_idle(move |state| {
                state
                    .common
                    .shell_overlay_state
                    .send_tab_activated(stack_id, index);
            });
        }
    }

    pub fn set_tiled(&self, tiled: bool) {
<<<<<<< HEAD
        let p = self.p();
        p.tiled.store(tiled, Ordering::Release);
    }

    /// Whether this stack is currently in the workspace's tiling layer.
    ///
    /// This is the workspace-level placement flag, written by
    /// [`Workspace::set_tiled`]. Distinct from any xdg-toplevel tiled
    /// state which the compositor may set on individual stacked windows
    /// for clipping intent regardless of placement.
    pub fn is_tiled(&self) -> bool {
        self.p().tiled.load(Ordering::Acquire)
    }

    pub fn surfaces(&self) -> impl Iterator<Item = CosmicSurface> {
        let p = self.p();
        p.windows
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
=======
        self.0.with_program(|p| {
            p.tiled = tiled;
        });
    }

    pub fn surfaces(&self) -> impl Iterator<Item = CosmicSurface> {
        self.0.with_program(|p| p.windows.clone().into_iter())
>>>>>>> upstream/master
    }

    pub fn focus_under(
        &self,
        mut relative_pos: Point<f64, Logical>,
        surface_type: WindowSurfaceType,
    ) -> Option<(PointerFocusTarget, Point<f64, Logical>)> {
<<<<<<< HEAD
        let p = self.p();
        let mut stack_ui = None;
        let geo = p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)].geometry();
=======
        self.0.with_program(|p| {
            let mut stack_ui = None;
            let geo = p.windows[p.active].geometry();
>>>>>>> upstream/master

        if surface_type.contains(WindowSurfaceType::TOPLEVEL) {
            let point_i32 = relative_pos.to_i32_round::<i32>();
            if (point_i32.x - geo.loc.x >= -RESIZE_BORDER && point_i32.x - geo.loc.x < 0)
                || (point_i32.y - geo.loc.y >= -RESIZE_BORDER && point_i32.y - geo.loc.y < 0)
                || (point_i32.x - geo.loc.x >= geo.size.w
                    && point_i32.x - geo.loc.x < geo.size.w + RESIZE_BORDER)
                || (point_i32.y - geo.loc.y >= geo.size.h + TAB_HEIGHT
                    && point_i32.y - geo.loc.y < geo.size.h + TAB_HEIGHT + RESIZE_BORDER)
            {
                stack_ui = Some((
                    PointerFocusTarget::StackUI(self.clone()),
                    Point::from((0., 0.)),
                ));
            }

            if point_i32.y - geo.loc.y < TAB_HEIGHT {
                stack_ui = Some((
                    PointerFocusTarget::StackUI(self.clone()),
                    Point::from((0., 0.)),
                ));
            }
        }

<<<<<<< HEAD
        relative_pos.y -= TAB_HEIGHT as f64;

        let active_window = &p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)];
        stack_ui.or_else(|| {
            active_window
                .focus_under(relative_pos, surface_type)
                .map(|(target, surface_offset)| {
                    (
                        target,
                        surface_offset.to_f64() + Point::from((0., TAB_HEIGHT as f64)),
                    )
                })
=======
            let active_window = &p.windows[p.active];
            stack_ui.or_else(|| {
                active_window.focus_under(relative_pos, surface_type).map(
                    |(target, surface_offset)| {
                        (
                            target,
                            surface_offset.to_f64() + Point::from((0., TAB_HEIGHT as f64)),
                        )
                    },
                )
            })
>>>>>>> upstream/master
        })
    }

    pub fn offset(&self) -> Point<i32, Logical> {
        Point::from((0, TAB_HEIGHT))
    }

    pub fn pending_size(&self) -> Option<Size<i32, Logical>> {
<<<<<<< HEAD
        let p = self.p();
        (*p.geometry.lock().unwrap()).map(|geo| geo.size.as_logical())
=======
        self.0
            .with_program(|p| p.geometry.map(|geo| geo.size.as_logical()))
>>>>>>> upstream/master
    }

    pub fn last_server_size(&self) -> Option<Size<i32, Logical>> {
        let mut size = self.active().last_server_size()?;
        // if stacked the window doesn't have SSD, instead it has a tab
        size.h += TAB_HEIGHT;
        Some(size)
    }

    pub fn set_geometry(&self, geo: Rectangle<i32, Global>) {
        let p = self.p();
        let loc = (geo.loc.x, geo.loc.y + TAB_HEIGHT);
        let size = (geo.size.w, geo.size.h - TAB_HEIGHT);

<<<<<<< HEAD
        let win_geo = Rectangle::new(loc.into(), size.into());
        for window in p.windows.lock().unwrap().iter() {
            window.set_geometry(win_geo, TAB_HEIGHT as u32);
        }

        *p.geometry.lock().unwrap() = Some(geo);
        p.mask.lock().unwrap().take();
=======
            let win_geo = Rectangle::new(loc.into(), size.into());
            for window in p.windows.iter() {
                window.set_geometry(win_geo, TAB_HEIGHT as u32);
            }

            p.geometry = Some(geo);
            p.mask.take();
        });
>>>>>>> upstream/master
    }

    pub fn on_commit(&self, surface: &WlSurface) {
        if let Some(surface) = self.surfaces().find(|w| w == surface) {
            surface.0.on_commit();
        }
    }

<<<<<<< HEAD
    fn keyboard_leave_if_previous(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        serial: Serial,
    ) -> usize {
        let p = self.p();
        let active = p.active.load(Ordering::SeqCst);
        let previous = p.previous_keyboard.swap(active, Ordering::SeqCst);
        if previous != active || p.reenter.swap(false, Ordering::SeqCst) {
            let windows = p.windows.lock().unwrap();
            if let Some(previous_surface) = windows.get(previous)
                && previous != active
            {
                KeyboardTarget::leave(previous_surface, seat, data, serial);
            }
            KeyboardTarget::enter(
                &windows[active],
                seat,
                data,
                Vec::new(), /* TODO */
                serial,
            )
        }
        active
    }

    pub(in super::super) fn focus_stack(&self) {
        let p = self.p();
        p.group_focused.store(true, Ordering::SeqCst);
=======
    pub(in super::super) fn focus_stack(&self) {
        self.0.with_program(|p| {
            p.group_focused = true;
        });
>>>>>>> upstream/master
    }

    pub(in super::super) fn loop_handle(&self) -> LoopHandle<'static, crate::state::State> {
        self.handle.clone()
    }

    pub fn push_popup_render_elements<R>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
        scanout_node: Option<DrmNode>,
        push: &mut dyn FnMut(CosmicStackRenderElement<R>),
    ) where
        R: AsGlowRenderer,
        R::TextureId: Send + Clone + 'static,
    {
        let window_loc = location + Point::from((0, (TAB_HEIGHT as f64 * scale.y) as i32));
<<<<<<< HEAD
        let p = self.p();
        let windows = p.windows.lock().unwrap();
        let active = p.active.load(Ordering::SeqCst);

        let blur_strength = crate::theme::arlen_blur_strength(&crate::theme::arlen_theme());
        windows[active].push_popup_render_elements(
            renderer,
            window_loc,
            scale,
            alpha,
            scanout_node,
            blur_strength,
            &mut |elem| push(elem.into()),
        )
=======
        self.0.with_program(|p| {
            let frosted = if p.theme.cosmic().frosted_windows {
                (p.theme.cosmic().frosted as u8 + 1) as usize
            } else {
                0
            };

            p.windows[p.active].push_popup_render_elements(
                renderer,
                window_loc,
                scale,
                alpha,
                scanout_node,
                frosted,
                &mut |elem| push(elem.into()),
            )
        })
>>>>>>> upstream/master
    }

    pub fn shadow_render_element<R, C>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        max_size: Option<Size<i32, Logical>>,
        output_scale: Scale<f64>,
        scale: f64,
        alpha: f32,
    ) -> Option<C>
    where
        R: AsGlowRenderer,
        R::TextureId: Send + Clone + 'static,
        C: From<CosmicStackRenderElement<R>>,
    {
<<<<<<< HEAD
        let p = self.p();
        let windows = p.windows.lock().unwrap();
        let active = p.active.load(Ordering::SeqCst);
        let activated = p.activated.load(Ordering::Acquire);
        let appearance = p.appearance_conf.lock().unwrap();
        let tiled = p.tiled.load(Ordering::Acquire);

        if windows[active].is_maximized(false) {
            return None;
        }

        let round = appearance.clip_tiled_windows || !tiled;
        if tiled && !appearance.shadow_tiled_windows {
            return None;
        }
        let lt = crate::theme::arlen_theme();
        let radii = if round {
            crate::theme::window_frame_corners(&lt).map(|x| (x * scale as f32).round() as u8)
        } else {
            [0, 0, 0, 0]
        };

        let mut geo = SpaceElement::geometry(&windows[active]).to_f64();
        geo.size.h += TAB_HEIGHT as f64;
        if let Some(max_size) = max_size {
            geo.size = geo.size.clamp(Size::default(), max_size.to_f64());
        }
=======
        self.0.with_program(|p| {
            if p.windows[p.active].is_maximized(false) {
                return None;
            }

            let round = p.appearance_conf.clip_tiled_windows || !p.tiled;
            if p.tiled && !p.appearance_conf.shadow_tiled_windows {
                return None;
            }
            let radii = if round {
                p.theme
                    .cosmic()
                    .radius_s()
                    .map(|x| if x < 4.0 { x } else { x + 4.0 })
                    .map(|x| (x * scale as f32).round() as u8)
            } else {
                [0, 0, 0, 0]
            };

            let mut geo = SpaceElement::geometry(&p.windows[p.active]).to_f64();
            geo.size.h += TAB_HEIGHT as f64;
            if let Some(max_size) = max_size {
                geo.size = geo.size.clamp(Size::default(), max_size.to_f64());
            }
>>>>>>> upstream/master

        geo = geo.upscale(scale);
        geo.loc += location.to_f64().to_logical(output_scale);

        let window_key = CosmicMappedKey(CosmicMappedKeyInner::Stack(Arc::downgrade(&self.inner)));

<<<<<<< HEAD
        Some(
            CosmicStackRenderElement::Shadow(ShadowShader::element(
                renderer,
                window_key,
                geo.to_i32_round().as_local(),
                radii,
                if activated { alpha } else { alpha * 0.75 },
                output_scale.x,
                lt.is_dark(),
            ))
            .into(),
        )
=======
            Some(
                CosmicStackRenderElement::Shadow(ShadowShader::element(
                    renderer,
                    window_key,
                    geo.to_i32_round().as_local(),
                    radii,
                    if p.activated { alpha } else { alpha * 0.75 },
                    output_scale.x,
                    p.theme.cosmic().is_dark,
                ))
                .into(),
            )
        })
>>>>>>> upstream/master
    }

    pub fn push_render_elements<R>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        // Unused here: it clamped the border geometry, and desktop-shell
        // draws the chrome now. Kept so the signature matches upstream's.
        _max_size: Option<Size<i32, Logical>>,
        scale: Scale<f64>,
        alpha: f32,
        scanout_override: Option<bool>,
        scanout_node: Option<DrmNode>,
        push_above: &mut dyn FnMut(CosmicStackRenderElement<R>),
        push_below: &mut dyn FnMut(CosmicStackRenderElement<R>),
    ) where
        R: AsGlowRenderer,
        R::TextureId: Send + Clone + 'static,
    {
<<<<<<< HEAD
        if !{
            let p = self.p();
            p.override_alive.load(Ordering::Acquire)
        } {
            return;
        }

        let window_loc = location + Point::from((0, (TAB_HEIGHT as f64 * scale.y) as i32));

        // No tab bar: desktop-shell draws it over the shell overlay protocol.
        {
            let p = self.p();
            let windows = p.windows.lock().unwrap();
            let active = p.active.load(Ordering::SeqCst);
            let appearance = p.appearance_conf.lock().unwrap();
            let tiled = p.tiled.load(Ordering::Acquire);
            let maximized = windows[active].is_maximized(false);

            let lt = crate::theme::arlen_theme();
            let round = (appearance.clip_tiled_windows || !tiled) && !maximized;
            let radii =
                round.then(|| crate::theme::window_frame_corners(&lt).map(|x| x.round() as u8));

            // A stack always carries a header (its tab strip, drawn by
            // desktop-shell), so the active client's top corners are interior
            // and cut square; the window's bottom corners keep the radius.
            let radii = radii.map(|mut r| {
                r[corner::TOP_LEFT] = 0;
                r[corner::TOP_RIGHT] = 0;
                r
            });
            windows[active].push_render_elements(
=======
        if !self.0.with_program(|p| p.override_alive) {
            return;
        }

        let geometry = self
            .0
            .with_program(|p| p.windows[p.active].geometry())
            .to_physical_precise_round(scale);
        let stack_loc = location + geometry.loc;
        let window_loc = location + Point::from((0, (TAB_HEIGHT as f64 * scale.y) as i32));
        let frosted = self.0.with_program(|p| {
            if p.theme.cosmic().frosted_windows {
                (p.theme.cosmic().frosted as u8 + 1) as usize
            } else {
                0
            }
        });
        let radii = self.0.with_program(|p| {
            let maximized = p.windows[p.active].is_maximized(false);
            let round = (p.appearance_conf.clip_tiled_windows || !p.tiled) && !maximized;
            round.then(|| {
                p.theme
                    .cosmic()
                    .radius_s()
                    .map(|x| if x < 4.0 { x } else { x + 4.0 })
                    .map(|x| x.round() as u8)
            })
        });

        self.0.with_program(|p| {
            let maximized = p.windows[p.active].is_maximized(false);

            let mut geo = SpaceElement::geometry(&p.windows[p.active]).to_f64();
            geo.loc += location.to_f64().to_logical(scale);
            geo.size.h += TAB_HEIGHT as f64;
            if let Some(max_size) = max_size {
                geo.size = geo.size.clamp(Size::default(), max_size.to_f64());
            }

            let window_key =
                CosmicMappedKey(CosmicMappedKeyInner::Stack(Arc::downgrade(&self.0.0)));

            if !maximized {
                let (r, g, b, a) = p.theme.cosmic().bg_divider().into_components();
                push_above(CosmicStackRenderElement::Border(IndicatorShader::element(
                    renderer,
                    Key::Window(Usage::Border, window_key.clone()),
                    geo.to_i32_round().as_local(),
                    1,
                    radii.unwrap_or([0; 4]),
                    a * alpha,
                    scale.x,
                    [r, g, b],
                )));
            };

            let radii = radii.map(|[_, _, c, d]| [0, 0, c, d]);
            p.windows[p.active].push_render_elements(
>>>>>>> upstream/master
                renderer,
                window_loc,
                scale,
                alpha,
                scanout_override,
                scanout_node,
                radii.is_some(),
                radii.unwrap_or([0; 4]),
                crate::theme::arlen_blur_strength(&crate::theme::arlen_theme()),
                &mut |elem| push_above(elem.into()),
                Some(&mut |elem| push_below(elem.into())),
            );
<<<<<<< HEAD
        }
    }

    pub fn update_appearance_conf(&self, appearance: &AppearanceConfig) {
        let p = self.p();
        let mut conf = p.appearance_conf.lock().unwrap();
        if &*conf != appearance {
            *conf = *appearance;
=======
        });

        self.0.push_render_elements(
            renderer,
            stack_loc,
            scale,
            alpha,
            radii.map(|[_, b, _, d]| [0, b, 0, d]).unwrap_or([0; 4]),
            &mut |elem| push_above(elem.into()),
            Some(&mut |elem| push_below(elem.into())),
        );
    }

    pub(crate) fn set_theme(&self, mut theme: cosmic::Theme) {
        theme.transparent = theme.cosmic().frosted_windows;
        self.0.with_program(|p| {
            p.theme = theme.clone();
        });
        self.0.set_theme(theme);
    }

    pub fn update_appearance_conf(&self, appearance: &AppearanceConfig) {
        if self.0.with_program(|p| {
            if p.appearance_conf != *appearance {
                p.appearance_conf = *appearance;
                true
            } else {
                false
            }
        }) {
            self.0.force_redraw();
            self.0.force_update();
>>>>>>> upstream/master
        }
    }

    pub(crate) fn force_redraw(&self) {
        // No-op: tab bar rendering is handled by desktop-shell via the shell overlay protocol.
    }

    fn start_drag(&self, data: &mut State, seat: &Seat<State>, serial: Serial) {
<<<<<<< HEAD
        if let Some(dragged_out) = {
            let p = self.p();
            p.potential_drag.lock().unwrap().take()
        } && let Some(surface) = {
            let p = self.p();
            p.windows.lock().unwrap().get(dragged_out).cloned()
        } {
=======
        if let Some(dragged_out) = self.0.with_program(|p| p.potential_drag.take())
            && let Some(surface) = self.0.with_program(|p| p.windows.get(dragged_out).cloned())
        {
>>>>>>> upstream/master
            let seat = seat.clone();
            surface.try_force_undecorated(false);
            surface.send_configure();
            if let Some(surface) = surface.wl_surface().map(Cow::into_owned) {
                let _ = data.common.event_loop_handle.insert_idle(move |state| {
                    let res = state.common.shell.write().move_request(
                        &surface,
                        &seat,
                        serial,
                        ReleaseMode::NoMouseButtons,
                        true,
                        &state.common.config,
                        &state.common.event_loop_handle,
                        false,
                        ModalBehavior::Block,
                    );
                    if let Some((grab, focus)) = res {
                        match grab.grab_type() {
                            GrabType::Touch => {
                                seat.get_touch().unwrap().set_grab(state, grab, serial)
                            }
                            GrabType::Pointer => seat
                                .get_pointer()
                                .unwrap()
                                .set_grab(state, grab, serial, focus),
                            GrabType::TabletTool => seat
                                .tablet_seat()
                                .get_tool(grab.tool().unwrap())
                                .unwrap()
                                .set_grab(state, grab, InputTime::now(), serial, focus),
                        }
                    }
                });
            }
        }
    }

    pub fn min_size(&self) -> Option<Size<i32, Logical>> {
        self.surfaces()
            .fold(None, |min_size, window| {
                let win_min_size = window.min_size_without_ssd();
                match (min_size, win_min_size) {
                    (None, None) => None,
                    (None, x) | (x, None) => x,
                    (Some(min1), Some(min2)) => {
                        Some((min1.w.max(min2.w), min1.h.max(min2.h)).into())
                    }
                }
            })
            .map(|size| size + (0, TAB_HEIGHT).into())
    }
    pub fn max_size(&self) -> Option<Size<i32, Logical>> {
        let theoretical_max = self
            .surfaces()
            .fold(None, |max_size, window| {
                let win_max_size = window.max_size_without_ssd();
                match (max_size, win_max_size) {
                    (None, None) => None,
                    (None, x) | (x, None) => x,
                    (Some(max1), Some(max2)) => Some(
                        (
                            if max1.w == 0 {
                                max2.w
                            } else if max2.w == 0 {
                                max1.w
                            } else {
                                max1.w.min(max2.w)
                            },
                            if max1.h == 0 {
                                max2.h
                            } else if max2.h == 0 {
                                max1.h
                            } else {
                                max1.h.min(max2.h)
                            },
                        )
                            .into(),
                    ),
                }
            })
            .map(|size| size + (0, TAB_HEIGHT).into());
        // The problem is, with accumulated sizes, the minimum size could be larger than our maximum...
        let min_size = self.min_size();
        match (theoretical_max, min_size) {
            (None, _) => None,
            (Some(max), None) => Some(max),
            (Some(max), Some(min)) => Some((max.w.max(min.w), max.h.max(min.h)).into()),
        }
    }

<<<<<<< HEAD
    pub fn corner_radius(&self, geometry_size: Size<i32, Logical>, _default_radius: u8) -> [u8; 4] {
        let p = self.p();
        let active_window = &p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)];
        let is_tiled = p.tiled.load(Ordering::Acquire);
        let appearance = p.appearance_conf.lock().unwrap();
        let maximized = active_window.is_maximized(false);

        let round = (appearance.clip_tiled_windows || !is_tiled) && !maximized;
        let radii = crate::theme::window_frame_corners(&crate::theme::arlen_theme())
            .map(|val| val.round() as u8);
=======
    pub fn corner_radius(&self, geometry_size: Size<i32, Logical>, default_radius: u8) -> [u8; 4] {
        self.0.with_program(|p| {
            let active_window = &p.windows[p.active];
            let maximized = active_window.is_maximized(false);

            let round = (p.appearance_conf.clip_tiled_windows || !p.tiled) && !maximized;
            let radii = p
                .theme
                .cosmic()
                .radius_s()
                .map(|x| if x < 4.0 { x } else { x + 4.0 })
                .map(|val| val.round() as u8);
>>>>>>> upstream/master

        if !round {
            let mut corners = active_window
                .corner_radius(geometry_size)
                .unwrap_or([_default_radius; 4]);

            corners[corner::TOP_LEFT] = 0;
            corners[corner::TOP_RIGHT] = 0;

<<<<<<< HEAD
            corners
        } else {
            let mut corners = active_window.corner_radius(geometry_size).unwrap_or(radii);

            // The top corners are the header's, so the frame decides them.
            corners[corner::TOP_LEFT] = radii[corner::TOP_LEFT];
            corners[corner::TOP_RIGHT] = radii[corner::TOP_RIGHT];
            corners[corner::BOTTOM_RIGHT] =
                radii[corner::BOTTOM_RIGHT].max(corners[corner::BOTTOM_RIGHT]);
            corners[corner::BOTTOM_LEFT] =
                radii[corner::BOTTOM_LEFT].max(corners[corner::BOTTOM_LEFT]);

            corners
        }
=======
                corners
            } else {
                let mut corners = active_window.corner_radius(geometry_size).unwrap_or(radii);

                corners[0] = radii[0];
                corners[1] = radii[1];
                corners[2] = radii[2].max(corners[2]);
                corners[3] = radii[3].max(corners[3]);

                corners
            }
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    DragStart,
    Menu,
    TabMenu(usize),
    PotentialTabDragStart(usize),
    Activate(usize),
    Close(usize),
    ScrollForward,
    ScrollBack,
    Scrolled,
}

impl TabMessage for Message {
    fn activate(idx: usize) -> Self {
        Message::Activate(idx)
    }

    fn scroll_back() -> Self {
        Message::ScrollBack
    }

    fn scroll_further() -> Self {
        Message::ScrollForward
    }

    fn populate_scroll(&mut self, mut current_offset: AbsoluteOffset) -> Option<AbsoluteOffset> {
        match self {
            Message::ScrollBack => Some({
                current_offset.x -= 10.;
                current_offset
            }),
            Message::ScrollForward => Some({
                current_offset.x += 10.;
                current_offset
            }),
            _ => None,
        }
    }

    fn scrolled() -> Self {
        Message::Scrolled
    }
}

impl Program for CosmicStackInternal {
    type Message = Message;

    fn update(
        &mut self,
        message: Self::Message,
        loop_handle: &LoopHandle<'static, crate::state::State>,
        last_seat: Option<&(Seat<State>, Serial)>,
    ) -> Task<Self::Message> {
        match message {
            Message::DragStart => {
                if let Some((seat, serial)) = last_seat.cloned()
                    && let Some(surface) =
                        self.windows[self.active].wl_surface().map(Cow::into_owned)
                {
                    loop_handle.insert_idle(move |state| {
                        let res = state.common.shell.write().move_request(
                            &surface,
                            &seat,
                            serial,
                            ReleaseMode::NoMouseButtons,
                            false,
                            &state.common.config,
                            &state.common.event_loop_handle,
                            false,
                            ModalBehavior::Block,
                        );
                        if let Some((grab, focus)) = res {
                            match grab.grab_type() {
                                GrabType::Touch => {
                                    seat.get_touch().unwrap().set_grab(state, grab, serial)
                                }
                                GrabType::Pointer => seat
                                    .get_pointer()
                                    .unwrap()
                                    .set_grab(state, grab, serial, focus),
                                GrabType::TabletTool => seat
                                    .tablet_seat()
                                    .get_tool(grab.tool().unwrap())
                                    .unwrap()
                                    .set_grab(state, grab, InputTime::now(), serial, focus),
                            }
                        }
                    });
                }
            }
            Message::PotentialTabDragStart(idx) => {
                self.potential_drag = Some(idx);
            }
            Message::Activate(idx) => {
                self.potential_drag = None;
                if let Some(surface) = self.windows.get(idx).cloned() {
                    loop_handle.insert_idle(move |state| {
                        if let Some(mapped) =
                            state.common.shell.read().element_for_surface(&surface)
                        {
                            mapped.stack_ref().unwrap().set_active(&surface);
                        }
                    });
                    self.scroll_to_focus = true;
                }
            }
            Message::Close(idx) => {
                if let Some(val) = self.windows.get(idx) {
                    val.close()
                }
            }
            Message::Scrolled => {
                self.scroll_to_focus = false;
            }
            Message::Menu => {
                if let Some((seat, serial)) = last_seat.cloned()
                    && let Some(surface) =
                        self.windows[self.active].wl_surface().map(Cow::into_owned)
                {
                    loop_handle.insert_idle(move |state| {
                        let shell = state.common.shell.read();
                        if let Some(mapped) = shell.element_for_surface(&surface).cloned() {
                            let position = if let Some((output, set)) =
                                shell.workspaces.sets.iter().find(|(_, set)| {
                                    set.sticky_layer.mapped().any(|m| m == &mapped)
                                }) {
                                set.sticky_layer
                                    .element_geometry(&mapped)
                                    .unwrap()
                                    .loc
                                    .to_global(output)
                            } else if let Some(workspace) = shell.space_for(&mapped) {
                                let Some(elem_geo) = workspace.element_geometry(&mapped) else {
                                    return;
                                };
                                elem_geo.loc.to_global(&workspace.output)
                            } else {
                                return;
                            };

                            let mut cursor = seat
                                .get_pointer()
                                .unwrap()
                                .current_location()
                                .to_i32_round();
                            cursor.y -= TAB_HEIGHT;
                            let res = shell.menu_request(
                                false,
                                &surface,
                                &seat,
                                serial,
                                cursor - position.as_logical(),
                                true,
                                &state.common.config,
                                &state.common.event_loop_handle,
                            );

                            std::mem::drop(shell);
                            if let Some((grab, focus)) = res {
                                seat.get_pointer()
                                    .unwrap()
                                    .set_grab(state, grab, serial, focus);
                            }
                        }
                    });
                }
            }
            Message::TabMenu(idx) => {
                if let Some((seat, serial)) = last_seat.cloned()
                    && let Some(surface) = self.windows[idx].wl_surface().map(Cow::into_owned)
                {
                    loop_handle.insert_idle(move |state| {
                        let shell = state.common.shell.read();
                        if let Some(mapped) = shell.element_for_surface(&surface).cloned()
                            && let Some(workspace) = shell.space_for(&mapped)
                        {
                            let Some(elem_geo) = workspace.element_geometry(&mapped) else {
                                return;
                            };
                            let position = elem_geo.loc.to_global(&workspace.output);

                            let mut cursor = seat
                                .get_pointer()
                                .unwrap()
                                .current_location()
                                .to_i32_round();
                            cursor.y -= TAB_HEIGHT;
                            let res = shell.menu_request(
                                false,
                                &surface,
                                &seat,
                                serial,
                                cursor - position.as_logical(),
                                false,
                                &state.common.config,
                                &state.common.event_loop_handle,
                            );

                            std::mem::drop(shell);
                            if let Some((grab, focus)) = res {
                                seat.get_pointer()
                                    .unwrap()
                                    .set_grab(state, grab, serial, focus);
                            }
                        }
                    });
                }
            }
            _ => unreachable!(),
        }
        Task::none()
    }

    fn view(&self) -> CosmicElement<'_, Self::Message> {
        HOOKS.get().unwrap().stack_decorations.view(self)
    }

    fn foreground(
        &self,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        damage: &[Rectangle<i32, Buffer>],
        scale: f32,
        theme: &Theme,
    ) {
        if self.group_focused {
            let border = Rectangle::new(
                (0, ((TAB_HEIGHT as f32 * scale) - scale).floor() as i32).into(),
                (pixels.width() as i32, scale.ceil() as i32).into(),
            );

            let mut paint = tiny_skia::Paint::default();
            let (b, g, r, a) = theme.cosmic().accent_color().into_components();
            paint.set_color(tiny_skia::Color::from_rgba(r, g, b, a).unwrap());

            for rect in damage {
                if let Some(overlap) = rect.intersection(border) {
                    pixels.fill_rect(
                        tiny_skia::Rect::from_xywh(
                            overlap.loc.x as f32,
                            overlap.loc.y as f32,
                            overlap.size.w as f32,
                            overlap.size.h as f32,
                        )
                        .unwrap(),
                        &paint,
                        Default::default(),
                        None,
                    )
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct DefaultDecorations;

impl Decorations<CosmicStackInternal, Message> for DefaultDecorations {
    fn view(&self, stack: &CosmicStackInternal) -> cosmic::Element<'_, Message> {
        if stack.geometry.is_none() {
            return widget::space().into();
        };

        let elements = [
            widget::icon::from_name("window-stack-symbolic")
                .size(16)
                .prefer_svg(true)
                .icon()
                .class(if stack.group_focused {
                    theme::Svg::custom(|theme| widget::svg::Style {
                        color: Some(if theme.cosmic().is_dark {
                            Color::BLACK
                        } else {
                            Color::WHITE
                        }),
                    })
                } else {
                    theme::Svg::Default
                })
                .apply(widget::container)
                .center_y(24)
                .center_x(64)
                .apply(widget::mouse_area)
                .on_press(Message::DragStart)
                .on_right_press(Message::Menu)
                .into(),
            CosmicElement::new(
                Tabs::new(
                    stack.windows.iter().enumerate().map(|(i, w)| {
                        let user_data = w.user_data();
                        user_data.insert_if_missing(Id::unique);
                        Tab::new(
                            w.title(),
                            w.app_id(),
                            user_data.get::<Id>().unwrap().clone(),
                        )
                        .on_press(Message::PotentialTabDragStart(i))
                        .on_right_click(Message::TabMenu(i))
                        .on_close(Message::Close(i))
                    }),
                    stack.active,
                    stack.windows[stack.active].is_activated(false),
                    stack.group_focused,
                )
                .id(SCROLLABLE_ID.clone())
                .force_visible(stack.scroll_to_focus.then_some(stack.active))
                .height(Length::Fill)
                .width(Length::Fill),
            ),
            widget::space()
                .height(24)
                .width(64)
                .apply(widget::mouse_area)
                .on_press(Message::DragStart)
                .on_right_press(Message::Menu)
                .into(),
        ];

        let radius = if stack.windows[stack.active].is_maximized(false)
            || (stack.tiled && !stack.appearance_conf.clip_tiled_windows)
        {
            Radius::from(0.0)
        } else {
            let radii = stack
                .theme
                .cosmic()
                .radius_s()
                .map(|x| if x < 4.0 { x } else { x + 4.0 });
            Radius::from([radii[0], radii[1], 0., 0.])
        };
        let group_focused = stack.group_focused;

        widget::row(elements)
            .height(TAB_HEIGHT as u16)
            .width(Length::Fill)
            .apply(widget::container)
            .align_y(Alignment::Center)
            .class(theme::Container::custom(move |theme| {
                let cosmic_theme = theme.cosmic();

                let mut background = if group_focused {
                    cosmic_theme.accent_color()
                } else {
                    cosmic_theme.primary_container_color()
                };
                if cosmic_theme.frosted_windows {
                    background.alpha = cosmic_theme.alpha_map.blurred_alpha(cosmic_theme.frosted);
                }

                widget::container::Style {
                    snap: true,
                    icon_color: Some(
                        cosmic_theme
                            .background(cosmic_theme.frosted_windows)
                            .on
                            .into(),
                    ),
                    text_color: Some(
                        cosmic_theme
                            .background(cosmic_theme.frosted_windows)
                            .on
                            .into(),
                    ),
                    background: Some(Background::Color(background.into())),
                    border: Border {
                        radius,
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    shadow: Default::default(),
                }
            }))
            .into()
>>>>>>> upstream/master
    }
}

impl IsAlive for CosmicStack {
    fn alive(&self) -> bool {
<<<<<<< HEAD
        let p = self.p();
        p.override_alive.load(Ordering::SeqCst)
            && p.windows.lock().unwrap().iter().any(IsAlive::alive)
=======
        self.0
            .with_program(|p| p.override_alive && p.windows.iter().any(IsAlive::alive))
>>>>>>> upstream/master
    }
}

impl SpaceElement for CosmicStack {
    fn bbox(&self) -> Rectangle<i32, Logical> {
<<<<<<< HEAD
        let p = self.p();
        let mut bbox =
            SpaceElement::bbox(&p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)]);
        bbox.loc -= Point::from((RESIZE_BORDER, RESIZE_BORDER));
        bbox.size += Size::from((RESIZE_BORDER * 2, RESIZE_BORDER * 2));
        bbox.size.h += TAB_HEIGHT;
        bbox
=======
        self.0.with_program(|p| {
            let mut bbox = SpaceElement::bbox(&p.windows[p.active]);
            bbox.loc -= Point::from((RESIZE_BORDER, RESIZE_BORDER));
            bbox.size += Size::from((RESIZE_BORDER * 2, RESIZE_BORDER * 2));
            bbox.size.h += TAB_HEIGHT;
            bbox
        })
>>>>>>> upstream/master
    }
    fn is_in_input_region(&self, point: &Point<f64, Logical>) -> bool {
        self.focus_under(*point, WindowSurfaceType::ALL).is_some()
    }
    fn set_activate(&self, activated: bool) {
<<<<<<< HEAD
        let changed = {
            let p = self.p();
            if !p.group_focused.load(Ordering::SeqCst) {
                p.windows
                    .lock()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .for_each(|(i, w)| {
                        w.set_activated(activated && p.active.load(Ordering::SeqCst) == i);
                        w.send_configure();
                    });
            }
            p.activated.swap(activated, Ordering::SeqCst) != activated
        };
=======
        SpaceElement::set_activate(&self.0, activated);
        let changed = self.0.with_program(|p| {
            if !p.group_focused {
                p.windows.iter().enumerate().for_each(|(i, w)| {
                    w.set_activated(activated && p.active == i);
                    w.send_configure();
                });
            }
            mem::replace(&mut p.activated, activated) != activated
        });
>>>>>>> upstream/master

        let _ = changed;
    }
    fn output_enter(&self, output: &Output, overlap: Rectangle<i32, Logical>) {
<<<<<<< HEAD
        let p = self.p();
        p.windows
            .lock()
            .unwrap()
            .iter()
            .for_each(|w| SpaceElement::output_enter(w, output, overlap))
    }
    fn output_leave(&self, output: &Output) {
        let p = self.p();
        p.windows
            .lock()
            .unwrap()
            .iter()
            .for_each(|w| SpaceElement::output_leave(w, output))
    }
    fn geometry(&self) -> Rectangle<i32, Logical> {
        let p = self.p();
        let mut geo =
            SpaceElement::geometry(&p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)]);
        geo.size.h += TAB_HEIGHT;
        geo
    }
    fn z_index(&self) -> u8 {
        let p = self.p();
        SpaceElement::z_index(&p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)])
    }
    fn refresh(&self) {
        let p = self.p();
        let mut windows = p.windows.lock().unwrap();

        // don't let the stack become empty
        let old_active = p.active.load(Ordering::SeqCst);
        let active = windows[old_active].clone();
        windows.retain(IsAlive::alive);
        if windows.is_empty() {
            windows.push(active);
        }

        let len = windows.len();
        let _ = p
            .active
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |active| {
                (active >= len).then_some(len - 1)
=======
        SpaceElement::output_enter(&self.0, output, overlap);
        self.0.with_program(|p| {
            p.windows
                .iter()
                .for_each(|w| SpaceElement::output_enter(w, output, overlap))
        })
    }
    fn output_leave(&self, output: &Output) {
        SpaceElement::output_leave(&self.0, output);
        self.0.with_program(|p| {
            p.windows
                .iter()
                .for_each(|w| SpaceElement::output_leave(w, output))
        })
    }
    fn geometry(&self) -> Rectangle<i32, Logical> {
        self.0.with_program(|p| {
            let mut geo = SpaceElement::geometry(&p.windows[p.active]);
            geo.size.h += TAB_HEIGHT;
            geo
        })
    }
    fn z_index(&self) -> u8 {
        self.0
            .with_program(|p| SpaceElement::z_index(&p.windows[p.active]))
    }
    fn refresh(&self) {
        self.0.with_program(|p| {
            // don't let the stack become empty
            let old_active = p.active;
            let active = p.windows[old_active].clone();
            p.windows.retain(IsAlive::alive);
            if p.windows.is_empty() {
                p.windows.push(active);
            }

            p.active = p.active.min(p.windows.len() - 1);

            if old_active != p.active {
                self.0
                    .resize(Size::from((self.active().geometry().size.w, TAB_HEIGHT)));
                self.0.force_redraw();
            }

            p.windows.iter().enumerate().for_each(|(i, w)| {
                if i == p.active {
                    w.set_suspended(false);
                } else {
                    w.set_suspended(true);
                }
                w.send_configure();

                SpaceElement::refresh(w)
>>>>>>> upstream/master
            });
        let active = p.active.load(Ordering::SeqCst);

        windows.iter().enumerate().for_each(|(i, w)| {
            if i == active {
                w.set_suspended(false);
            } else {
                w.set_suspended(true);
            }
            w.send_configure();

            SpaceElement::refresh(w)
        });
    }
}

impl KeyboardTarget<State> for CosmicStack {
    fn enter(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        keys: Vec<KeysymHandle<'_>>,
        serial: Serial,
    ) {
<<<<<<< HEAD
        let p = self.p();
        let active = p.active.load(Ordering::SeqCst);
        p.previous_keyboard.store(active, Ordering::SeqCst);
        KeyboardTarget::enter(
            &p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)],
            seat,
            data,
            keys,
            serial,
        )
    }
    fn leave(&self, seat: &Seat<State>, data: &mut State, serial: Serial) {
        let active = self.keyboard_leave_if_previous(seat, data, serial);
        let p = self.p();
        p.group_focused.store(false, Ordering::SeqCst);
        KeyboardTarget::leave(&p.windows.lock().unwrap()[active], seat, data, serial)
=======
        self.0.with_program(|p| {
            KeyboardTarget::enter(&p.windows[p.active], seat, data, keys, serial);
            p.previous_keyboard = Some(p.windows[p.active].downgrade());
        })
    }
    fn leave(&self, seat: &Seat<State>, data: &mut State, serial: Serial) {
        self.0.with_program(|p| {
            p.group_focused = false;
            if let Some(window) = p
                .previous_keyboard
                .take()
                .and_then(|prev| p.windows.iter().find(|w| **w == prev))
            {
                KeyboardTarget::leave(window, seat, data, serial)
            }
        })
>>>>>>> upstream/master
    }
    fn key(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        key: KeysymHandle<'_>,
        state: KeyState,
        serial: Serial,
        time: InputTime,
    ) {
<<<<<<< HEAD
        let active = self.keyboard_leave_if_previous(seat, data, serial);
        let p = self.p();
        if !p.group_focused.load(Ordering::SeqCst) {
            KeyboardTarget::key(
                &p.windows.lock().unwrap()[active],
                seat,
                data,
                key,
                state,
                serial,
                time,
            )
        }
=======
        self.0.with_program(|p| {
            p.keyboard_leave_if_previous(seat, data, serial);
            if !p.group_focused {
                KeyboardTarget::key(&p.windows[p.active], seat, data, key, state, serial, time)
            }
        })
>>>>>>> upstream/master
    }
    fn modifiers(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        modifiers: ModifiersState,
        serial: Serial,
    ) {
<<<<<<< HEAD
        let active = self.keyboard_leave_if_previous(seat, data, serial);
        let p = self.p();
        if !p.group_focused.load(Ordering::SeqCst) {
            KeyboardTarget::modifiers(
                &p.windows.lock().unwrap()[active],
                seat,
                data,
                modifiers,
                serial,
            )
        }
=======
        self.0.with_program(|p| {
            p.keyboard_leave_if_previous(seat, data, serial);
            if !p.group_focused {
                KeyboardTarget::modifiers(&p.windows[p.active], seat, data, modifiers, serial)
            }
        })
>>>>>>> upstream/master
    }
}

impl PointerTarget<State> for CosmicStack {
<<<<<<< HEAD
    fn enter(&self, seat: &Seat<State>, _data: &mut State, event: &PointerMotionEvent) {
        let p = self.p();
        let active_window = &p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)];
        let Some(next) = Focus::under(active_window, TAB_HEIGHT, event.location) else {
            return;
        };
        let _old_focus = p.swap_focus(Some(next));

        let mut cursor_state = seat
            .user_data()
            .get::<CursorState>()
            .unwrap()
            .lock()
            .unwrap();
        cursor_state.set_shape(next.cursor_shape());
        seat.set_cursor_image_status(CursorImageStatus::default_named());
    }

    fn motion(&self, seat: &Seat<State>, data: &mut State, event: &PointerMotionEvent) {
        // Disarm any double-click baseline if the pointer has moved
        // far enough between the two clicks to look like an
        // intentional drag. See seats::DoubleClickTracker for the
        // distance threshold.
        seat.double_click_tracker()
            .invalidate_on_motion(event.location);

        let event = event.clone();
        {
            let p = self.p();
            let active = p.active.load(Ordering::SeqCst);
            let active_window = &p.windows.lock().unwrap()[active];
=======
    fn enter(&self, seat: &Seat<State>, data: &mut State, event: &PointerMotionEvent) {
        let mut event = event.clone();
        self.0.with_program(|p| {
            let active_window = &p.windows[p.active];
            let Some(next) = Focus::under(active_window, TAB_HEIGHT, event.location) else {
                return;
            };
            let _old_focus = p.swap_focus(Some(next));

            let mut cursor_state = seat
                .user_data()
                .get::<CursorState>()
                .unwrap()
                .lock()
                .unwrap();
            cursor_state.set_shape(next.cursor_shape());
            seat.set_cursor_image_status(CursorImageStatus::default_named());
        });

        event.location -= self
            .0
            .with_program(|p| p.windows[p.active].geometry().loc.to_f64());
        PointerTarget::enter(&self.0, seat, data, &event)
    }

    fn motion(&self, seat: &Seat<State>, data: &mut State, event: &PointerMotionEvent) {
        let mut event = event.clone();
        self.0.with_program(|p| {
            let active_window = &p.windows[p.active];
>>>>>>> upstream/master
            let Some(next) = Focus::under(active_window, TAB_HEIGHT, event.location) else {
                return;
            };
            let _previous = p.swap_focus(Some(next));

            let mut cursor_state = seat
                .user_data()
                .get::<CursorState>()
                .unwrap()
                .lock()
                .unwrap();
            cursor_state.set_shape(next.cursor_shape());
            seat.set_cursor_image_status(CursorImageStatus::default_named());
        }

<<<<<<< HEAD
        let active_window_geo = {
            let p = self.p();
            p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)].geometry()
        };
        let adjusted_location = event.location - active_window_geo.loc.to_f64();
=======
        let active_window_geo = self.0.with_program(|p| p.windows[p.active].geometry());
        event.location -= active_window_geo.loc.to_f64();
>>>>>>> upstream/master

        if adjusted_location.y < 0.0
            || adjusted_location.y > TAB_HEIGHT as f64
            || adjusted_location.x < 64.0
            || adjusted_location.x > (active_window_geo.size.w as f64 - 64.0)
        {
            self.start_drag(data, seat, event.serial);
        }
    }

    fn relative_motion(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _event: &RelativeMotionEvent,
    ) {
    }

    fn button(&self, seat: &Seat<State>, _data: &mut State, event: &PointerButtonEvent) {
        let current_focus = {
            let p = self.p();
            p.current_focus()
        };
        match current_focus {
            Some(Focus::Header) => {
                // Left click: start drag. Right click: open context menu.
                if event.state == smithay::backend::input::ButtonState::Pressed {
                    if event.button == 0x111 {
                        // Right click: open context menu
                        let seat = seat.clone();
                        let serial = event.serial;
                        let active_surface = {
                            let p = self.p();
                            let window =
                                &p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)];
                            window.wl_surface().map(Cow::into_owned)
                        };
                        if let Some(surface) = active_surface {
                            self.handle.insert_idle(move |state| {
                                let shell = state.common.shell.read();
                                if let Some(mapped) = shell.element_for_surface(&surface).cloned() {
                                    let position = if let Some((output, set)) =
                                        shell.workspaces.sets.iter().find(|(_, set)| {
                                            set.sticky_layer.mapped().any(|m| m == &mapped)
                                        }) {
                                        set.sticky_layer
                                            .element_geometry(&mapped)
                                            .unwrap()
                                            .loc
                                            .to_global(output)
                                    } else if let Some(workspace) = shell.space_for(&mapped) {
                                        let Some(elem_geo) = workspace.element_geometry(&mapped)
                                        else {
                                            return;
                                        };
                                        elem_geo.loc.to_global(&workspace.output)
                                    } else {
                                        return;
                                    };

                                    let mut cursor = seat
                                        .get_pointer()
                                        .unwrap()
                                        .current_location()
                                        .to_i32_round();
                                    cursor.y -= TAB_HEIGHT;
                                    let res = shell.menu_request(
                                        false,
                                        &surface,
                                        &seat,
                                        serial,
                                        cursor - position.as_logical(),
                                        true,
                                        &state.common.config,
                                        &state.common.event_loop_handle,
                                        &state.common.display_handle,
                                        &mut state.common.shell_overlay_state,
                                        &mut state.common.pending_menu_callbacks,
                                    );

                                    std::mem::drop(shell);
                                    if let Some((grab, focus)) = res {
                                        seat.get_pointer()
                                            .unwrap()
                                            .set_grab(state, grab, serial, focus);
                                    }
                                }
                            });
                        }
                    } else if event.button == 0x110 {
                        // Left press on the stack title region: start
                        // a drag, unless this is the second press of
                        // a double-click in which case maximize-toggle.
                        let serial = event.serial;
                        // See CosmicWindow::button for why we don't
                        // read pointer.current_location() here — it
                        // deadlocks the compositor under the outer
                        // pointer Mutex held during button dispatch.
                        let active_surface = {
                            let p = self.p();
                            let active = p.active.load(Ordering::SeqCst);
                            p.windows.lock().unwrap()[active]
                                .wl_surface()
                                .map(Cow::into_owned)
                        };
                        if let Some(surface) = active_surface {
                            use smithay::reexports::wayland_server::Resource;
                            let target_id = surface.id().protocol_id() as u64;
                            let is_double = seat.double_click_tracker().observe_press(
                                event.time.millis(),
                                event.button,
                                target_id,
                            );
                            tracing::info!(
                                "DCLK-DEBUG CosmicStack header press time={} \
                                 button=0x{:x} target_id={} double={}",
                                event.time.millis(),
                                event.button,
                                target_id,
                                is_double
                            );
                            if is_double {
                                let seat = seat.clone();
                                let surface_cloned = surface.clone();
                                self.handle.insert_idle(move |state| {
                                    let mapped = {
                                        let shell_r = state.common.shell.read();
                                        shell_r.element_for_surface(&surface_cloned).cloned()
                                    };
                                    if let Some(mapped) = mapped {
                                        tracing::info!(
                                            "DCLK-DEBUG stack maximize_toggle target_id={}",
                                            target_id
                                        );
                                        state.common.shell.write().maximize_toggle(
                                            &mapped,
                                            &seat,
                                            &state.common.event_loop_handle,
                                            ModalBehavior::Block,
                                        );
                                    }
                                });
                                return;
                            }
                            let seat = seat.clone();
                            self.handle.insert_idle(move |state| {
                                let res = state.common.shell.write().move_request(
                                    &surface,
                                    &seat,
                                    serial,
                                    ReleaseMode::NoMouseButtons,
                                    false,
                                    &state.common.config,
                                    &state.common.event_loop_handle,
                                    false,
                                    ModalBehavior::Block,
                                );
                                if let Some((grab, focus)) = res {
                                    // Feature 4: synchronous
                                    // drag_start before set_grab,
                                    // same reasoning as
                                    // CosmicWindow::button.
                                    if let Some(sid) = grab.drag_surface_id() {
                                        tracing::info!(
                                            "ATTACH-DEBUG drag_start surface_id={} (stack)",
                                            sid
                                        );
                                        state
                                            .common
                                            .shell_overlay_state
                                            .send_window_drag_start(sid);
                                    }
                                    if matches!(grab.grab_type(), GrabType::Touch) {
                                        seat.get_touch().unwrap().set_grab(state, grab, serial);
                                    } else {
                                        seat.get_pointer()
                                            .unwrap()
                                            .set_grab(state, grab, serial, focus);
                                    }
                                }
                            });
                        }
                    }
                }
            }
            Some(x) => {
                let serial = event.serial;
                let seat = seat.clone();
<<<<<<< HEAD
                let Some(surface) = ({
                    let p = self.p();
                    let window = &p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)];
=======
                let Some(surface) = self.0.with_program(|p| {
                    let window = &p.windows[p.active];
>>>>>>> upstream/master
                    window.wl_surface().map(Cow::into_owned)
                }) else {
                    return;
                };
                self.handle.insert_idle(move |state| {
                    let res = state.common.shell.write().resize_request(
                        &surface,
                        &seat,
                        serial,
                        match x {
                            Focus::ResizeTop => ResizeEdge::TOP,
                            Focus::ResizeTopLeft => ResizeEdge::TOP_LEFT,
                            Focus::ResizeTopRight => ResizeEdge::TOP_RIGHT,
                            Focus::ResizeBottom => ResizeEdge::BOTTOM,
                            Focus::ResizeBottomLeft => ResizeEdge::BOTTOM_LEFT,
                            Focus::ResizeBottomRight => ResizeEdge::BOTTOM_RIGHT,
                            Focus::ResizeLeft => ResizeEdge::LEFT,
                            Focus::ResizeRight => ResizeEdge::RIGHT,
                            Focus::Header => unreachable!(),
                        },
                        state.common.config.cosmic_conf.edge_snap_threshold,
                        false,
                        ModalBehavior::Block,
                    );
                    if let Some((grab, focus)) = res
                        && let GrabType::Pointer = grab.grab_type()
                    {
                        seat.get_pointer()
                            .unwrap()
                            .set_grab(state, grab, serial, focus);
                    }
                });
            }
            None => {}
        }
    }

    fn axis(&self, _seat: &Seat<State>, _data: &mut State, _frame: PointerAxisFrame) {
        // No-op: tab bar scrolling was handled by IcedElement.
    }

    fn frame(&self, _seat: &Seat<State>, _data: &mut State) {
        // No-op.
    }

    fn leave(&self, seat: &Seat<State>, data: &mut State, serial: Serial, time: InputTime) {
        // Pointer left the stack; clear the double-click tracker so
        // the next window's click doesn't inherit this stack's
        // timing baseline.
        seat.double_click_tracker().invalidate();

        {
            let p = self.p();
            let mut cursor_state = seat
                .user_data()
                .get::<CursorState>()
                .unwrap()
                .lock()
                .unwrap();
            cursor_state.unset_shape();
            let _previous = p.swap_focus(None);
        }

<<<<<<< HEAD
        if let Some(dragged_out) = {
            let p = self.p();
            p.potential_drag.lock().unwrap().take()
        } && let Some(surface) = {
            let p = self.p();
            p.windows.lock().unwrap().get(dragged_out).cloned()
        } {
=======
        PointerTarget::leave(&self.0, seat, data, serial, time);

        if let Some(dragged_out) = self.0.with_program(|p| p.potential_drag.take())
            && let Some(surface) = self.0.with_program(|p| p.windows.get(dragged_out).cloned())
        {
>>>>>>> upstream/master
            let seat = seat.clone();
            surface.try_force_undecorated(false);
            surface.send_configure();
            if let Some(surface) = surface.wl_surface().map(Cow::into_owned) {
                let _ = data.common.event_loop_handle.insert_idle(move |state| {
                    let res = state.common.shell.write().move_request(
                        &surface,
                        &seat,
                        serial,
                        ReleaseMode::NoMouseButtons,
                        true,
                        &state.common.config,
                        &state.common.event_loop_handle,
                        false,
                        ModalBehavior::Block,
                    );
                    if let Some((grab, focus)) = res
                        && let GrabType::Pointer = grab.grab_type()
                    {
                        seat.get_pointer()
                            .unwrap()
                            .set_grab(state, grab, serial, focus);
                    }
                });
            }
        }

        let _ = (serial, time);
    }

    fn gesture_swipe_begin(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _event: &GestureSwipeBeginEvent,
    ) {
    }

    fn gesture_swipe_update(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _event: &GestureSwipeUpdateEvent,
    ) {
    }

    fn gesture_swipe_end(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _event: &GestureSwipeEndEvent,
    ) {
    }

    fn gesture_pinch_begin(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _event: &GesturePinchBeginEvent,
    ) {
    }

    fn gesture_pinch_update(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _event: &GesturePinchUpdateEvent,
    ) {
    }

    fn gesture_pinch_end(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _event: &GesturePinchEndEvent,
    ) {
    }

    fn gesture_hold_begin(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _event: &GestureHoldBeginEvent,
    ) {
    }

    fn gesture_hold_end(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _event: &GestureHoldEndEvent,
    ) {
    }
}

impl TouchTarget<State> for CosmicStack {
<<<<<<< HEAD
    fn down(&self, _seat: &Seat<State>, _data: &mut State, event: &TouchDownEvent) {
        self.p()
            .touch_serial
            .store(event.serial.into(), Ordering::Release);
=======
    fn down(&self, seat: &Seat<State>, data: &mut State, event: &TouchDownEvent) {
        let mut event = event.clone();
        let active_window_geo = self.0.with_program(|p| p.windows[p.active].geometry());
        event.location -= active_window_geo.loc.to_f64();
        self.0.with_program(|p| {
            p.touch_serial = event.serial.into();
        });
        TouchTarget::down(&self.0, seat, data, &event)
>>>>>>> upstream/master
    }

    fn up(&self, _seat: &Seat<State>, _data: &mut State, _event: &TouchUpEvent) {
        // No-op.
    }

    fn motion(&self, seat: &Seat<State>, data: &mut State, event: &TouchMotionEvent) {
<<<<<<< HEAD
        let active_window_geo = {
            let p = self.p();
            p.windows.lock().unwrap()[p.active.load(Ordering::SeqCst)].geometry()
        };
        let adjusted_location = event.location - active_window_geo.loc.to_f64();
=======
        let mut event = event.clone();
        let active_window_geo = self.0.with_program(|p| p.windows[p.active].geometry());
        event.location -= active_window_geo.loc.to_f64();
        TouchTarget::motion(&self.0, seat, data, &event);
>>>>>>> upstream/master

        if adjusted_location.y < 0.0
            || adjusted_location.y > TAB_HEIGHT as f64
            || adjusted_location.x < 64.0
            || adjusted_location.x > (active_window_geo.size.w as f64 - 64.0)
        {
<<<<<<< HEAD
            let serial = self.p().touch_serial.load(Ordering::Acquire).into();
            self.start_drag(data, seat, serial);
=======
            self.start_drag(data, seat, self.0.with_program(|p| p.touch_serial).into());
>>>>>>> upstream/master
        }
    }

    fn frame(&self, _seat: &Seat<State>, _data: &mut State, _frame: FrameMarker) {
        // No-op.
    }

    fn cancel(&self, _seat: &Seat<State>, _data: &mut State, _frame: FrameMarker) {
        // No-op.
    }

    fn shape(&self, _seat: &Seat<State>, _data: &mut State, _event: &ShapeEvent) {
        // No-op.
    }

    fn orientation(&self, _seat: &Seat<State>, _data: &mut State, _event: &OrientationEvent) {}

    fn last_frame(&self, _seat: &Seat<State>, _data: &mut State) -> Option<FrameMarker> {
        None
    }
}

impl TabletToolTarget<State> for CosmicStack {
    // Upstream forwards each of these to the IcedElement that draws its tab bar.
    // This fork has no such element - desktop-shell draws the tab bar over the
    // shell overlay protocol - so the compositor has nothing to hand a tablet
    // tool here. The stack still resizes and moves under a tool, because that
    // goes through the window surface below, not through this target.
    fn proximity_in(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _tool_descriptor: &TabletToolDescriptor,
        _tablet: &Tablet,
        _serial: Serial,
    ) {
    }

    fn proximity_out(
        &self,
        seat: &Seat<State>,
        _data: &mut State,
        _tool_descriptor: &TabletToolDescriptor,
    ) {
        let mut cursor_state = seat
            .user_data()
            .get::<CursorState>()
            .unwrap()
            .lock()
            .unwrap();
        cursor_state.unset_shape();
        let _previous = self.p().swap_focus(None);
    }

    fn down(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _tool_descriptor: &TabletToolDescriptor,
        event: &ToolDownEvent,
    ) {
<<<<<<< HEAD
        self.p()
            .tablet_serial
            .store(event.serial.into(), Ordering::Release);
=======
        self.0.with_program(|p| {
            p.tablet_serial = event.serial.into();
        });
        match self.0.with_program(|p| p.current_focus()) {
            Some(Focus::Header) => {
                TabletToolTarget::down(&self.0, seat, data, tool_descriptor, event)
            }
            Some(x) => {
                let serial = self.0.with_program(|p| p.tablet_serial).into();
                let seat = seat.clone();
                let Some(surface) = self.0.with_program(|p| {
                    let window = &p.windows[p.active];
                    window.wl_surface().map(Cow::into_owned)
                }) else {
                    return;
                };
                self.0.loop_handle().insert_idle(move |state| {
                    let res = state.common.shell.write().resize_request(
                        &surface,
                        &seat,
                        serial,
                        match x {
                            Focus::ResizeTop => ResizeEdge::TOP,
                            Focus::ResizeTopLeft => ResizeEdge::TOP_LEFT,
                            Focus::ResizeTopRight => ResizeEdge::TOP_RIGHT,
                            Focus::ResizeBottom => ResizeEdge::BOTTOM,
                            Focus::ResizeBottomLeft => ResizeEdge::BOTTOM_LEFT,
                            Focus::ResizeBottomRight => ResizeEdge::BOTTOM_RIGHT,
                            Focus::ResizeLeft => ResizeEdge::LEFT,
                            Focus::ResizeRight => ResizeEdge::RIGHT,
                            Focus::Header => unreachable!(),
                        },
                        state.common.config.cosmic_conf.edge_snap_threshold,
                        false,
                        ModalBehavior::Block,
                    );
                    if let Some((grab, focus)) = res
                        && let GrabType::TabletTool = grab.grab_type()
                    {
                        seat.tablet_seat()
                            .get_tool(grab.tool().unwrap())
                            .unwrap()
                            .set_grab(state, grab, InputTime::now(), serial, focus);
                    }
                });
            }
            None => {}
        }
>>>>>>> upstream/master
    }

    fn up(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _tool_descriptor: &TabletToolDescriptor,
        _event: &ToolUpEvent,
    ) {
    }

    fn motion(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _tool_descriptor: &TabletToolDescriptor,
        _event: &ToolMotionEvent,
    ) {
<<<<<<< HEAD
=======
        let mut event = event.clone();
        self.0.with_program(|p| {
            let active_window = &p.windows[p.active];
            let Some(next) = Focus::under(active_window, TAB_HEIGHT, event.location) else {
                return;
            };
            let _previous = p.swap_focus(Some(next));

            let mut cursor_state = seat
                .user_data()
                .get::<CursorState>()
                .unwrap()
                .lock()
                .unwrap();
            cursor_state.set_shape(next.cursor_shape());
            seat.set_cursor_image_status(CursorImageStatus::default_named());
        });

        let active_window_geo = self.0.with_program(|p| p.windows[p.active].geometry());
        event.location -= active_window_geo.loc.to_f64();

        TabletToolTarget::motion(&self.0, seat, data, tool_descriptor, &event);
        if event.location.y < 0.0
            || event.location.y > TAB_HEIGHT as f64
            || event.location.x < 64.0
            || event.location.x > (active_window_geo.size.w as f64 - 64.0)
        {
            self.start_drag(data, seat, self.0.with_program(|p| p.tablet_serial).into());
        }
>>>>>>> upstream/master
    }

    fn axis(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _tool_descriptor: &TabletToolDescriptor,
        _frame: ToolAxisFrame,
    ) {
    }

    fn button(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _tool_descriptor: &TabletToolDescriptor,
        _event: &ToolButtonEvent,
    ) {
    }

    fn frame(
        &self,
        _seat: &Seat<State>,
        _data: &mut State,
        _tool_descriptor: &TabletToolDescriptor,
        _time: InputTime,
    ) {
    }
}

pub enum CosmicStackRenderElement<R: Renderer + ImportAll + ImportMem> {
    Header(MemoryRenderBufferRenderElement<R>),
    Shadow(PixelShaderElement),
    Border(PixelShaderElement),
    Window(SurfaceRenderElement<R>),
}

impl<R: Renderer + ImportAll + ImportMem> From<MemoryRenderBufferRenderElement<R>>
    for CosmicStackRenderElement<R>
{
    fn from(value: MemoryRenderBufferRenderElement<R>) -> Self {
        Self::Header(value)
    }
}

impl<R: Renderer + ImportAll + ImportMem> From<SurfaceRenderElement<R>>
    for CosmicStackRenderElement<R>
{
    fn from(value: SurfaceRenderElement<R>) -> Self {
        Self::Window(value)
    }
}

impl<R> Element for CosmicStackRenderElement<R>
where
    R: Renderer + ImportAll + ImportMem + AsGlowRenderer,
    R::TextureId: Send + 'static,
{
    fn id(&self) -> &RendererId {
        match self {
            CosmicStackRenderElement::Header(elem) => elem.id(),
            CosmicStackRenderElement::Shadow(elem) => elem.id(),
            CosmicStackRenderElement::Border(elem) => elem.id(),
            CosmicStackRenderElement::Window(elem) => elem.id(),
        }
    }

    fn current_commit(&self) -> CommitCounter {
        match self {
            CosmicStackRenderElement::Header(elem) => elem.current_commit(),
            CosmicStackRenderElement::Shadow(elem) => elem.current_commit(),
            CosmicStackRenderElement::Border(elem) => elem.current_commit(),
            CosmicStackRenderElement::Window(elem) => elem.current_commit(),
        }
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        match self {
            CosmicStackRenderElement::Header(elem) => elem.src(),
            CosmicStackRenderElement::Shadow(elem) => elem.src(),
            CosmicStackRenderElement::Border(elem) => elem.src(),
            CosmicStackRenderElement::Window(elem) => elem.src(),
        }
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        match self {
            CosmicStackRenderElement::Header(elem) => elem.geometry(scale),
            CosmicStackRenderElement::Shadow(elem) => elem.geometry(scale),
            CosmicStackRenderElement::Border(elem) => elem.geometry(scale),
            CosmicStackRenderElement::Window(elem) => elem.geometry(scale),
        }
    }

    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        match self {
            CosmicStackRenderElement::Header(elem) => elem.location(scale),
            CosmicStackRenderElement::Shadow(elem) => elem.location(scale),
            CosmicStackRenderElement::Border(elem) => elem.location(scale),
            CosmicStackRenderElement::Window(elem) => elem.location(scale),
        }
    }

    fn transform(&self) -> Transform {
        match self {
            CosmicStackRenderElement::Header(elem) => elem.transform(),
            CosmicStackRenderElement::Shadow(elem) => elem.transform(),
            CosmicStackRenderElement::Border(elem) => elem.transform(),
            CosmicStackRenderElement::Window(elem) => elem.transform(),
        }
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        match self {
            CosmicStackRenderElement::Header(elem) => elem.damage_since(scale, commit),
            CosmicStackRenderElement::Shadow(elem) => elem.damage_since(scale, commit),
            CosmicStackRenderElement::Border(elem) => elem.damage_since(scale, commit),
            CosmicStackRenderElement::Window(elem) => elem.damage_since(scale, commit),
        }
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        match self {
            CosmicStackRenderElement::Header(elem) => elem.opaque_regions(scale),
            CosmicStackRenderElement::Shadow(elem) => elem.opaque_regions(scale),
            CosmicStackRenderElement::Border(elem) => elem.opaque_regions(scale),
            CosmicStackRenderElement::Window(elem) => elem.opaque_regions(scale),
        }
    }

    fn alpha(&self) -> f32 {
        match self {
            CosmicStackRenderElement::Header(elem) => elem.alpha(),
            CosmicStackRenderElement::Shadow(elem) => elem.alpha(),
            CosmicStackRenderElement::Border(elem) => elem.alpha(),
            CosmicStackRenderElement::Window(elem) => elem.alpha(),
        }
    }

    fn kind(&self) -> Kind {
        match self {
            CosmicStackRenderElement::Header(elem) => elem.kind(),
            CosmicStackRenderElement::Shadow(elem) => elem.kind(),
            CosmicStackRenderElement::Border(elem) => elem.kind(),
            CosmicStackRenderElement::Window(elem) => elem.kind(),
        }
    }

    fn is_framebuffer_effect(&self) -> bool {
        match self {
            CosmicStackRenderElement::Header(elem) => elem.is_framebuffer_effect(),
            CosmicStackRenderElement::Shadow(elem) => elem.is_framebuffer_effect(),
            CosmicStackRenderElement::Border(elem) => elem.is_framebuffer_effect(),
            CosmicStackRenderElement::Window(elem) => elem.is_framebuffer_effect(),
        }
    }
}

impl<R> RenderElement<R> for CosmicStackRenderElement<R>
where
    R: AsGlowRenderer,
    R::TextureId: Send + 'static,
{
    fn draw(
        &self,
        frame: &mut R::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), <R>::Error> {
        match self {
            CosmicStackRenderElement::Header(elem) => {
                elem.draw(frame, src, dst, damage, opaque_regions, cache)
            }
            CosmicStackRenderElement::Shadow(elem) | CosmicStackRenderElement::Border(elem) => {
                RenderElement::<GlowRenderer>::draw(
                    elem,
                    R::glow_frame_mut(frame),
                    src,
                    dst,
                    damage,
                    opaque_regions,
                    cache,
                )
                .map_err(R::from_gles_error)
            }
            CosmicStackRenderElement::Window(elem) => {
                elem.draw(frame, src, dst, damage, opaque_regions, cache)
            }
        }
    }

    fn underlying_storage(&self, renderer: &mut R) -> Option<UnderlyingStorage<'_>> {
        match self {
            CosmicStackRenderElement::Header(elem) => elem.underlying_storage(renderer),
            CosmicStackRenderElement::Shadow(elem) | CosmicStackRenderElement::Border(elem) => {
                elem.underlying_storage(renderer.glow_renderer_mut())
            }
            CosmicStackRenderElement::Window(elem) => elem.underlying_storage(renderer),
        }
    }

    fn capture_framebuffer(
        &self,
        frame: &mut R::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        cache: &UserDataMap,
    ) -> Result<(), <R>::Error> {
        match self {
            CosmicStackRenderElement::Header(elem) => {
                elem.capture_framebuffer(frame, src, dst, cache)
            }
            CosmicStackRenderElement::Shadow(elem) | CosmicStackRenderElement::Border(elem) => {
                RenderElement::<GlowRenderer>::capture_framebuffer(
                    elem,
                    R::glow_frame_mut(frame),
                    src,
                    dst,
                    cache,
                )
                .map_err(R::from_gles_error)
            }
            CosmicStackRenderElement::Window(elem) => {
                elem.capture_framebuffer(frame, src, dst, cache)
            }
        }
    }
}

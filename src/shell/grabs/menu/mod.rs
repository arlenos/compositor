use std::{
    fmt,
    sync::{Arc, Mutex},
};

use calloop::LoopHandle;
<<<<<<< HEAD
use smithay::{
    backend::renderer::{
        ImportMem, Renderer,
        element::memory::MemoryRenderBufferRenderElement,
=======
use cosmic::{
    Apply as _, Task,
    iced::{
        Alignment, Background,
        core::{Border, Length, Rectangle as IcedRectangle, alignment::Horizontal},
        widget::{self as iced_widget, Row, text::Style as TextStyle},
    },
    theme,
    widget::{button, divider, icon::from_name, menu::menu_column::MenuColumn, space, text},
};
use smithay::{
    backend::{
        input::{ButtonState, TouchSlot},
        renderer::ImportMem,
>>>>>>> upstream/master
    },
    input::{
        Seat,
        pointer::{
            AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent,
            GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
            GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent,
            GrabStartData as PointerGrabStartData, MotionEvent as PointerMotionEvent, PointerGrab,
            PointerInnerHandle, RelativeMotionEvent,
        },
        touch::{
            GrabStartData as TouchGrabStartData,
            TouchGrab, TouchInnerHandle,
        },
    },
    output::Output,
    utils::{Logical, Point, Size},
};

use crate::{
<<<<<<< HEAD
    shell::focus::target::PointerFocusTarget,
    state::State,
    utils::prelude::*,
    wayland::protocols::shell_overlay::WindowAction,
=======
    backend::render::element::AsGlowRenderer,
    shell::{SeatExt, focus::target::PointerFocusTarget},
    state::State,
    utils::{
        iced::{IcedElement, IcedRenderElement, Program},
        prelude::*,
    },
>>>>>>> upstream/master
};

use super::{GrabStartData, ResizeEdge};

mod default;
pub use self::default::*;

/// Persistent state for an active menu grab, stored on the seat.
pub struct MenuGrabState {
    screen_space_relative: Option<Output>,
    /// Set when the overlay protocol is active for this grab.
    /// Rendering is always delegated to desktop-shell.
    pub menu_id: Option<u32>,
}
pub type SeatMenuGrabState = Mutex<Option<MenuGrabState>>;

impl MenuGrabState {
<<<<<<< HEAD
    /// Render elements for the menu.
    ///
    /// With the overlay protocol active, rendering is handled entirely by
    /// desktop-shell, so this always returns an empty list.
    pub fn render<I, R>(&self, _renderer: &mut R, _output: &Output) -> Vec<I>
    where
        R: Renderer + ImportMem,
=======
    pub fn render<R>(
        &self,
        renderer: &mut R,
        output: &Output,
        push: &mut dyn FnMut(IcedRenderElement<R>),
    ) where
        R: AsGlowRenderer + ImportMem,
>>>>>>> upstream/master
        R::TextureId: Send + Clone + 'static,
    {
<<<<<<< HEAD
        Vec::new()
=======
        let scale = output.current_scale().fractional_scale();
        for elem in self.elements.lock().unwrap().iter() {
            elem.iced.push_render_elements(
                renderer,
                elem.position
                    .to_local(output)
                    .as_logical()
                    .to_physical_precise_round(scale),
                scale.into(),
                1.0,
                elem.iced
                    .with_theme(|theme| theme.cosmic().radius_s())
                    .map(|x| x.round() as u8),
                push,
                None,
            )
        }
>>>>>>> upstream/master
    }

    /// Whether the menu is positioned in screen space.
    pub fn is_in_screen_space(&self) -> bool {
        self.screen_space_relative.is_some()
    }
}

#[derive(Clone)]
pub enum Item {
    Separator,
    Submenu {
        title: String,
        items: Vec<Item>,
    },
    Entry {
        title: String,
        shortcut: Option<String>,
        on_press: Arc<Box<dyn Fn(&LoopHandle<'_, State>) + Send + Sync>>,
        toggled: bool,
        submenu: bool,
        disabled: bool,
        /// The window management action this entry maps to in the overlay protocol.
        /// `None` for items that are not sent over the protocol (e.g. zoom menu entries).
        action: Option<WindowAction>,
    },
}

impl fmt::Debug for Item {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Separator => write!(f, "Separator"),
            Self::Submenu { title, items } => f
                .debug_struct("Submenu")
                .field("title", title)
                .field("items", items)
                .finish(),
            Self::Entry {
                title,
                shortcut,
                on_press: _,
                toggled,
                submenu,
                disabled,
                action,
            } => f
                .debug_struct("Entry")
                .field("title", title)
                .field("shortcut", shortcut)
                .field("on_press", &"...")
                .field("toggled", toggled)
                .field("submenu", submenu)
                .field("disabled", disabled)
                .field("action", action)
                .finish(),
        }
    }
}

/// Flatten a tree of `Item` into a DFS-ordered `Vec<Item>` so that the
/// index the shell sends back in `activate(menu_id, index)` resolves to
/// the matching `on_press` closure.
///
/// Walk order must match `ShellOverlayState::send_context_menu`'s
/// recursive serializer exactly: each item receives one slot, and a
/// submenu header is followed immediately by its children (recursively).
/// The unit test in this module locks this invariant.
pub fn flatten_callbacks(items: &[Item]) -> Vec<Item> {
    let mut out = Vec::new();
    flatten_callbacks_into(items, &mut out);
    out
}

fn flatten_callbacks_into(items: &[Item], out: &mut Vec<Item>) {
    for item in items {
        match item {
            Item::Submenu { items: children, .. } => {
                out.push(item.clone());
                flatten_callbacks_into(children, out);
            }
            _ => out.push(item.clone()),
        }
    }
}

impl Item {
    pub fn new<S: Into<String>, F: Fn(&LoopHandle<'_, State>) + Send + Sync + 'static>(
        title: S,
        on_press: F,
    ) -> Item {
        Item::Entry {
            title: title.into(),
            shortcut: None,
            on_press: Arc::new(Box::new(on_press)),
            toggled: false,
            submenu: false,
            disabled: false,
            action: None,
        }
    }

    /// Set the `WindowAction` this entry maps to in the overlay protocol.
    pub fn action(mut self, action: WindowAction) -> Self {
        if let Item::Entry {
            action: ref mut a, ..
        } = self
        {
            *a = Some(action);
        }
        self
    }

    pub fn new_submenu<S: Into<String>>(title: S, items: Vec<Item>) -> Item {
        Item::Submenu {
            title: title.into(),
            items,
        }
    }

    pub fn shortcut(mut self, shortcut: impl Into<Option<String>>) -> Self {
        if let Item::Entry {
            shortcut: ref mut s,
            ..
        } = self
        {
            *s = shortcut.into();
        }
        self
    }

    pub fn toggled(mut self, toggled: bool) -> Self {
        if let Item::Entry {
            toggled: ref mut t, ..
        } = self
        {
            *t = toggled;
        }
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        if let Item::Entry {
            disabled: ref mut d,
            ..
        } = self
        {
            *d = disabled;
        }
        self
    }
}

<<<<<<< HEAD
/// Active menu grab.
///
/// The menu is always rendered by desktop-shell via the `arlen-shell-overlay`
/// protocol. Pointer events are forwarded to `shell_focus` so that the
/// desktop-shell client can detect clicks on the rendered menu.
=======
/// Menu that comes up when right-clicking an application header bar
#[derive(Debug)]
pub struct ContextMenu {
    items: Vec<Item>,
    selected: AtomicBool,
    row_width: Mutex<Option<f32>>,
}

impl ContextMenu {
    pub fn new(items: Vec<Item>) -> ContextMenu {
        ContextMenu {
            items,
            selected: AtomicBool::new(false),
            row_width: Mutex::new(None),
        }
    }

    pub fn set_row_width(&self, width: f32) {
        *self.row_width.lock().unwrap() = Some(width);
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    ItemEntered(usize, IcedRectangle<f32>),
    ItemPressed(usize),
    ItemLeft(usize, IcedRectangle<f32>),
}

impl item::CursorEvents for Message {
    fn cursor_entered(idx: usize, bounds: IcedRectangle<f32>) -> Self {
        Message::ItemEntered(idx, bounds)
    }

    fn cursor_left(idx: usize, bounds: IcedRectangle<f32>) -> Self {
        Message::ItemLeft(idx, bounds)
    }
}

impl Program for ContextMenu {
    type Message = Message;

    fn update(
        &mut self,
        message: Self::Message,
        loop_handle: &LoopHandle<'static, crate::state::State>,
        last_seat: Option<&(Seat<State>, Serial)>,
    ) -> Task<Self::Message> {
        match message {
            Message::ItemPressed(idx) => {
                if let Some(Item::Entry { on_press, .. }) = self.items.get_mut(idx) {
                    (on_press)(loop_handle);
                    self.selected.store(true, Ordering::SeqCst);
                }
                // TODO: If Submenu, then also expand on "Pressed" for touch events.
                // But right now we don't have any touch responsive menus with submenus
            }
            Message::ItemEntered(idx, bounds) => {
                if let Some(Item::Submenu { items, .. }) = self.items.get_mut(idx)
                    && let Some((seat, _)) = last_seat.cloned()
                {
                    let items = items.clone();
                    let _ = loop_handle.insert_idle(move |state| {
                        let grab_state = seat
                            .user_data()
                            .get::<SeatMenuGrabState>()
                            .unwrap()
                            .lock()
                            .unwrap();

                        if let Some(grab_state) = &*grab_state {
                            let mut elements = grab_state.elements.lock().unwrap();

                            let position = elements.last().unwrap().position;
                            let mut theme = state.common.theme.clone();
                            theme.transparent = theme.cosmic().frosted_system_interface;
                            let element = IcedElement::new(
                                ContextMenu::new(items),
                                Size::default(),
                                state.common.event_loop_handle.clone(),
                                theme,
                            );

                            let min_size = element.minimum_size();
                            element.with_program(|p| {
                                *p.row_width.lock().unwrap() = Some(min_size.w as f32);
                            });
                            element.resize(min_size);

                            let output = seat.active_output();
                            let position = [
                                // to the right -> down
                                Rectangle::new(
                                    position
                                        + Point::from((
                                            bounds.width.floor() as i32,
                                            bounds.y.ceil() as i32,
                                        )),
                                    min_size.as_global(),
                                ),
                                // to the right -> up
                                Rectangle::new(
                                    position
                                        + Point::from((
                                            bounds.width.floor() as i32,
                                            bounds.y.ceil() as i32 + bounds.height.ceil() as i32
                                                - min_size.h,
                                        )),
                                    min_size.as_global(),
                                ),
                                // to the left -> down
                                Rectangle::new(
                                    position
                                        + Point::from((-min_size.w + 1, bounds.y.ceil() as i32)),
                                    min_size.as_global(),
                                ),
                                // to the left -> up
                                Rectangle::new(
                                    position
                                        + Point::from((
                                            -min_size.w + 1,
                                            bounds.y.ceil() as i32 + bounds.height.ceil() as i32
                                                - min_size.h,
                                        )),
                                    min_size.as_global(),
                                ),
                            ]
                            .iter()
                            .rev() // preference of max_by_key is backwards
                            .max_by_key(|rect| {
                                output
                                    .geometry()
                                    .intersection(**rect)
                                    .map(|rect| rect.size.w * rect.size.h)
                            })
                            .unwrap()
                            .loc;
                            element.output_enter(&output, element.bbox());
                            element.set_additional_scale(*grab_state.scale.lock().unwrap());

                            elements.push(Element {
                                iced: element,
                                position,
                                pointer_entered: false,
                                touch_entered: None,
                            })
                        }
                    });
                }
            }
            Message::ItemLeft(idx, _) => {
                if let Some(Item::Submenu { .. }) = self.items.get_mut(idx)
                    && let Some((seat, _)) = last_seat.cloned()
                {
                    let _ = loop_handle.insert_idle(move |_| {
                        let grab_state = seat
                            .user_data()
                            .get::<SeatMenuGrabState>()
                            .unwrap()
                            .lock()
                            .unwrap();

                        if let Some(grab_state) = &*grab_state {
                            let mut elements = grab_state.elements.lock().unwrap();
                            elements.pop();
                        }
                    });
                }
            }
        };

        Task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        let width = self
            .row_width
            .lock()
            .unwrap()
            .map(Length::Fixed)
            .unwrap_or(Length::Shrink);
        let mode = match width {
            Length::Shrink => Length::Shrink,
            _ => Length::Fill,
        };

        MenuColumn::with_children(self.items.iter().enumerate().map(|(idx, item)| {
            match item {
                Item::Separator => divider::horizontal::light()
                    .class(theme::Rule::Default)
                    .into(),
                Item::Submenu { title, .. } => Row::with_children(vec![
                    space::horizontal().width(16).into(),
                    text::body(title).width(mode).into(),
                    from_name("go-next-symbolic")
                        .size(16)
                        .prefer_svg(true)
                        .icon()
                        .into(),
                ])
                .spacing(8)
                .width(width)
                .padding([8, 16])
                .align_y(Alignment::Center)
                .apply(|row| item::SubmenuItem::new(row, idx))
                .style(theme::Button::MenuItem)
                .into(),
                Item::Entry {
                    title,
                    shortcut,
                    toggled,
                    disabled,
                    ..
                } => {
                    let mut components = vec![
                        if *toggled {
                            from_name("object-select-symbolic")
                                .size(16)
                                .prefer_svg(true)
                                .icon()
                                .class(theme::Svg::custom(|theme| iced_widget::svg::Style {
                                    color: Some(theme.cosmic().accent.base.into()),
                                }))
                                .into()
                        } else {
                            space::horizontal().width(16).into()
                        },
                        text::body(title)
                            .width(mode)
                            .class(if *disabled {
                                theme::Text::Custom(|theme| {
                                    let mut color = theme.cosmic().background(false).component.on;
                                    color.alpha *= 0.5;
                                    TextStyle {
                                        color: Some(color.into()),
                                        ..Default::default()
                                    }
                                })
                            } else {
                                theme::Text::Default
                            })
                            .into(),
                        space::horizontal().width(16).into(),
                    ];
                    if let Some(shortcut) = shortcut.as_ref() {
                        components.push(
                            text::body(shortcut)
                                .align_x(Horizontal::Right)
                                .width(Length::Shrink)
                                .class(theme::Text::Custom(|theme| {
                                    let mut color = theme.cosmic().background(false).component.on;
                                    color.alpha *= 0.75;
                                    TextStyle {
                                        color: Some(color.into()),
                                        ..Default::default()
                                    }
                                }))
                                .into(),
                        );
                    }

                    Row::with_children(components)
                        .spacing(8)
                        .width(mode)
                        .align_y(Alignment::Center)
                        .apply(button::custom)
                        .width(width)
                        .padding([8, 16])
                        .on_press_maybe((!disabled).then_some(Message::ItemPressed(idx)))
                        .class(theme::Button::MenuItem)
                        .into()
                }
            }
        }))
        .width(Length::Shrink)
        .apply(iced_widget::container)
        .padding(1)
        .class(theme::Container::custom(|theme| {
            let cosmic = theme.cosmic();
            let component = &cosmic.background(theme.cosmic().frosted_windows).component;
            iced_widget::container::Style {
                snap: true,
                icon_color: Some(cosmic.accent.base.into()),
                text_color: Some(component.on.into()),
                background: Some(Background::Color(component.base.into())),
                border: Border {
                    radius: cosmic.radius_s().into(),
                    width: 1.0,
                    color: component.divider.into(),
                },
                shadow: Default::default(),
            }
        }))
        .width(Length::Shrink)
        .into()
    }
}

pub struct Element {
    iced: IcedElement<ContextMenu>,
    position: Point<i32, Global>,
    pointer_entered: bool,
    touch_entered: Option<TouchSlot>,
}

>>>>>>> upstream/master
pub struct MenuGrab {
    start_data: GrabStartData,
    seat: Seat<State>,
    /// Desktop-shell surface focus target for pointer event routing.
    shell_focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
}

impl PointerGrab<State> for MenuGrab {
    fn motion(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
        event: &PointerMotionEvent,
    ) {
<<<<<<< HEAD
        // Forward pointer events to desktop-shell so it can handle menu interaction.
        handle.motion(state, self.shell_focus.clone(), event);
=======
        {
            let mut guard = self.elements.lock().unwrap();
            let elements = &mut *guard;
            let event_location = if let Some(output) = self.screen_space_relative.as_ref() {
                if state.common.shell.read().zoom_state().is_some() {
                    event
                        .location
                        .as_global()
                        .to_zoomed(output)
                        .to_global(output)
                        .as_logical()
                } else {
                    event.location
                }
            } else {
                event.location
            };

            if let Some(i) = elements.iter().position(|elem| {
                let mut bbox = elem.iced.bbox();
                bbox.loc = elem.position.as_logical();

                bbox.contains(event_location.to_i32_round())
            }) {
                let element = &mut elements[i];

                let new_event = PointerMotionEvent {
                    location: event_location - element.position.as_logical().to_f64(),
                    serial: event.serial,
                    time: event.time,
                };
                if !element.pointer_entered {
                    PointerTarget::enter(&element.iced, &self.seat, state, &new_event);
                    element.pointer_entered = true;
                } else {
                    PointerTarget::motion(&element.iced, &self.seat, state, &new_event);
                }
            } else {
                elements
                    .iter_mut()
                    .filter(|element| element.pointer_entered)
                    .skip(1)
                    .for_each(|element| {
                        PointerTarget::leave(
                            &element.iced,
                            &self.seat,
                            state,
                            event.serial,
                            event.time,
                        );
                        element.pointer_entered = false;
                    })
            }
        }
        handle.motion(state, None, event);
>>>>>>> upstream/master
    }

    fn relative_motion(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        // While the grab is active, no client has pointer focus.
        handle.relative_motion(state, None, event);
    }

    fn button(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &ButtonEvent,
    ) {
        // If no shell client is connected, the grab has no way to be released
        // via protocol (no activate/dismiss will arrive). Release immediately
        // on any button press to prevent the pointer from getting stuck.
        if self.shell_focus.is_none() {
            handle.unset_grab(self, state, event.serial, event.time, true);
            return;
        }
        // Forward button events to desktop-shell.
        // The grab is released when desktop-shell sends activate or dismiss.
        handle.button(state, event);
    }

    fn axis(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        details: AxisFrame,
    ) {
        handle.axis(state, details);
    }

    fn frame(&mut self, data: &mut State, handle: &mut PointerInnerHandle<'_, State>) {
        handle.frame(data)
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event)
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event)
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event)
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event)
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event)
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event)
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event)
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event)
    }

    fn start_data(&self) -> &PointerGrabStartData<State> {
        match &self.start_data {
            GrabStartData::Pointer(start_data) => start_data,
            _ => unreachable!(),
        }
    }

    fn unset(&mut self, _data: &mut State) {}
}

impl TouchGrab<State> for MenuGrab {
    fn down(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        _focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
<<<<<<< HEAD
        event: &smithay::input::touch::DownEvent,
        seq: smithay::utils::Serial,
    ) {
        handle.down(data, None, event, seq);
    }

    fn up(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        _event: &smithay::input::touch::UpEvent,
        _seq: smithay::utils::Serial,
    ) {
=======
        event: &DownEvent,
    ) {
        {
            let mut guard = self.elements.lock().unwrap();
            let elements = &mut *guard;
            let event_location = if let Some(output) = self.screen_space_relative.as_ref() {
                if data.common.shell.read().zoom_state().is_some() {
                    event
                        .location
                        .as_global()
                        .to_zoomed(output)
                        .to_global(output)
                        .as_logical()
                } else {
                    event.location
                }
            } else {
                event.location
            };

            if let Some(i) = elements.iter().position(|elem| {
                let mut bbox = elem.iced.bbox();
                bbox.loc = elem.position.as_logical();

                bbox.contains(event_location.to_i32_round())
            }) {
                let element = &mut elements[i];

                let new_event = DownEvent {
                    slot: event.slot,
                    location: event_location - element.position.as_logical().to_f64(),
                    serial: event.serial,
                    time: event.time,
                };
                if element.touch_entered.is_none() {
                    TouchTarget::down(&element.iced, &self.seat, data, &new_event);
                    element.touch_entered = Some(event.slot);
                }
            }
        }
        handle.down(data, None, event);
    }

    fn up(&mut self, data: &mut State, handle: &mut TouchInnerHandle<'_, State>, event: &UpEvent) {
        {
            let elements = self.elements.lock().unwrap();
            for element in elements.iter().filter(|elem| {
                elem.touch_entered
                    .as_ref()
                    .is_some_and(|slot| *slot == event.slot)
            }) {
                TouchTarget::up(&element.iced, &self.seat, data, event);
            }
        }
>>>>>>> upstream/master
        handle.unset_grab(self, data);
    }

    fn motion(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        _focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
<<<<<<< HEAD
        event: &smithay::input::touch::MotionEvent,
        seq: smithay::utils::Serial,
    ) {
        handle.motion(data, None, event, seq);
    }

    fn frame(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        seq: smithay::utils::Serial,
    ) {
        handle.frame(data, seq);
    }

    fn cancel(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        seq: smithay::utils::Serial,
    ) {
        handle.cancel(data, seq);
=======
        event: &TouchMotionEvent,
    ) {
        {
            let elements = self.elements.lock().unwrap();
            for element in elements.iter().filter(|elem| {
                elem.touch_entered
                    .as_ref()
                    .is_some_and(|slot| *slot == event.slot)
            }) {
                TouchTarget::motion(&element.iced, &self.seat, data, event);
            }
        }
        handle.motion(data, None, event);
    }

    fn frame(&mut self, data: &mut State, handle: &mut TouchInnerHandle<'_, State>) {
        handle.frame(data);
    }

    fn cancel(&mut self, data: &mut State, handle: &mut TouchInnerHandle<'_, State>) {
        {
            let mut elements = self.elements.lock().unwrap();
            for element in elements.iter_mut() {
                let _ = element.touch_entered.take();
            }
        }
        handle.cancel(data);
>>>>>>> upstream/master
    }

    fn shape(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        event: &smithay::input::touch::ShapeEvent,
<<<<<<< HEAD
        seq: smithay::utils::Serial,
=======
>>>>>>> upstream/master
    ) {
        handle.shape(data, event);
    }

    fn orientation(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        event: &smithay::input::touch::OrientationEvent,
<<<<<<< HEAD
        seq: smithay::utils::Serial,
=======
>>>>>>> upstream/master
    ) {
        handle.orientation(data, event);
    }

    fn start_data(&self) -> &TouchGrabStartData<State> {
        match &self.start_data {
            GrabStartData::Touch(start_data) => start_data,
            _ => unreachable!(),
        }
    }

    fn unset(&mut self, _data: &mut State) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuAlignment {
    pub x: AxisAlignment,
    pub y: AxisAlignment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisAlignment {
    Corner(u32),
    Centered,
    PreferCentered,
}

impl MenuAlignment {
    pub const CORNER: Self = MenuAlignment {
        x: AxisAlignment::Corner(0),
        y: AxisAlignment::Corner(0),
    };
    pub const PREFER_CENTERED: Self = MenuAlignment {
        x: AxisAlignment::PreferCentered,
        y: AxisAlignment::PreferCentered,
    };
    pub const CENTERED: Self = MenuAlignment {
        x: AxisAlignment::Centered,
        y: AxisAlignment::Centered,
    };
    pub const HORIZONTALLY_CENTERED: Self = MenuAlignment {
        x: AxisAlignment::Centered,
        y: AxisAlignment::Corner(0),
    };
    pub const VERTICALLY_CENTERED: Self = MenuAlignment {
        x: AxisAlignment::Corner(0),
        y: AxisAlignment::Centered,
    };

    pub fn horizontally_centered(offset: u32, fixed: bool) -> MenuAlignment {
        MenuAlignment {
            x: if fixed {
                AxisAlignment::Centered
            } else {
                AxisAlignment::PreferCentered
            },
            y: AxisAlignment::Corner(offset),
        }
    }

    pub fn vertically_centered(offset: u32, fixed: bool) -> MenuAlignment {
        MenuAlignment {
            x: AxisAlignment::Corner(offset),
            y: if fixed {
                AxisAlignment::Centered
            } else {
                AxisAlignment::PreferCentered
            },
        }
    }

    #[allow(dead_code)]
    fn rectangles(
        &self,
        position: Point<i32, Global>,
        size: Size<i32, Global>,
    ) -> Vec<smithay::utils::Rectangle<i32, Global>> {
        fn for_alignment(
            position: Point<i32, Global>,
            size: Size<i32, Global>,
            x: AxisAlignment,
            y: AxisAlignment,
        ) -> Vec<smithay::utils::Rectangle<i32, Global>> {
            match (x, y) {
                (AxisAlignment::Corner(x_offset), AxisAlignment::Corner(y_offset)) => {
                    let offset = Point::from((x_offset as i32, y_offset as i32));
                    vec![
                        smithay::utils::Rectangle::new(position + offset, size), // normal
                        smithay::utils::Rectangle::new(
                            position - Point::from((size.w, 0))
                                + Point::from((-(x_offset as i32), y_offset as i32)),
                            size,
                        ), // flipped left
                        smithay::utils::Rectangle::new(
                            position
                                - Point::from((0, size.h))
                                - Point::from((x_offset as i32, -(y_offset as i32))),
                            size,
                        ), // flipped up
                        smithay::utils::Rectangle::new(position - size.to_point() - offset, size), // flipped left & up
                    ]
                }
                (AxisAlignment::Centered, AxisAlignment::Corner(offset)) => {
                    let x = position.x - ((size.w as f64 / 2.).round() as i32);
                    vec![
                        smithay::utils::Rectangle::new(
                            Point::from((x, position.y + offset as i32)),
                            size,
                        ), // below
                        smithay::utils::Rectangle::new(
                            Point::from((x, position.y - size.h - offset as i32)),
                            size,
                        ), // above
                    ]
                }
                (AxisAlignment::Corner(offset), AxisAlignment::Centered) => {
                    let y = position.y - ((size.h as f64 / 2.).round() as i32);
                    vec![
                        smithay::utils::Rectangle::new(
                            Point::from((position.x + offset as i32, y)),
                            size,
                        ), // left
                        smithay::utils::Rectangle::new(
                            Point::from((position.x - size.w - offset as i32, y)),
                            size,
                        ), // right
                    ]
                }
                (AxisAlignment::Centered, AxisAlignment::Centered) => {
                    vec![smithay::utils::Rectangle::new(
                        position - size.to_f64().downscale(2.).to_i32_round().to_point(),
                        size,
                    )]
                }
                (AxisAlignment::PreferCentered, AxisAlignment::PreferCentered) => for_alignment(
                    position,
                    size,
                    AxisAlignment::Centered,
                    AxisAlignment::Centered,
                )
                .into_iter()
                .chain(for_alignment(
                    position,
                    size,
                    AxisAlignment::Centered,
                    AxisAlignment::Corner(0),
                ))
                .chain(for_alignment(
                    position,
                    size,
                    AxisAlignment::Corner(0),
                    AxisAlignment::Centered,
                ))
                .chain(for_alignment(
                    position,
                    size,
                    AxisAlignment::Corner(0),
                    AxisAlignment::Corner(0),
                ))
                .collect(),
                (AxisAlignment::PreferCentered, y) => {
                    for_alignment(position, size, AxisAlignment::Centered, y)
                        .into_iter()
                        .chain(for_alignment(position, size, AxisAlignment::Corner(0), y))
                        .collect()
                }
                (x, AxisAlignment::PreferCentered) => {
                    for_alignment(position, size, x, AxisAlignment::Centered)
                        .into_iter()
                        .chain(for_alignment(position, size, x, AxisAlignment::Corner(0)))
                        .collect()
                }
            }
        }

        for_alignment(position, size, self.x, self.y)
    }
}

impl MenuGrab {
    /// Create a new `MenuGrab`.
    ///
    /// `menu_id` identifies the overlay protocol menu. Rendering and interaction
    /// are handled by desktop-shell; pointer events are forwarded to `shell_focus`.
    pub fn new(
        start_data: GrabStartData,
        seat: &Seat<State>,
        _items: impl Iterator<Item = Item>,
        _position: Point<i32, Global>,
        _alignment: MenuAlignment,
        screen_space_relative: Option<f64>,
        _handle: LoopHandle<'static, crate::state::State>,
        menu_id: Option<u32>,
        shell_focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
    ) -> MenuGrab {
        let output = seat.active_output();
        let screen_space_output = screen_space_relative.is_some().then_some(output.clone());

        let grab_state = MenuGrabState {
            screen_space_relative: screen_space_output,
            menu_id,
        };
        *seat
            .user_data()
            .get::<SeatMenuGrabState>()
            .unwrap()
            .lock()
            .unwrap() = Some(grab_state);

        MenuGrab {
            start_data,
            seat: seat.clone(),
            shell_focus,
        }
    }

    /// Whether this grab was initiated by a touch event.
    pub fn is_touch_grab(&self) -> bool {
        match self.start_data {
            GrabStartData::Touch(_) => true,
            GrabStartData::Pointer(_) => false,
        }
    }
}

impl Drop for MenuGrab {
    fn drop(&mut self) {
        self.seat
            .user_data()
            .get::<SeatMenuGrabState>()
            .unwrap()
            .lock()
            .unwrap()
            .take();
        // NOTE: `context_menu_closed` (compositor-initiated close) is not sent
        // from Drop because `LoopHandle` is not `Send` and cannot be stored here.
        // Explicit compositor-side teardown (e.g. window destroyed while menu is
        // open) must call `ShellOverlayState::close_context_menu` at the call site.
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    /// Labels collected during the same recursive walk the real Wayland
    /// serializer performs (see `ShellOverlayState::send_items_recursive`).
    /// A test double that only records the kind + label at each counter
    /// position — enough to compare index-by-index with `flatten_callbacks`.
    fn serializer_walk(items: &[Item]) -> Vec<(u32, String)> {
        fn recurse(items: &[Item], counter: &mut u32, out: &mut Vec<(u32, String)>) {
            for item in items {
                let my_index = *counter;
                *counter += 1;
                match item {
                    Item::Separator => out.push((my_index, "Separator".into())),
                    Item::Entry { title, .. } => {
                        out.push((my_index, format!("Entry:{title}")))
                    }
                    Item::Submenu { title, items: children } => {
                        out.push((my_index, format!("Submenu:{title}")));
                        recurse(children, counter, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        let mut counter = 0;
        recurse(items, &mut counter, &mut out);
        out
    }

    /// Same projection applied to the flattened callback Vec.
    fn flatten_labels(items: &[Item]) -> Vec<(u32, String)> {
        flatten_callbacks(items)
            .into_iter()
            .enumerate()
            .map(|(i, it)| {
                let label = match &it {
                    Item::Separator => "Separator".to_string(),
                    Item::Entry { title, .. } => format!("Entry:{title}"),
                    Item::Submenu { title, .. } => format!("Submenu:{title}"),
                };
                (i as u32, label)
            })
            .collect()
    }

    fn entry(title: &str) -> Item {
        Item::new(title.to_string(), |_| {})
    }

    #[test]
    fn flat_menu_indices_match_serializer() {
        let items = vec![
            entry("Minimize"),
            Item::Separator,
            entry("Maximize"),
            entry("Close"),
        ];
        assert_eq!(serializer_walk(&items), flatten_labels(&items));
    }

    #[test]
    fn submenu_indices_match_serializer() {
        // Mirrors the real window menu shape: flat entries, a
        // "Move to Workspace" submenu with numeric children, then
        // more flat entries after.
        let items = vec![
            entry("Minimize"),
            entry("Maximize"),
            Item::Separator,
            Item::new_submenu(
                "Move to Workspace",
                vec![entry("1"), entry("2"), entry("3")],
            ),
            Item::Separator,
            entry("Sticky"),
            entry("Close"),
        ];
        let serializer = serializer_walk(&items);
        let flatten = flatten_labels(&items);
        assert_eq!(
            serializer, flatten,
            "serializer DFS indices must match flatten_callbacks order"
        );

        // Explicit expected sequence so a regression is obvious.
        assert_eq!(
            serializer,
            vec![
                (0, "Entry:Minimize".into()),
                (1, "Entry:Maximize".into()),
                (2, "Separator".into()),
                (3, "Submenu:Move to Workspace".into()),
                (4, "Entry:1".into()),
                (5, "Entry:2".into()),
                (6, "Entry:3".into()),
                (7, "Separator".into()),
                (8, "Entry:Sticky".into()),
                (9, "Entry:Close".into()),
            ],
        );
    }

    #[test]
    fn nested_submenus_match_serializer() {
        let items = vec![
            entry("A"),
            Item::new_submenu(
                "Outer",
                vec![
                    entry("B"),
                    Item::new_submenu("Inner", vec![entry("C"), entry("D")]),
                    entry("E"),
                ],
            ),
            entry("F"),
        ];
        assert_eq!(serializer_walk(&items), flatten_labels(&items));
    }
}

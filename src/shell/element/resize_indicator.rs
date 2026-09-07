<<<<<<< HEAD
use std::{
    fmt,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
};

use calloop::LoopHandle;
use cosmic_settings_config::shortcuts::action::ResizeDirection;
use smithay::{
    backend::renderer::{ImportMem, Renderer, element::memory::MemoryRenderBufferRenderElement},
    input::{
        Seat,
        pointer::{
            AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent,
            GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
            GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent, MotionEvent,
            PointerTarget, RelativeMotionEvent,
        },
        touch::{DownEvent, MotionEvent as TouchMotionEvent, OrientationEvent, ShapeEvent, TouchTarget, UpEvent},
    },
    output::Output,
    utils::{IsAlive, Logical, Physical, Point, Rectangle, Scale, Serial, Size},
};

use smithay::backend::renderer::element::AsRenderElements;
use smithay::desktop::space::SpaceElement;

use crate::shell::grabs::ResizeEdge;

// SAFETY: LoopHandle contains Rc which is !Send, but ResizeIndicator is never
// moved across threads or dropped on a foreign thread. Same safety contract as
// the former IcedElement wrapper.
unsafe impl Send for ResizeIndicator {}
unsafe impl Sync for ResizeIndicator {}

/// Lightweight indicator element for resize operations.
///
/// Previously backed by `IcedElement`; now a standalone struct that stores
/// resize metadata and exposes the same public API surface. Rendering is
/// handled by `desktop-shell` via `arlen-shell-overlay`, so all
/// `render_elements` calls return an empty `Vec`.
#[derive(Clone)]
pub struct ResizeIndicator {
    inner: Arc<Mutex<ResizeIndicatorInternal>>,
    handle: LoopHandle<'static, crate::state::State>,
}

impl fmt::Debug for ResizeIndicator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResizeIndicator").finish_non_exhaustive()
    }
}

impl PartialEq for ResizeIndicator {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for ResizeIndicator {}

impl Hash for ResizeIndicator {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (Arc::as_ptr(&self.inner) as usize).hash(state)
    }
}

/// Internal data for [`ResizeIndicator`].
pub struct ResizeIndicatorInternal {
    /// Current active resize edges.
    pub edges: Mutex<ResizeEdge>,
    /// Resize direction (outwards or inwards).
    pub direction: ResizeDirection,
    /// Formatted shortcut string for the outwards action.
    pub shortcut1: String,
    /// Formatted shortcut string for the inwards action.
    pub shortcut2: String,
}

/// Creates a new [`ResizeIndicator`] pre-populated with keybinding info.
pub fn resize_indicator(
    direction: ResizeDirection,
    config: &crate::config::Config,
    evlh: LoopHandle<'static, crate::state::State>,
) -> ResizeIndicator {
    use cosmic_settings_config::shortcuts::action::Action;
    ResizeIndicator {
        inner: Arc::new(Mutex::new(ResizeIndicatorInternal {
            edges: Mutex::new(ResizeEdge::all()),
            direction,
            shortcut1: config
                .shortcuts
                .iter()
                .find_map(|(pattern, action)| {
                    (*action == Action::Resizing(ResizeDirection::Outwards))
                        .then_some(format!("{}: ", pattern.to_string()))
                })
                .unwrap_or_else(|| crate::fl!("unknown-keybinding")),
            shortcut2: config
                .shortcuts
                .iter()
                .find_map(|(pattern, action)| {
                    (*action == Action::Resizing(ResizeDirection::Inwards))
                        .then_some(format!("{}: ", pattern.to_string()))
                })
                .unwrap_or_else(|| crate::fl!("unknown-keybinding")),
        })),
        handle: evlh,
=======
use std::sync::{Arc, Mutex};

use crate::{
    backend::render::element::AsGlowRenderer,
    config::Config,
    fl,
    shell::grabs::ResizeEdge,
    utils::iced::{IcedElement, IcedRenderElement, Program},
};

use calloop::LoopHandle;
use cosmic::{
    Apply,
    iced::{
        Alignment,
        core::{Background, Border, Color, Length},
        widget::{container, row, space},
    },
    theme,
    widget::{icon::from_name, text},
};
use cosmic_settings_config::shortcuts::action::{Action, ResizeDirection};
use smithay::{
    backend::renderer::ImportMem,
    desktop::space::SpaceElement,
    output::Output,
    utils::{Logical, Physical, Point, Rectangle, Scale, Size},
};

#[derive(Debug, Clone)]
pub struct ResizeIndicator {
    edges: ResizeEdge,
    direction: Arc<Mutex<ResizeDirection>>,
    size: Size<i32, Logical>,

    center_elem: IcedElement<ResizeIndicatorInternal>,
    left_elem: IcedElement<ResizeIndicatorArrow>,
    top_elem: IcedElement<ResizeIndicatorArrow>,
    right_elem: IcedElement<ResizeIndicatorArrow>,
    down_elem: IcedElement<ResizeIndicatorArrow>,
}

const ARROW_SIZE: i32 = 36;

impl ResizeIndicator {
    pub fn new(
        direction: ResizeDirection,
        config: &Config,
        evlh: LoopHandle<'static, crate::state::State>,
        mut theme: cosmic::Theme,
    ) -> ResizeIndicator {
        theme.transparent = theme.cosmic().frosted_system_interface;
        let direction = Arc::new(Mutex::new(direction));

        ResizeIndicator {
            edges: ResizeEdge::all(),
            direction: direction.clone(),
            size: Size::default(),
            center_elem: IcedElement::new(
                ResizeIndicatorInternal {
                    shortcut1: config
                        .shortcuts
                        .iter()
                        .find_map(|(pattern, action)| {
                            (*action == Action::Resizing(ResizeDirection::Outwards))
                                .then_some(pattern)
                        })
                        .map(|pattern| format!("{}: ", pattern.to_string()))
                        .unwrap_or_else(|| crate::fl!("unknown-keybinding")),
                    shortcut2: config
                        .shortcuts
                        .iter()
                        .find_map(|(pattern, action)| {
                            (*action == Action::Resizing(ResizeDirection::Inwards))
                                .then_some(pattern)
                        })
                        .map(|pattern| format!("{}: ", pattern.to_string()))
                        .unwrap_or_else(|| crate::fl!("unknown-keybinding")),
                },
                Size::from((1, 1)),
                evlh.clone(),
                theme.clone(),
            ),
            left_elem: IcedElement::new(
                ResizeIndicatorArrow {
                    direction: direction.clone(),
                    icon_outwards: "go-previous-symbolic",
                    icon_inwards: "go-next-symbolic",
                },
                Size::from((ARROW_SIZE, ARROW_SIZE)),
                evlh.clone(),
                theme.clone(),
            ),
            top_elem: IcedElement::new(
                ResizeIndicatorArrow {
                    direction: direction.clone(),
                    icon_outwards: "go-up-symbolic",
                    icon_inwards: "go-down-symbolic",
                },
                Size::from((ARROW_SIZE, ARROW_SIZE)),
                evlh.clone(),
                theme.clone(),
            ),
            right_elem: IcedElement::new(
                ResizeIndicatorArrow {
                    direction: direction.clone(),
                    icon_outwards: "go-next-symbolic",
                    icon_inwards: "go-previous-symbolic",
                },
                Size::from((ARROW_SIZE, ARROW_SIZE)),
                evlh.clone(),
                theme.clone(),
            ),
            down_elem: IcedElement::new(
                ResizeIndicatorArrow {
                    direction,
                    icon_outwards: "go-down-symbolic",
                    icon_inwards: "go-up-symbolic",
                },
                Size::from((ARROW_SIZE, ARROW_SIZE)),
                evlh.clone(),
                theme.clone(),
            ),
        }
    }

    pub fn resize(&mut self, size: Size<i32, Logical>) {
        let minimum = self.center_elem.minimum_size();
        let new_size = Size::<i32, Logical>::new(size.w.min(minimum.w), size.h.min(minimum.h));
        self.center_elem.resize(new_size);
        self.size = size;
    }

    pub fn push_render_elements<R>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
        push_above: &mut dyn FnMut(IcedRenderElement<R>),
        mut push_below: Option<&mut dyn FnMut(IcedRenderElement<R>)>,
    ) where
        R: AsGlowRenderer + ImportMem,
        R::TextureId: Send + Clone + 'static,
    {
        let elem_size = self.center_elem.current_size();
        let center_location = Point::new(
            self.size.w.saturating_sub(elem_size.w) / 2,
            self.size.h.saturating_sub(elem_size.h) / 2,
        );
        let left_location = Point::new(0, self.size.h.saturating_sub(ARROW_SIZE) / 2);
        let top_location = Point::new(self.size.w.saturating_sub(ARROW_SIZE) / 2, 0);
        let right_location = Point::new(
            self.size.w.saturating_sub(ARROW_SIZE),
            self.size.h.saturating_sub(ARROW_SIZE) / 2,
        );
        let down_location = Point::new(
            self.size.w.saturating_sub(ARROW_SIZE) / 2,
            self.size.h.saturating_sub(ARROW_SIZE),
        );
        let radii = self
            .center_elem
            .with_theme(|theme| theme.cosmic().radius_s())
            .map(|x| x.round() as u8);

        self.center_elem.push_render_elements(
            renderer,
            location + center_location.to_physical_precise_round(scale),
            scale,
            alpha,
            radii,
            push_above,
            if let Some(push_below) = push_below.as_mut() {
                Some(&mut **push_below)
            } else {
                None
            },
        );
        let mut render = move |elem: &IcedElement<ResizeIndicatorArrow>,
                               loc: Point<i32, Logical>| {
            elem.push_render_elements(
                renderer,
                location + loc.to_physical_precise_round(scale),
                scale,
                alpha,
                radii,
                push_above,
                if let Some(push_below) = push_below.as_mut() {
                    Some(&mut **push_below)
                } else {
                    None
                },
            );
        };

        if self.edges.contains(ResizeEdge::LEFT) {
            render(&self.left_elem, left_location);
        }
        if self.edges.contains(ResizeEdge::TOP) {
            render(&self.top_elem, top_location);
        }
        if self.edges.contains(ResizeEdge::RIGHT) {
            render(&self.right_elem, right_location);
        }
        if self.edges.contains(ResizeEdge::BOTTOM) {
            render(&self.down_elem, down_location);
        }
    }

    pub fn set_edges(&mut self, edges: ResizeEdge) {
        self.edges = edges;
    }

    pub fn set_direction(&self, direction: ResizeDirection) {
        let mut dir_ref = self.direction.lock().unwrap();
        if *dir_ref != direction {
            *dir_ref = direction;
            self.left_elem.force_redraw();
            self.top_elem.force_redraw();
            self.right_elem.force_redraw();
            self.down_elem.force_redraw();
        }
    }

    pub fn output_enter(&self, output: &Output) {
        self.center_elem
            .output_enter(output, Rectangle::default() /*unused*/);
        self.left_elem
            .output_enter(output, Rectangle::default() /*unused*/);
        self.top_elem
            .output_enter(output, Rectangle::default() /*unused*/);
        self.right_elem
            .output_enter(output, Rectangle::default() /*unused*/);
        self.down_elem
            .output_enter(output, Rectangle::default() /*unused*/);
    }

    pub fn output_leave(&self, output: &Output) {
        self.center_elem.output_leave(output);
        self.left_elem.output_leave(output);
        self.top_elem.output_leave(output);
        self.right_elem.output_leave(output);
        self.down_elem.output_leave(output);
    }
}

pub struct ResizeIndicatorInternal {
    shortcut1: String,
    shortcut2: String,
}

impl Program for ResizeIndicatorInternal {
    type Message = ();

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        row(vec![
            text::heading(&self.shortcut1).into(),
            text::body(fl!("grow-window")).into(),
            space::horizontal().width(40).into(),
            text::heading(&self.shortcut2).into(),
            text::body(fl!("shrink-window")).into(),
        ])
        .apply(container)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .padding(16)
        .apply(container)
        .class(theme::Container::custom(|theme| {
            let mut background = theme.cosmic().accent_color();
            if theme.transparent {
                background.alpha = theme
                    .cosmic()
                    .alpha_map
                    .blurred_alpha(theme.cosmic().frosted);
            }

            container::Style {
                snap: true,
                icon_color: Some(Color::from(theme.cosmic().accent.on)),
                text_color: Some(Color::from(theme.cosmic().accent.on)),
                background: Some(Background::Color(background.into())),
                border: Border {
                    radius: theme.cosmic().radius_s().into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                shadow: Default::default(),
            }
        }))
        .width(Length::Shrink)
        .height(Length::Shrink)
        .into()
    }
}

pub struct ResizeIndicatorArrow {
    direction: Arc<Mutex<ResizeDirection>>,
    icon_outwards: &'static str,
    icon_inwards: &'static str,
}

impl Program for ResizeIndicatorArrow {
    type Message = ();

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        from_name(
            if *self.direction.lock().unwrap() == ResizeDirection::Outwards {
                self.icon_outwards
            } else {
                self.icon_inwards
            },
        )
        .size(20)
        .prefer_svg(true)
        .apply(container)
        .padding(8)
        .class(theme::Container::custom(|theme| {
            let mut background = theme.cosmic().accent_color();
            if theme.transparent {
                background.alpha = theme
                    .cosmic()
                    .alpha_map
                    .blurred_alpha(theme.cosmic().frosted);
            }

            container::Style {
                snap: true,
                icon_color: Some(Color::from(theme.cosmic().accent.on)),
                text_color: Some(Color::from(theme.cosmic().accent.on)),
                background: Some(Background::Color(background.into())),
                border: Border {
                    radius: theme.cosmic().radius_s().into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                shadow: Default::default(),
            }
        }))
        .width(Length::Shrink)
        .into()
>>>>>>> upstream/master
    }
}

impl ResizeIndicator {
    /// Runs a closure against the internal state, matching the former
    /// `IcedElement::with_program` API.
    pub fn with_program<R>(&self, f: impl FnOnce(&ResizeIndicatorInternal) -> R) -> R {
        let internal = self.inner.lock().unwrap();
        f(&internal)
    }

    /// Returns the calloop `LoopHandle` stored at construction time.
    pub fn loop_handle(&self) -> LoopHandle<'static, crate::state::State> {
        self.handle.clone()
    }

    /// No-op -- previously triggered an Iced re-render.
    pub fn force_update(&self) {}

    /// No-op -- size is unused since desktop-shell handles rendering.
    pub fn resize(&self, _size: Size<i32, Logical>) {}

    /// No-op -- output tracking is unused.
    pub fn output_enter(&self, _output: &Output, _overlap: Rectangle<i32, Logical>) {}

    /// No-op -- output tracking is unused.
    pub fn output_leave(&self, _output: &Output) {}

    /// Returns an empty `Size` -- the indicator has no compositor-side geometry.
    pub fn current_size(&self) -> Size<i32, Logical> {
        Size::default()
    }
}

impl<R> AsRenderElements<R> for ResizeIndicator
where
    R: Renderer + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    type RenderElement = MemoryRenderBufferRenderElement<R>;

    fn render_elements<C: From<Self::RenderElement>>(
        &self,
        _renderer: &mut R,
        _location: Point<i32, Physical>,
        _scale: Scale<f64>,
        _alpha: f32,
    ) -> Vec<C> {
        Vec::new()
    }
}

impl IsAlive for ResizeIndicator {
    fn alive(&self) -> bool {
        true
    }
}

impl SpaceElement for ResizeIndicator {
    fn bbox(&self) -> Rectangle<i32, Logical> {
        Rectangle::default()
    }

    fn is_in_input_region(&self, _point: &Point<f64, Logical>) -> bool {
        false
    }

    fn set_activate(&self, _activated: bool) {}

    fn output_enter(&self, _output: &Output, _overlap: Rectangle<i32, Logical>) {}

    fn output_leave(&self, _output: &Output) {}
}

impl PointerTarget<crate::state::State> for ResizeIndicator {
    fn enter(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &MotionEvent) {}
    fn motion(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &MotionEvent) {}
    fn relative_motion(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &RelativeMotionEvent) {}
    fn button(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &ButtonEvent) {}
    fn axis(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: AxisFrame) {}
    fn frame(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State) {}
    fn leave(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _serial: Serial, _time: u32) {}
    fn gesture_swipe_begin(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &GestureSwipeBeginEvent) {}
    fn gesture_swipe_update(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &GestureSwipeUpdateEvent) {}
    fn gesture_swipe_end(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &GestureSwipeEndEvent) {}
    fn gesture_pinch_begin(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &GesturePinchBeginEvent) {}
    fn gesture_pinch_update(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &GesturePinchUpdateEvent) {}
    fn gesture_pinch_end(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &GesturePinchEndEvent) {}
    fn gesture_hold_begin(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &GestureHoldBeginEvent) {}
    fn gesture_hold_end(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &GestureHoldEndEvent) {}
}

impl TouchTarget<crate::state::State> for ResizeIndicator {
    fn down(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &DownEvent, _seq: Serial) {}
    fn up(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &UpEvent, _seq: Serial) {}
    fn motion(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &TouchMotionEvent, _seq: Serial) {}
    fn frame(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _seq: Serial) {}
    fn cancel(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _seq: Serial) {}
    fn shape(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &ShapeEvent, _seq: Serial) {}
    fn orientation(&self, _seat: &Seat<crate::state::State>, _data: &mut crate::state::State, _event: &OrientationEvent, _seq: Serial) {}
}

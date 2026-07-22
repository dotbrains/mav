//! Div is the central, reusable element that most GPUI trees will be built from.
//! It functions as a container for other elements, and provides a number of
//! useful features for laying out and styling its children as well as binding
//! mouse events and action handlers. It is meant to be similar to the HTML `<div>`
//! element, but for GPUI.
//!
//! # Build your own div
//!
//! GPUI does not directly provide APIs for stateful, multi step events like `click`
//! and `drag`. We want GPUI users to be able to build their own abstractions for
//! their own needs. However, as a UI framework, we're also obliged to provide some
//! building blocks to make the process of building your own elements easier.
//! For this we have the [`Interactivity`] and the [`StyleRefinement`] structs, as well
//! as several associated traits. Together, these provide the full suite of Dom-like events
//! and Tailwind-like styling that you can use to build your own custom elements. Div is
//! constructed by combining these two systems into an all-in-one element.

use crate::PinchEvent;
use crate::{
    Action, AnyDrag, AnyElement, AnyView, App, Bounds, ClickEvent, DispatchPhase, Display, Element,
    ElementId, Entity, FocusHandle, Global, GlobalElementId, Hitbox, HitboxBehavior, HitboxId,
    InspectorElementId, IntoElement, IsZero, KeyContext, KeyDownEvent, KeyUpEvent, KeyboardButton,
    KeyboardClickEvent, LayoutId, ModifiersChangedEvent, MouseButton, MouseClickEvent,
    MouseDownEvent, MouseMoveEvent, MousePressureEvent, MouseUpEvent, Overflow, ParentElement,
    Pixels, Point, Render, ScrollWheelEvent, SharedString, Size, Style, StyleRefinement, Styled,
    TooltipId, Visibility, Window, WindowControlArea, point, px, size,
};
use collections::HashMap;
use gpui_util::ResultExt;
use refineable::Refineable;
use smallvec::SmallVec;
use stacksafe::{StackSafe, stacksafe};
use std::{
    any::{Any, TypeId},
    cell::RefCell,
    cmp::Ordering,
    fmt::Debug,
    marker::PhantomData,
    mem,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use super::ImageCacheProvider;

mod scroll;
mod stateful;
mod tooltip;
pub use scroll::{ScrollAnchor, ScrollHandle};
use stateful::GroupHitboxes;
pub use stateful::{ElementClickedState, ElementHoverState, InteractiveElementState, Stateful};
#[cfg(test)]
pub(crate) use tooltip::DEFAULT_TOOLTIP_SHOW_DELAY;
pub(crate) use tooltip::{
    ActiveTooltip, TooltipBuilder, register_tooltip_mouse_handlers, set_tooltip_on_window,
};
mod element;
mod interactive_element;
mod interactivity_api;
mod interactivity_click_api;
mod interactivity_debug_paint;
mod interactivity_drop_api;
mod interactivity_keyboard_api;
mod interactivity_keyboard_paint;
mod interactivity_layout;
mod interactivity_mouse_api;
mod interactivity_mouse_paint;
mod interactivity_paint;
mod interactivity_scroll_paint;
mod interactivity_state;
mod interactivity_style;
mod listener_types;
mod stateful_interactive_element;
mod traits;
mod types;

pub use element::{Div, DivFrameState, DivInspectorState, div};
pub use interactivity_state::Interactivity;
pub(crate) use listener_types::*;
pub use traits::{InteractiveElement, StatefulInteractiveElement};
pub use types::{DragMoveEvent, GroupStyle};

const DRAG_THRESHOLD: f64 = 2.;

#[cfg(test)]
mod tests;

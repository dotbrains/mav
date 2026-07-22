#[cfg(any(feature = "inspector", debug_assertions))]
use crate::Inspector;
use crate::{
    Action, AnyDrag, AnyElement, AnyImageCache, AnyTooltip, AnyView, App, AppContext, Asset,
    AsyncWindowContext, AvailableSpace, Background, BorderStyle, Bounds, BoxShadow, Capslock,
    Context, Corners, CursorHideMode, CursorStyle, Decorations, DevicePixels,
    DispatchActionListener, DispatchNodeId, DispatchTree, DisplayId, Edges, Effect, Entity,
    EntityId, EventEmitter, FileDropEvent, FontId, Global, GlobalElementId, GlyphId, GpuSpecs,
    Hsla, InputHandler, IsZero, KeyBinding, KeyContext, KeyDownEvent, KeyEvent, Keystroke,
    KeystrokeEvent, LayoutId, LineLayoutIndex, Modifiers, ModifiersChangedEvent, MonochromeSprite,
    MouseButton, MouseEvent, MouseMoveEvent, MouseUpEvent, Path, Pixels, PlatformAtlas,
    PlatformDisplay, PlatformInput, PlatformInputHandler, PlatformWindow, Point, PolychromeSprite,
    Priority, PromptButton, PromptLevel, Quad, Render, RenderGlyphParams, RenderImage,
    RenderImageParams, RenderSvgParams, Replay, ResizeEdge, SMOOTH_SVG_SCALE_FACTOR,
    SUBPIXEL_VARIANTS_X, SUBPIXEL_VARIANTS_Y, ScaledPixels, Scene, Shadow, SharedString, Size,
    StrikethroughStyle, Style, SubpixelSprite, SubscriberSet, Subscription, SystemWindowTab,
    SystemWindowTabController, TabStopMap, TaffyLayoutEngine, Task, TextRenderingMode, TextStyle,
    TextStyleRefinement, ThermalState, TransformationMatrix, Underline, UnderlineStyle,
    WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowControls, WindowDecorations,
    WindowOptions, WindowParams, WindowTextSystem, point, prelude::*, profiler, px, rems, size,
    transparent_black,
};

use anyhow::{Context as _, Result, anyhow};
use collections::{FxHashMap, FxHashSet};
#[cfg(target_os = "macos")]
use core_video::pixel_buffer::CVPixelBuffer;
use derive_more::{Deref, DerefMut};
use futures::FutureExt;
use futures::channel::oneshot;
use gpui_util::post_inc;
use gpui_util::{ResultExt, measure};
#[cfg(feature = "input-latency-histogram")]
use hdrhistogram::Histogram;
use itertools::FoldWhile::{Continue, Done};
use itertools::Itertools;
use raw_window_handle::{HandleError, HasDisplayHandle, HasWindowHandle};
use refineable::Refineable;
use scheduler::Instant;
use smallvec::SmallVec;
use std::{
    any::{Any, TypeId},
    borrow::Cow,
    cell::{Cell, RefCell},
    cmp,
    fmt::{Debug, Display},
    hash::{Hash, Hasher},
    marker::PhantomData,
    mem,
    ops::{DerefMut, Range},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::SeqCst},
    },
    time::Duration,
};
use uuid::Uuid;

pub(crate) mod a11y;
mod accessibility_actions;
mod action_dispatch;
mod actions_bindings;
mod basics;
mod construction;
#[path = "window/content_mask.rs"]
mod content_mask;
mod context;
mod default_bounds;
mod deferred_draws;
mod dispatch_event;
mod element_arena;
mod element_state;
mod focus;
mod frame;
mod frame_reuse;
mod geometry;
mod handles;
mod hitbox;
mod input;
mod input_handlers;
mod inspector;
mod interaction_state;
mod invalidation;
mod layout_render;
mod mouse_key_dispatch;
mod paint_media;
mod paint_primitives;
mod platform_controls;
mod primitives;
mod prompts;
mod state;
mod tooltip;

pub use a11y::A11ySubtreeBuilder;
pub use content_mask::ContentMask;
pub(crate) use element_arena::{ArenaClearNeeded, ElementArenaScope, with_element_arena};
pub(crate) use focus::FocusMap;
pub use focus::{
    DismissEvent, FocusHandle, FocusId, FocusOutEvent, Focusable, ManagedView, WeakFocusHandle,
};
pub(crate) use frame::{
    AnyObserver, AnyWindowFocusListener, CursorStyleRequest, DeferredDraw, Frame, FrameCallback,
    HitTest, PaintIndex, PrepaintStateIndex,
};
pub use handles::{AnyWindowHandle, WindowHandle, WindowId};
pub use hitbox::{Hitbox, HitboxBehavior, HitboxId, WindowControlArea};
pub(crate) use invalidation::WindowInvalidator;
pub use primitives::{ElementId, PaintQuad, fill, outline, quad};
pub use tooltip::TooltipId;
pub(crate) use tooltip::{TooltipBounds, TooltipRequest};

use self::a11y::A11y;
#[cfg(not(target_family = "wasm"))]
use self::a11y::ROOT_NODE_ID;
use self::default_bounds::default_bounds;
pub use self::input::DrawPhase;
#[cfg(feature = "input-latency-histogram")]
use self::input::{InputLatencySnapshot, InputLatencyTracker};
use self::input::{InputRateTracker, PendingInput};
pub use self::state::DispatchPhase;
use self::state::{ElementStateBox, InputModality, ModifierState};
use crate::util::{
    ceil_to_device_pixel, floor_to_device_pixel, round_half_toward_zero,
    round_half_toward_zero_f64, round_stroke_to_device_pixel, round_to_device_pixel,
};
pub use prompts::*;

/// Default window size used when no explicit size is provided.
pub const DEFAULT_WINDOW_SIZE: Size<Pixels> = size(px(1536.), px(1095.));

/// A 6:5 aspect ratio minimum window size to be used for functional,
/// additional-to-main-Mav windows, like the settings and rules library windows.
pub const DEFAULT_ADDITIONAL_WINDOW_SIZE: Size<Pixels> = Size {
    width: Pixels(900.),
    height: Pixels(750.),
};

/// Holds the state for a specific window.
pub struct Window {
    pub(crate) handle: AnyWindowHandle,
    pub(crate) invalidator: WindowInvalidator,
    pub(crate) removed: bool,
    pub(crate) platform_window: Box<dyn PlatformWindow>,
    display_id: Option<DisplayId>,
    sprite_atlas: Arc<dyn PlatformAtlas>,
    text_system: Arc<WindowTextSystem>,
    text_rendering_mode: Rc<Cell<TextRenderingMode>>,
    rem_size: Pixels,
    /// The stack of override values for the window's rem size.
    ///
    /// This is used by `with_rem_size` to allow rendering an element tree with
    /// a given rem size.
    rem_size_override_stack: SmallVec<[Pixels; 8]>,
    pub(crate) viewport_size: Size<Pixels>,
    layout_engine: Option<TaffyLayoutEngine>,
    pub(crate) root: Option<AnyView>,
    pub(crate) element_id_stack: SmallVec<[ElementId; 32]>,
    pub(crate) text_style_stack: Vec<TextStyleRefinement>,
    pub(crate) rendered_entity_stack: Vec<EntityId>,
    pub(crate) element_offset_stack: Vec<Point<Pixels>>,
    pub(crate) element_opacity: f32,
    pub(crate) content_mask_stack: Vec<ContentMask<Pixels>>,
    pub(crate) requested_autoscroll: Option<Bounds<Pixels>>,
    pub(crate) image_cache_stack: Vec<AnyImageCache>,
    pub(crate) rendered_frame: Frame,
    pub(crate) next_frame: Frame,
    next_hitbox_id: HitboxId,
    pub(crate) next_tooltip_id: TooltipId,
    pub(crate) tooltip_bounds: Option<TooltipBounds>,
    next_frame_callbacks: Rc<RefCell<Vec<FrameCallback>>>,
    pub(crate) dirty_views: FxHashSet<EntityId>,
    focus_listeners: SubscriberSet<(), AnyWindowFocusListener>,
    pub(crate) focus_lost_listeners: SubscriberSet<(), AnyObserver>,
    default_prevented: bool,
    mouse_position: Point<Pixels>,
    mouse_hit_test: HitTest,
    modifiers: Modifiers,
    capslock: Capslock,
    scale_factor: f32,
    pub(crate) bounds_observers: SubscriberSet<(), AnyObserver>,
    appearance: WindowAppearance,
    pub(crate) appearance_observers: SubscriberSet<(), AnyObserver>,
    pub(crate) button_layout_observers: SubscriberSet<(), AnyObserver>,
    active: Rc<Cell<bool>>,
    hovered: Rc<Cell<bool>>,
    pub(crate) needs_present: Rc<Cell<bool>>,
    /// Tracks recent input event timestamps to determine if input is arriving at a high rate.
    /// Used to selectively enable VRR optimization only when input rate exceeds 60fps.
    pub(crate) input_rate_tracker: Rc<RefCell<InputRateTracker>>,
    #[cfg(feature = "input-latency-histogram")]
    input_latency_tracker: InputLatencyTracker,
    last_input_modality: InputModality,
    pub(crate) refreshing: bool,
    pub(crate) activation_observers: SubscriberSet<(), AnyObserver>,
    pub(crate) focus: Option<FocusId>,
    focus_enabled: bool,
    /// Incremented every time focus moves. Used to invalidate a
    /// pending keyboard activation state when focus changes.
    pub(crate) focus_generation: u64,
    pending_input: Option<PendingInput>,
    pending_modifier: ModifierState,
    pub(crate) pending_input_observers: SubscriberSet<(), AnyObserver>,
    prompt: Option<RenderablePromptHandle>,
    pub(crate) client_inset: Option<Pixels>,
    /// The hitbox that has captured the pointer, if any.
    /// While captured, mouse events route to this hitbox regardless of hit testing.
    captured_hitbox: Option<HitboxId>,
    #[cfg(any(feature = "inspector", debug_assertions))]
    inspector: Option<Entity<Inspector>>,
    pub(crate) a11y: A11y,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(missing_docs)]
pub struct DispatchEventResult {
    pub propagate: bool,
    pub default_prevented: bool,
}

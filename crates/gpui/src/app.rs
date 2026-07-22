use scheduler::Instant;
use std::{
    any::{TypeId, type_name},
    cell::{BorrowMutError, Cell, Ref, RefCell, RefMut},
    marker::PhantomData,
    mem,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::{Arc, atomic::Ordering::SeqCst},
    time::Duration,
};

use anyhow::{Context as _, Result, anyhow};
use derive_more::{Deref, DerefMut};
use futures::{
    Future, FutureExt,
    channel::oneshot,
    future::{LocalBoxFuture, Shared},
};
use itertools::Itertools;
use parking_lot::RwLock;
use slotmap::SlotMap;

pub use async_context::*;
#[cfg(feature = "bench")]
pub use bench_context::{BenchAppContext, BenchReport, BenchWindowContext, bench_platform};
use collections::{FxHashMap, FxHashSet, HashMap, TypeIdHashMap, TypeIdHashSet, VecDeque};
pub use context::*;
pub use entity_map::*;
use gpui_util::{ResultExt, debug_panic};
#[cfg(any(test, feature = "test-support"))]
pub use headless_app_context::*;
use http_client::{HttpClient, Url};
use smallvec::SmallVec;
#[cfg(any(test, feature = "test-support"))]
pub use test_app::*;
#[cfg(any(test, feature = "test-support"))]
pub use test_context::*;
#[cfg(all(target_os = "macos", any(test, feature = "test-support")))]
pub use visual_test_context::*;

#[cfg(any(feature = "inspector", debug_assertions))]
use crate::InspectorElementRegistry;
use crate::{
    Action, ActionBuildError, ActionRegistry, Any, AnyView, AnyWindowHandle, AppContext, Arena,
    ArenaBox, Asset, AssetSource, BackgroundExecutor, Bounds, ClipboardItem, CursorStyle,
    DispatchPhase, DisplayId, EventEmitter, FocusHandle, FocusMap, ForegroundExecutor, Global,
    KeyBinding, KeyContext, Keymap, Keystroke, LayoutId, Menu, MenuItem, OwnedMenu,
    PathPromptOptions, Pixels, Platform, PlatformDisplay, PlatformKeyboardLayout,
    PlatformKeyboardMapper, Point, Priority, PromptBuilder, PromptButton, PromptHandle,
    PromptLevel, Render, RenderImage, RenderablePromptHandle, Reservation, ScreenCaptureSource,
    SharedString, SubscriberSet, Subscription, SvgRenderer, Task, TextRenderingMode, TextSystem,
    ThermalState, Window, WindowAppearance, WindowButtonLayout, WindowHandle, WindowId,
    WindowInvalidator,
    colors::{Colors, GlobalColors},
    hash, init_app_menus,
};

mod async_context;
#[cfg(feature = "bench")]
mod bench_context;
mod context;
mod entity_map;
#[cfg(any(test, feature = "test-support"))]
mod headless_app_context;
#[cfg(any(test, feature = "test-support"))]
mod test_app;
#[cfg(any(test, feature = "test-support"))]
mod test_context;
#[cfg(all(target_os = "macos", any(test, feature = "test-support")))]
mod visual_test_context;

/// The duration for which futures returned from [Context::on_app_quit] can run before the application fully quits.
mod actions;
mod app_cell;
mod app_context_impl;
mod application;
mod async_runtime;
mod construction;
mod drag_assets;
mod effect;
mod effects;
mod global_lease;
mod globals;
mod mode;
mod observers;
mod platform_api;
mod state;
mod support;
mod system_window_tabs;
#[cfg(test)]
mod tests;
mod types;
mod windows;

/// The duration for which futures returned from [Context::on_app_quit] can run before the application fully quits.
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(200);

pub use app_cell::*;
pub use application::*;
pub(crate) use effect::*;
pub(crate) use global_lease::*;
pub(crate) use mode::*;
pub use state::*;
pub use support::*;
pub use system_window_tabs::*;
pub(crate) use types::*;

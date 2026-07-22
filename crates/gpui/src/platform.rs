mod app_menu;
#[cfg(any(test, feature = "bench"))]
mod bench_dispatcher;
#[cfg(test)]
mod bench_dispatcher_tests;
mod keyboard;
mod keystroke;
#[cfg(all(target_os = "linux", feature = "wayland"))]
#[expect(missing_docs)]
pub mod layer_shell;

#[cfg(any(test, feature = "test-support"))]
mod test;

#[cfg(all(target_os = "macos", any(test, feature = "test-support")))]
mod visual_test;

#[cfg(all(
    feature = "screen-capture",
    any(target_os = "windows", target_os = "linux", target_os = "freebsd",)
))]
pub mod scap_screen_capture;

#[cfg(all(
    any(target_os = "windows", target_os = "linux"),
    feature = "screen-capture"
))]
pub(crate) type PlatformScreenCaptureFrame = scap::frame::Frame;
#[cfg(not(feature = "screen-capture"))]
pub(crate) type PlatformScreenCaptureFrame = ();
#[cfg(all(target_os = "macos", feature = "screen-capture"))]
pub(crate) type PlatformScreenCaptureFrame = core_video::image_buffer::CVImageBuffer;

use crate::{
    Action, AnyWindowHandle, App, AsyncWindowContext, BackgroundExecutor, Bounds,
    DEFAULT_WINDOW_SIZE, DevicePixels, DispatchEventResult, Font, FontId, FontMetrics, FontRun,
    ForegroundExecutor, GlyphId, GpuSpecs, Hsla, ImageSource, Keymap, LineLayout, Pixels,
    PlatformInput, Point, Priority, RenderGlyphParams, RenderImage, RenderImageParams,
    RenderSvgParams, Scene, ShapedGlyph, ShapedRun, SharedString, Size, SvgRenderer,
    SystemWindowTab, Task, Window, WindowControlArea, hash, point, px, size,
};
use anyhow::Result;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use anyhow::bail;
use async_task::Runnable;
use futures::channel::oneshot;
#[cfg(any(test, feature = "test-support"))]
use image::RgbaImage;
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder as _, Frame};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use scheduler::Instant;
pub use scheduler::RunnableMeta;
use schemars::JsonSchema;
use seahash::SeaHasher;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::ops;
use std::time::Duration;
use std::{
    fmt::{self, Debug},
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};
use strum::EnumIter;
use uuid::Uuid;

pub use app_menu::*;
pub use keyboard::*;
pub use keystroke::*;

#[cfg(any(test, feature = "test-support"))]
pub(crate) use test::*;

#[cfg(any(test, feature = "test-support"))]
pub use test::{TestDispatcher, TestScreenCaptureSource, TestScreenCaptureStream};

#[cfg(any(test, feature = "bench"))]
pub use bench_dispatcher::BenchDispatcher;

#[cfg(all(target_os = "macos", any(test, feature = "test-support")))]
pub use visual_test::VisualTestPlatform;

mod atlas;
mod clipboard;
mod dispatcher_text;
mod display_capture;
mod input;
mod platform_core;
mod window_options;
mod window_traits;
mod window_types;

#[cfg(test)]
mod image_tests;
#[cfg(all(test, any(target_os = "linux", target_os = "freebsd")))]
mod window_button_tests;

pub use atlas::*;
pub use clipboard::*;
pub use dispatcher_text::*;
pub use display_capture::*;
pub use input::*;
pub use platform_core::*;
pub use window_options::*;
pub use window_traits::*;
pub use window_types::*;

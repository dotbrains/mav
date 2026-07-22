use crate::{
    BoolExt, DisplayLink, MacDisplay, NSRange, NSStringExt, TISCopyCurrentKeyboardInputSource,
    TISGetInputSourceProperty, events::platform_input_from_native,
    kTISPropertyInputSourceIsASCIICapable, kTISPropertyInputSourceType, kTISTypeKeyboardInputMode,
    ns_string, renderer,
};
#[cfg(any(test, feature = "test-support"))]
use anyhow::Result;
use block::ConcreteBlock;
use cocoa::{
    appkit::{
        NSAppKitVersionNumber, NSAppKitVersionNumber12_0, NSApplication, NSBackingStoreBuffered,
        NSColor, NSEvent, NSEventModifierFlags, NSFilenamesPboardType, NSPasteboard, NSScreen,
        NSView, NSViewHeightSizable, NSViewWidthSizable, NSVisualEffectMaterial,
        NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowCollectionBehavior,
        NSWindowOcclusionState, NSWindowOrderingMode, NSWindowStyleMask, NSWindowTitleVisibility,
    },
    base::{id, nil},
    foundation::{
        NSArray, NSAutoreleasePool, NSDictionary, NSFastEnumeration, NSInteger, NSNotFound,
        NSOperatingSystemVersion, NSPoint, NSProcessInfo, NSRect, NSSize, NSString, NSUInteger,
        NSUserDefaults,
    },
};
use dispatch2::DispatchQueue;
use gpui::{
    AnyWindowHandle, BackgroundExecutor, Bounds, Capslock, CursorStyle, ExternalPaths,
    FileDropEvent, ForegroundExecutor, KeyDownEvent, Keystroke, Modifiers, ModifiersChangedEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, PlatformAtlas,
    PlatformDisplay, PlatformInput, PlatformInputHandler, PlatformWindow, Point, PromptButton,
    PromptLevel, RequestFrameOptions, SharedString, Size, SystemWindowTab, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowKind, WindowParams, point,
    px, size,
};
#[cfg(any(test, feature = "test-support"))]
use image::RgbaImage;

use core_foundation::base::{CFRelease, CFTypeRef};
use core_foundation_sys::base::CFEqual;
use core_foundation_sys::number::{CFBooleanGetValue, CFBooleanRef};
use core_graphics::display::{CGDirectDisplayID, CGRect};
use futures::channel::oneshot;
use gpui_util::ResultExt;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{BOOL, Class, NO, Object, Protocol, Sel, YES},
    sel, sel_impl,
};
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBeep, NSButton as Objc2NSButton, NSView as Objc2NSView, NSWindow as Objc2NSWindow,
    NSWindowButton as Objc2NSWindowButton,
};
use objc2_foundation::{NSPoint as Objc2NSPoint, NSRect as Objc2NSRect};
use parking_lot::Mutex;
use raw_window_handle as rwh;
use smallvec::SmallVec;
use std::{
    cell::Cell,
    ffi::{CStr, c_void},
    mem,
    ops::Range,
    path::PathBuf,
    ptr::{self, NonNull},
    rc::Rc,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const WINDOW_STATE_IVAR: &str = "windowState";

static mut WINDOW_CLASS: *const Class = ptr::null();
static mut PANEL_CLASS: *const Class = ptr::null();
static mut VIEW_CLASS: *const Class = ptr::null();
static mut BLURRED_VIEW_CLASS: *const Class = ptr::null();

#[allow(non_upper_case_globals)]
const NSWindowStyleMaskNonactivatingPanel: NSWindowStyleMask =
    NSWindowStyleMask::from_bits_retain(1 << 7);
// WindowLevel const value ref: https://docs.rs/core-graphics2/0.4.1/src/core_graphics2/window_level.rs.html
#[allow(non_upper_case_globals)]
const NSNormalWindowLevel: NSInteger = 0;
#[allow(non_upper_case_globals)]
const NSFloatingWindowLevel: NSInteger = 3;
#[allow(non_upper_case_globals)]
const NSPopUpWindowLevel: NSInteger = 101;
#[allow(non_upper_case_globals)]
const NSTrackingMouseEnteredAndExited: NSUInteger = 0x01;
#[allow(non_upper_case_globals)]
const NSTrackingMouseMoved: NSUInteger = 0x02;
#[allow(non_upper_case_globals)]
const NSTrackingActiveAlways: NSUInteger = 0x80;
#[allow(non_upper_case_globals)]
const NSTrackingInVisibleRect: NSUInteger = 0x200;
#[allow(non_upper_case_globals)]
const NSWindowAnimationBehaviorUtilityWindow: NSInteger = 4;
#[allow(non_upper_case_globals)]
const NSViewLayerContentsRedrawDuringViewResize: NSInteger = 2;
// https://developer.apple.com/documentation/appkit/nsdragoperation
type NSDragOperation = NSUInteger;
#[allow(non_upper_case_globals)]
const NSDragOperationNone: NSDragOperation = 0;
#[allow(non_upper_case_globals)]
const NSDragOperationCopy: NSDragOperation = 1;
#[derive(PartialEq)]
pub enum UserTabbingPreference {
    Never,
    Always,
    InFullScreen,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    // Widely used private APIs; Apple uses them for their Terminal.app.
    fn CGSMainConnectionID() -> id;
    fn CGSSetWindowBackgroundBlurRadius(
        connection_id: id,
        window_id: NSInteger,
        radius: i64,
    ) -> i32;
}

mod accessibility;
mod blur_tabs_callbacks;
mod class_setup;
mod drag_drop_callbacks;
mod keyboard_callbacks;
mod lifecycle_callbacks;
mod open;
mod platform_helpers;
mod platform_window;
mod render_text_callbacks;
mod state;
mod state_access;
mod view_events;
mod window_notifications;

use accessibility::*;
use blur_tabs_callbacks::*;
pub(crate) use class_setup::set_active_window_cursor_style;
use class_setup::*;
use drag_drop_callbacks::*;
use keyboard_callbacks::*;
use lifecycle_callbacks::*;
use open::*;
use render_text_callbacks::*;
pub(crate) use state::MacWindow;
use state::*;
use state_access::*;
use view_events::*;
use window_notifications::*;

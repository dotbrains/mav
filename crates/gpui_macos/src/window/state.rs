use super::*;

pub(super) struct TrafficLightFrames {
    titlebar: Objc2NSRect,
    close: Objc2NSRect,
    minimize: Objc2NSRect,
    zoom: Objc2NSRect,
}

pub(super) struct TrafficLightButtons {
    close: Retained<Objc2NSButton>,
    minimize: Retained<Objc2NSButton>,
    zoom: Retained<Objc2NSButton>,
}

pub(super) struct MacWindowState {
    pub(super) handle: AnyWindowHandle,
    pub(super) foreground_executor: ForegroundExecutor,
    pub(super) background_executor: BackgroundExecutor,
    pub(super) native_window: id,
    pub(super) native_view: NonNull<Object>,
    pub(super) blurred_view: Option<id>,
    pub(super) background_appearance: WindowBackgroundAppearance,
    pub(super) cursor_style: CursorStyle,
    pub(super) cursor_visible: Arc<AtomicBool>,
    pub(super) display_link: Option<DisplayLink>,
    pub(super) renderer: renderer::Renderer,
    pub(super) request_frame_callback: Option<Box<dyn FnMut(RequestFrameOptions)>>,
    pub(super) event_callback: Option<Box<dyn FnMut(PlatformInput) -> gpui::DispatchEventResult>>,
    pub(super) activate_callback: Option<Box<dyn FnMut(bool)>>,
    pub(super) resize_callback: Option<Box<dyn FnMut(Size<Pixels>, f32)>>,
    pub(super) moved_callback: Option<Box<dyn FnMut()>>,
    pub(super) should_close_callback: Option<Box<dyn FnMut() -> bool>>,
    pub(super) close_callback: Option<Box<dyn FnOnce()>>,
    pub(super) appearance_changed_callback: Option<Box<dyn FnMut()>>,
    pub(super) input_handler: Option<PlatformInputHandler>,
    pub(super) last_key_equivalent: Option<KeyDownEvent>,
    pub(super) synthetic_drag_counter: usize,
    pub(super) traffic_light_position: Option<Point<Pixels>>,
    pub(super) traffic_light_frames: Option<TrafficLightFrames>,
    pub(super) transparent_titlebar: bool,
    pub(super) previous_modifiers_changed_event: Option<PlatformInput>,
    pub(super) keystroke_for_do_command: Option<Keystroke>,
    pub(super) do_command_handled: Option<bool>,
    pub(super) external_files_dragged: bool,
    // Whether the next left-mouse click is also the focusing click.
    pub(super) first_mouse: bool,
    pub(super) fullscreen_restore_bounds: Bounds<Pixels>,
    pub(super) move_tab_to_new_window_callback: Option<Box<dyn FnMut()>>,
    pub(super) merge_all_windows_callback: Option<Box<dyn FnMut()>>,
    pub(super) select_next_tab_callback: Option<Box<dyn FnMut()>>,
    pub(super) select_previous_tab_callback: Option<Box<dyn FnMut()>>,
    pub(super) toggle_tab_bar_callback: Option<Box<dyn FnMut()>>,
    pub(super) activated_least_once: bool,
    pub(super) closed: Arc<AtomicBool>,
    pub(super) accesskit_adapter: Option<accesskit_macos::SubclassingAdapter>,
    // The parent window if this window is a sheet (Dialog kind)
    pub(super) sheet_parent: Option<id>,
}

impl MacWindowState {
    pub(super) fn move_traffic_light(&mut self) {
        if let Some(traffic_light_position) = self.traffic_light_position {
            if self.is_fullscreen() {
                self.restore_traffic_light();
                return;
            }

            if self.traffic_light_frames.is_none() {
                self.traffic_light_frames = self.capture_traffic_light_frames();
            }

            let window_height = Pixels::from(self.native_window().frame().size.height);
            if self.traffic_light_frames.is_some() {
                // AppKit can recreate standard buttons, so fetch the live views for each layout pass.
                let Some(buttons) = self.traffic_light_buttons() else {
                    return;
                };
                let Some(titlebar_container) = Self::titlebar_container(&buttons.close) else {
                    return;
                };

                let close_frame = buttons.close.frame();
                let minimize_frame = buttons.minimize.frame();
                let button_width = Pixels::from(close_frame.size.width);
                let button_height = Pixels::from(close_frame.size.height);
                let button_padding = Pixels::from(
                    minimize_frame.origin.x - close_frame.origin.x - close_frame.size.width,
                );
                let container_height =
                    button_height + traffic_light_position.y + traffic_light_position.y;

                let mut titlebar_frame = titlebar_container.frame();
                titlebar_frame.size.height = container_height.to_f64();
                titlebar_frame.origin.y = (window_height - container_height).to_f64();

                let minimize_x = traffic_light_position.x + button_width + button_padding;
                let zoom_x = minimize_x + button_width + button_padding;

                titlebar_container.setFrame(titlebar_frame);
                buttons.close.setFrameOrigin(Objc2NSPoint::new(
                    traffic_light_position.x.to_f64(),
                    traffic_light_position.y.to_f64(),
                ));
                buttons.minimize.setFrameOrigin(Objc2NSPoint::new(
                    minimize_x.to_f64(),
                    traffic_light_position.y.to_f64(),
                ));
                buttons.zoom.setFrameOrigin(Objc2NSPoint::new(
                    zoom_x.to_f64(),
                    traffic_light_position.y.to_f64(),
                ));

                titlebar_container.updateTrackingAreas();
                buttons.close.updateTrackingAreas();
                buttons.minimize.updateTrackingAreas();
                buttons.zoom.updateTrackingAreas();
            }
        }
    }

    pub(super) fn capture_traffic_light_frames(&self) -> Option<TrafficLightFrames> {
        let buttons = self.traffic_light_buttons()?;
        let titlebar_container = Self::titlebar_container(&buttons.close)?;

        Some(TrafficLightFrames {
            titlebar: titlebar_container.frame(),
            close: buttons.close.frame(),
            minimize: buttons.minimize.frame(),
            zoom: buttons.zoom.frame(),
        })
    }

    pub(super) fn native_window(&self) -> &Objc2NSWindow {
        // SAFETY: `MacWindow::new` initializes `self.native_window` with the AppKit
        // window for this state. It is either `NSWindow` or `NSPanel`, so borrowing it
        // as `Objc2NSWindow` is valid here.
        unsafe { &*self.native_window.cast::<Objc2NSWindow>() }
    }

    pub(super) fn traffic_light_buttons(&self) -> Option<TrafficLightButtons> {
        let window = self.native_window();
        Some(TrafficLightButtons {
            close: window.standardWindowButton(Objc2NSWindowButton::CloseButton)?,
            minimize: window.standardWindowButton(Objc2NSWindowButton::MiniaturizeButton)?,
            zoom: window.standardWindowButton(Objc2NSWindowButton::ZoomButton)?,
        })
    }

    pub(super) fn titlebar_container(
        close_button: &Objc2NSButton,
    ) -> Option<Retained<Objc2NSView>> {
        // SAFETY: `close_button` comes from AppKit's `standardWindowButton(_:)`.
        // Although `superview` is unsafe, objc2 returns each result as `Retained<NSView>`.
        unsafe {
            let button_container = close_button.superview()?;
            button_container.superview()
        }
    }

    pub(super) fn restore_traffic_light(&mut self) {
        if let Some(frames) = self.traffic_light_frames.take() {
            let Some(buttons) = self.traffic_light_buttons() else {
                return;
            };
            let Some(titlebar_container) = Self::titlebar_container(&buttons.close) else {
                return;
            };

            buttons.close.setFrame(frames.close);
            buttons.minimize.setFrame(frames.minimize);
            buttons.zoom.setFrame(frames.zoom);
            titlebar_container.setFrame(frames.titlebar);

            titlebar_container.updateTrackingAreas();
            buttons.close.updateTrackingAreas();
            buttons.minimize.updateTrackingAreas();
            buttons.zoom.updateTrackingAreas();
        }
    }

    pub(super) fn start_display_link(&mut self) {
        self.stop_display_link();
        unsafe {
            if !self
                .native_window
                .occlusionState()
                .contains(NSWindowOcclusionState::NSWindowOcclusionStateVisible)
            {
                return;
            }
        }
        let display_id = unsafe { display_id_for_screen(self.native_window.screen()) };
        if let Some(mut display_link) =
            DisplayLink::new(display_id, self.native_view.as_ptr() as *mut c_void, step).log_err()
        {
            display_link.start().log_err();
            self.display_link = Some(display_link);
        }
    }

    pub(super) fn stop_display_link(&mut self) {
        self.display_link = None;
    }

    pub(super) fn is_maximized(&self) -> bool {
        fn rect_to_size(rect: NSRect) -> Size<Pixels> {
            let NSSize { width, height } = rect.size;
            size(width.into(), height.into())
        }

        unsafe {
            let bounds = self.bounds();
            let screen_size = rect_to_size(self.native_window.screen().visibleFrame());
            bounds.size == screen_size
        }
    }

    pub(super) fn is_fullscreen(&self) -> bool {
        unsafe {
            let style_mask = self.native_window.styleMask();
            style_mask.contains(NSWindowStyleMask::NSFullScreenWindowMask)
        }
    }

    pub(super) fn bounds(&self) -> Bounds<Pixels> {
        let mut window_frame = unsafe { NSWindow::frame(self.native_window) };
        let screen = unsafe { NSWindow::screen(self.native_window) };
        if screen == nil {
            return Bounds::new(point(px(0.), px(0.)), gpui::DEFAULT_WINDOW_SIZE);
        }
        let screen_frame = unsafe { NSScreen::frame(screen) };

        // Flip the y coordinate to be top-left origin
        window_frame.origin.y =
            screen_frame.size.height - window_frame.origin.y - window_frame.size.height;

        Bounds::new(
            point(
                px((window_frame.origin.x - screen_frame.origin.x) as f32),
                px((window_frame.origin.y + screen_frame.origin.y) as f32),
            ),
            size(
                px(window_frame.size.width as f32),
                px(window_frame.size.height as f32),
            ),
        )
    }

    pub(super) fn content_size(&self) -> Size<Pixels> {
        let NSSize { width, height, .. } =
            unsafe { NSView::frame(self.native_window.contentView()) }.size;
        size(px(width as f32), px(height as f32))
    }

    pub(super) fn scale_factor(&self) -> f32 {
        get_scale_factor(self.native_window)
    }

    pub(super) fn window_bounds(&self) -> WindowBounds {
        if self.is_fullscreen() {
            WindowBounds::Fullscreen(self.fullscreen_restore_bounds)
        } else {
            WindowBounds::Windowed(self.bounds())
        }
    }
}

unsafe impl Send for MacWindowState {}

pub(crate) struct MacWindow(pub(super) Arc<Mutex<MacWindowState>>);

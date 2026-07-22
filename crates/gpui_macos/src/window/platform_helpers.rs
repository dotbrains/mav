use super::*;

impl MacWindow {
    pub(super) fn prompt_sheet(
        &self,
        level: PromptLevel,
        msg: &str,
        detail: Option<&str>,
        answers: &[PromptButton],
    ) -> Option<oneshot::Receiver<usize>> {
        // NSAlert's first button keeps Return and Cancel keeps Escape, but the keyboard
        // focus (and therefore Space) defaults to Cancel, leaving the middle button of
        // prompts like "Save / Don't Save / Cancel" unreachable from the keyboard. Move
        // the initial focus onto the last non-cancel, non-default button instead.
        let initial_focus_ix = answers
            .iter()
            .enumerate()
            .rev()
            .find(|(_, label)| !label.is_cancel())
            .map(|(ix, _)| ix)
            .filter(|&ix| ix > 0);

        unsafe {
            let alert: id = msg_send![class!(NSAlert), alloc];
            let alert: id = msg_send![alert, init];
            let alert_style = match level {
                PromptLevel::Info => 1,
                PromptLevel::Warning => 0,
                PromptLevel::Critical => 2,
            };
            let _: () = msg_send![alert, setAlertStyle: alert_style];
            let _: () = msg_send![alert, setMessageText: ns_string(msg)];
            if let Some(detail) = detail {
                let _: () = msg_send![alert, setInformativeText: ns_string(detail)];
            }

            let mut initial_focus_button: Option<id> = None;
            for (ix, answer) in answers.iter().enumerate() {
                let button: id = msg_send![alert, addButtonWithTitle: ns_string(answer.label())];
                let _: () = msg_send![button, setTag: ix as NSInteger];

                if answer.is_cancel() {
                    if let Some(key) = std::char::from_u32(crate::events::ESCAPE_KEY as u32) {
                        let _: () =
                            msg_send![button, setKeyEquivalent: ns_string(&key.to_string())];
                    }
                } else if Some(ix) == initial_focus_ix {
                    initial_focus_button = Some(button);
                }
            }

            if let Some(button) = initial_focus_button {
                let alert_window: id = msg_send![alert, window];
                let _: () = msg_send![alert_window, setInitialFirstResponder: button];
            }

            let (done_tx, done_rx) = oneshot::channel();
            let done_tx = Cell::new(Some(done_tx));
            let block = ConcreteBlock::new(move |answer: NSInteger| {
                let _: () = msg_send![alert, release];
                if let Some(done_tx) = done_tx.take() {
                    let _ = done_tx.send(answer.try_into().unwrap());
                }
            });
            let block = block.copy();
            let lock = self.0.lock();
            let native_window = lock.native_window;
            let closed = lock.closed.clone();
            let executor = lock.foreground_executor.clone();
            executor
                .spawn(async move {
                    if !closed.load(Ordering::Acquire) {
                        let _: () = msg_send![
                            alert,
                            beginSheetModalForWindow: native_window
                            completionHandler: block
                        ];
                    } else {
                        let _: () = msg_send![alert, release];
                    }
                })
                .detach();

            Some(done_rx)
        }
    }

    pub(super) fn set_background_appearance_impl(
        &self,
        background_appearance: WindowBackgroundAppearance,
    ) {
        let mut this = self.0.as_ref().lock();
        this.background_appearance = background_appearance;

        let opaque = background_appearance == WindowBackgroundAppearance::Opaque;
        this.renderer.update_transparency(!opaque);

        unsafe {
            this.native_window.setOpaque_(opaque as BOOL);
            let background_color = if opaque {
                NSColor::colorWithSRGBRed_green_blue_alpha_(nil, 0f64, 0f64, 0f64, 1f64)
            } else {
                // Not using `+[NSColor clearColor]` to avoid broken shadow.
                NSColor::colorWithSRGBRed_green_blue_alpha_(nil, 0f64, 0f64, 0f64, 0.0001)
            };
            this.native_window.setBackgroundColor_(background_color);

            if NSAppKitVersionNumber < NSAppKitVersionNumber12_0 {
                // Whether `-[NSVisualEffectView respondsToSelector:@selector(_updateProxyLayer)]`.
                // On macOS Catalina/Big Sur `NSVisualEffectView` doesn’t own concrete sublayers
                // but uses a `CAProxyLayer`. Use the legacy WindowServer API.
                let blur_radius = if background_appearance == WindowBackgroundAppearance::Blurred {
                    80
                } else {
                    0
                };

                let window_number = this.native_window.windowNumber();
                CGSSetWindowBackgroundBlurRadius(CGSMainConnectionID(), window_number, blur_radius);
            } else {
                // On newer macOS `NSVisualEffectView` manages the effect layer directly. Using it
                // could have a better performance (it downsamples the backdrop) and more control
                // over the effect layer.
                if background_appearance != WindowBackgroundAppearance::Blurred {
                    if let Some(blur_view) = this.blurred_view {
                        NSView::removeFromSuperview(blur_view);
                        this.blurred_view = None;
                    }
                } else if this.blurred_view.is_none() {
                    let content_view = this.native_window.contentView();
                    let frame = NSView::bounds(content_view);
                    let mut blur_view: id = msg_send![BLURRED_VIEW_CLASS, alloc];
                    blur_view = NSView::initWithFrame_(blur_view, frame);
                    blur_view.setAutoresizingMask_(NSViewWidthSizable | NSViewHeightSizable);

                    let _: () = msg_send![
                        content_view,
                        addSubview: blur_view
                        positioned: NSWindowOrderingMode::NSWindowBelow
                        relativeTo: nil
                    ];
                    this.blurred_view = Some(blur_view.autorelease());
                }
            }
        }
    }

    pub(super) fn tabbed_windows_impl(&self) -> Option<Vec<SystemWindowTab>> {
        unsafe {
            let windows: id = msg_send![self.0.lock().native_window, tabbedWindows];
            if windows.is_null() {
                return None;
            }

            let count: NSUInteger = msg_send![windows, count];
            let mut result = Vec::new();
            for i in 0..count {
                let window: id = msg_send![windows, objectAtIndex:i];
                if msg_send![window, isKindOfClass: WINDOW_CLASS] {
                    let handle = get_window_state(&*window).lock().handle;
                    let title: id = msg_send![window, title];
                    let title = SharedString::from(title.to_str().to_string());

                    result.push(SystemWindowTab::new(title, handle));
                }
            }

            Some(result)
        }
    }

    pub(super) fn titlebar_double_click_impl(&self) {
        let this = self.0.lock();
        let window = this.native_window;
        let closed = this.closed.clone();
        this.foreground_executor
            .spawn(async move {
                if_window_not_closed(closed, || {
                    unsafe {
                        let defaults: id = NSUserDefaults::standardUserDefaults();
                        let domain = ns_string("NSGlobalDomain");
                        let key = ns_string("AppleActionOnDoubleClick");

                        let dict: id = msg_send![defaults, persistentDomainForName: domain];
                        let action: id = if !dict.is_null() {
                            msg_send![dict, objectForKey: key]
                        } else {
                            nil
                        };

                        let action_str = if !action.is_null() {
                            CStr::from_ptr(NSString::UTF8String(action)).to_string_lossy()
                        } else {
                            "".into()
                        };

                        match action_str.as_ref() {
                            "None" => {
                                // "Do Nothing" selected, so do no action
                            }
                            "Minimize" => {
                                window.miniaturize_(nil);
                            }
                            "Maximize" => {
                                window.zoom_(nil);
                            }
                            "Fill" => {
                                // There is no documented API for "Fill" action, so we'll just zoom the window
                                window.zoom_(nil);
                            }
                            _ => {
                                window.zoom_(nil);
                            }
                        }
                    }
                })
            })
            .detach();
    }
}

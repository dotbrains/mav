use super::*;

impl LinuxClient for WaylandClient {
    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        Box::new(self.0.borrow().keyboard_layout.clone())
    }

    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        self.0
            .borrow()
            .outputs
            .iter()
            .map(|(id, output)| {
                Rc::new(WaylandDisplay {
                    id: id.clone(),
                    name: output.name.clone(),
                    bounds: output.bounds.to_pixels(output.scale as f32),
                }) as Rc<dyn PlatformDisplay>
            })
            .collect()
    }

    fn display(&self, id: DisplayId) -> Option<Rc<dyn PlatformDisplay>> {
        self.0
            .borrow()
            .outputs
            .iter()
            .find_map(|(object_id, output)| {
                (object_id.protocol_id() as u64 == u64::from(id)).then(|| {
                    Rc::new(WaylandDisplay {
                        id: object_id.clone(),
                        name: output.name.clone(),
                        bounds: output.bounds.to_pixels(output.scale as f32),
                    }) as Rc<dyn PlatformDisplay>
                })
            })
    }

    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        None
    }

    #[cfg(feature = "screen-capture")]
    fn screen_capture_sources(
        &self,
    ) -> futures::channel::oneshot::Receiver<anyhow::Result<Vec<Rc<dyn gpui::ScreenCaptureSource>>>>
    {
        // TODO: Get screen capture working on wayland. Be sure to try window resizing as that may
        // be tricky.
        //
        // start_scap_default_target_source()
        let (sources_tx, sources_rx) = futures::channel::oneshot::channel();
        sources_tx
            .send(Err(anyhow::anyhow!(
                "Wayland screen capture not yet implemented."
            )))
            .ok();
        sources_rx
    }

    fn open_window(
        &self,
        handle: AnyWindowHandle,
        params: WindowParams,
    ) -> anyhow::Result<Box<dyn PlatformWindow>> {
        let mut state = self.0.borrow_mut();

        let parent = state.keyboard_focused_window.clone();

        let target_output = params.display_id.and_then(|display_id| {
            let target_protocol_id: u64 = display_id.into();
            state
                .wl_outputs
                .iter()
                .find(|(id, _)| id.protocol_id() as u64 == target_protocol_id)
                .map(|(_, output)| output.clone())
        });

        let appearance = state.common.appearance;
        let compositor_gpu = state.compositor_gpu.take();
        let (window, surface_id) = WaylandWindow::new(
            handle,
            state.globals.clone(),
            state.gpu_context.clone(),
            compositor_gpu,
            WaylandClientStatePtr(Rc::downgrade(&self.0)),
            params,
            appearance,
            parent,
            target_output,
        )?;
        if window.0.toplevel().is_some() {
            state.consume_startup_activation_token(&window.0.surface());
        }
        state.windows.insert(surface_id, window.0.clone());

        Ok(Box::new(window))
    }

    fn set_cursor_style(&self, style: CursorStyle) {
        let mut state = self.0.borrow_mut();

        let need_update = state.cursor_style != Some(style)
            && (state.mouse_focused_window.is_none()
                || state
                    .mouse_focused_window
                    .as_ref()
                    .is_some_and(|w| !w.is_blocked()));

        if !need_update {
            return;
        }

        state.cursor_style = Some(style);

        // Don't clobber the invisible cursor; restore reads back from `cursor_style`.
        if state.cursor_hidden_window.is_some() {
            return;
        }

        let serial = state.serial_tracker.get(SerialKind::MouseEnter);
        if let Some(cursor_shape_device) = &state.cursor_shape_device {
            cursor_shape_device.set_shape(serial, to_shape(style));
        } else if let Some(focused_window) = &state.mouse_focused_window {
            // cursor-shape-v1 isn't supported, set the cursor using a surface.
            let wl_pointer = state
                .wl_pointer
                .clone()
                .expect("window is focused by pointer");
            let scale = focused_window.primary_output_scale();
            state.cursor.set_icon(
                &wl_pointer,
                serial,
                cursor_style_to_icon_names(style),
                scale,
            );
        }
    }

    fn hide_cursor_until_mouse_moves(&self) {
        self.0.borrow_mut().hide_cursor_until_mouse_moves();
    }

    fn is_cursor_visible(&self) -> bool {
        self.0.borrow().cursor_hidden_window.is_none()
    }

    fn open_uri(&self, uri: &str) {
        let mut state = self.0.borrow_mut();
        if let (Some(activation), Some(window)) = (
            state.globals.activation.clone(),
            state.mouse_focused_window.clone(),
        ) {
            state.pending_activation = Some(PendingActivation::Uri(uri.to_string()));
            let token = activation.get_activation_token(&state.globals.qh, ());
            let serial = state.serial_tracker.get(SerialKind::MousePress);
            token.set_serial(serial, &state.wl_seat);
            token.set_surface(&window.surface());
            token.commit();
        } else {
            let executor = state.common.background_executor.clone();
            open_uri_internal(executor, uri, None);
        }
    }

    fn reveal_path(&self, path: PathBuf) {
        let mut state = self.0.borrow_mut();
        if let (Some(activation), Some(window)) = (
            state.globals.activation.clone(),
            state.mouse_focused_window.clone(),
        ) {
            state.pending_activation = Some(PendingActivation::Path(path));
            let token = activation.get_activation_token(&state.globals.qh, ());
            let serial = state.serial_tracker.get(SerialKind::MousePress);
            token.set_serial(serial, &state.wl_seat);
            token.set_surface(&window.surface());
            token.commit();
        } else {
            let executor = state.common.background_executor.clone();
            reveal_path_internal(executor, path, None);
        }
    }

    fn with_common<R>(&self, f: impl FnOnce(&mut LinuxCommon) -> R) -> R {
        f(&mut self.0.borrow_mut().common)
    }

    fn run(&self) {
        let mut event_loop = self
            .0
            .borrow_mut()
            .event_loop
            .take()
            .expect("App is already running");

        event_loop
            .run(
                None,
                &mut WaylandClientStatePtr(Rc::downgrade(&self.0)),
                |_| {},
            )
            .log_err();
    }

    fn write_to_primary(&self, item: gpui::ClipboardItem) {
        let mut state = self.0.borrow_mut();
        let (Some(primary_selection_manager), Some(primary_selection)) = (
            state.globals.primary_selection_manager.clone(),
            state.primary_selection.clone(),
        ) else {
            return;
        };
        if state.mouse_focused_window.is_some() || state.keyboard_focused_window.is_some() {
            state.clipboard.set_primary(item);
            let serial = state.serial_tracker.get_latest();
            let data_source = primary_selection_manager.create_source(&state.globals.qh, ());
            for mime_type in TEXT_MIME_TYPES {
                data_source.offer(mime_type.to_string());
            }
            data_source.offer(state.clipboard.self_mime());
            primary_selection.set_selection(Some(&data_source), serial);
        }
    }

    fn write_to_clipboard(&self, item: gpui::ClipboardItem) {
        let mut state = self.0.borrow_mut();
        let (Some(data_device_manager), Some(data_device)) = (
            state.globals.data_device_manager.clone(),
            state.data_device.clone(),
        ) else {
            return;
        };
        if state.mouse_focused_window.is_some() || state.keyboard_focused_window.is_some() {
            state.clipboard.set(item);
            let serial = state.serial_tracker.get_latest();
            let data_source = data_device_manager.create_data_source(&state.globals.qh, ());
            for mime_type in TEXT_MIME_TYPES {
                data_source.offer(mime_type.to_string());
            }
            data_source.offer(state.clipboard.self_mime());
            data_device.set_selection(Some(&data_source), serial);
        }
    }

    fn read_from_primary(&self) -> Option<gpui::ClipboardItem> {
        self.0.borrow_mut().clipboard.read_primary()
    }

    fn read_from_clipboard(&self) -> Option<gpui::ClipboardItem> {
        self.0.borrow_mut().clipboard.read()
    }

    fn active_window(&self) -> Option<AnyWindowHandle> {
        self.0
            .borrow_mut()
            .keyboard_focused_window
            .as_ref()
            .map(|window| window.handle())
    }

    fn window_stack(&self) -> Option<Vec<AnyWindowHandle>> {
        None
    }

    fn compositor_name(&self) -> &'static str {
        "Wayland"
    }

    fn window_identifier(&self) -> impl Future<Output = Option<WindowIdentifier>> + Send + 'static {
        async fn inner(surface: Option<wl_surface::WlSurface>) -> Option<WindowIdentifier> {
            if let Some(surface) = surface {
                ashpd::WindowIdentifier::from_wayland(&surface).await
            } else {
                None
            }
        }

        let client_state = self.0.borrow();
        let active_window = client_state.keyboard_focused_window.as_ref();
        inner(active_window.map(|aw| aw.surface()))
    }
}

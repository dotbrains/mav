use super::*;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WaylandClientStatePtr {
    fn event(
        this: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let client = this.get_client();
        let mut state = client.borrow_mut();

        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match &interface[..] {
                "wl_seat" => {
                    if let Some(wl_pointer) = state.wl_pointer.take() {
                        wl_pointer.release();
                    }
                    if let Some(wl_keyboard) = state.wl_keyboard.take() {
                        wl_keyboard.release();
                    }
                    state.wl_seat.release();
                    state.wl_seat = registry.bind::<wl_seat::WlSeat, _, _>(
                        name,
                        wl_seat_version(version),
                        qh,
                        (),
                    );
                }
                "wl_output" => {
                    let output = registry.bind::<wl_output::WlOutput, _, _>(
                        name,
                        wl_output_version(version),
                        qh,
                        (),
                    );

                    state
                        .in_progress_outputs
                        .insert(output.id(), InProgressOutput::default());
                    state.wl_outputs.insert(output.id(), output);
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name: _ } => {
                // TODO: handle global removal
            }
            _ => {}
        }
    }
}

delegate_noop!(WaylandClientStatePtr: ignore xdg_activation_v1::XdgActivationV1);
delegate_noop!(WaylandClientStatePtr: ignore xdg_system_bell_v1::XdgSystemBellV1);
delegate_noop!(WaylandClientStatePtr: ignore wl_compositor::WlCompositor);
delegate_noop!(WaylandClientStatePtr: ignore wp_cursor_shape_device_v1::WpCursorShapeDeviceV1);
delegate_noop!(WaylandClientStatePtr: ignore wp_cursor_shape_manager_v1::WpCursorShapeManagerV1);
delegate_noop!(WaylandClientStatePtr: ignore wl_data_device_manager::WlDataDeviceManager);
delegate_noop!(WaylandClientStatePtr: ignore zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1);
delegate_noop!(WaylandClientStatePtr: ignore wl_shm::WlShm);
delegate_noop!(WaylandClientStatePtr: ignore wl_shm_pool::WlShmPool);
delegate_noop!(WaylandClientStatePtr: ignore wl_buffer::WlBuffer);
delegate_noop!(WaylandClientStatePtr: ignore wl_region::WlRegion);
delegate_noop!(WaylandClientStatePtr: ignore wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1);
delegate_noop!(WaylandClientStatePtr: ignore zxdg_decoration_manager_v1::ZxdgDecorationManagerV1);
delegate_noop!(WaylandClientStatePtr: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);
delegate_noop!(WaylandClientStatePtr: ignore org_kde_kwin_blur_manager::OrgKdeKwinBlurManager);
delegate_noop!(WaylandClientStatePtr: ignore zwp_text_input_manager_v3::ZwpTextInputManagerV3);
delegate_noop!(WaylandClientStatePtr: ignore org_kde_kwin_blur::OrgKdeKwinBlur);
delegate_noop!(WaylandClientStatePtr: ignore wp_viewporter::WpViewporter);
delegate_noop!(WaylandClientStatePtr: ignore wp_viewport::WpViewport);

impl Dispatch<WlCallback, ObjectId> for WaylandClientStatePtr {
    fn event(
        state: &mut WaylandClientStatePtr,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        surface_id: &ObjectId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let client = state.get_client();
        let mut state = client.borrow_mut();
        let Some(window) = get_window(&mut state, surface_id) else {
            return;
        };
        drop(state);

        if let wl_callback::Event::Done { .. } = event {
            window.frame();
        }
    }
}

pub(crate) fn get_window(
    state: &mut RefMut<WaylandClientState>,
    surface_id: &ObjectId,
) -> Option<WaylandWindowStatePtr> {
    state.windows.get(surface_id).cloned()
}

impl Dispatch<wl_surface::WlSurface, ()> for WaylandClientStatePtr {
    fn event(
        this: &mut Self,
        surface: &wl_surface::WlSurface,
        event: <wl_surface::WlSurface as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let client = this.get_client();
        let mut state = client.borrow_mut();

        let Some(window) = get_window(&mut state, &surface.id()) else {
            return;
        };
        #[allow(clippy::mutable_key_type)]
        let outputs = state.outputs.clone();
        drop(state);

        window.handle_surface_event(event, outputs);
    }
}

impl Dispatch<wl_output::WlOutput, ()> for WaylandClientStatePtr {
    fn event(
        this: &mut Self,
        output: &wl_output::WlOutput,
        event: <wl_output::WlOutput as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let client = this.get_client();
        let mut state = client.borrow_mut();

        let Some(in_progress_output) = state.in_progress_outputs.get_mut(&output.id()) else {
            return;
        };

        match event {
            wl_output::Event::Name { name } => {
                in_progress_output.name = Some(name);
            }
            wl_output::Event::Scale { factor } => {
                in_progress_output.scale = Some(factor);
            }
            wl_output::Event::Geometry { x, y, subpixel, .. } => {
                in_progress_output.position = Some(point(DevicePixels(x), DevicePixels(y)));
                if let WEnum::Value(subpixel) = subpixel {
                    in_progress_output.subpixel = Some(subpixel);
                }
            }
            wl_output::Event::Mode { width, height, .. } => {
                in_progress_output.size = Some(size(DevicePixels(width), DevicePixels(height)))
            }
            wl_output::Event::Done => {
                if let Some(complete) = in_progress_output.complete() {
                    state.outputs.insert(output.id(), complete);
                }
                state.in_progress_outputs.remove(&output.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ObjectId> for WaylandClientStatePtr {
    fn event(
        state: &mut Self,
        _: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        surface_id: &ObjectId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let client = state.get_client();
        let mut state = client.borrow_mut();
        let Some(window) = get_window(&mut state, surface_id) else {
            return;
        };
        drop(state);
        window.handle_xdg_surface_event(event);
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ObjectId> for WaylandClientStatePtr {
    fn event(
        this: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: <xdg_toplevel::XdgToplevel as Proxy>::Event,
        surface_id: &ObjectId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let client = this.get_client();
        let mut state = client.borrow_mut();
        let Some(window) = get_window(&mut state, surface_id) else {
            return;
        };

        drop(state);
        let should_close = window.handle_toplevel_event(event);

        if should_close {
            // The close logic will be handled in drop_window()
            window.close();
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ObjectId> for WaylandClientStatePtr {
    fn event(
        this: &mut Self,
        _: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: <zwlr_layer_surface_v1::ZwlrLayerSurfaceV1 as Proxy>::Event,
        surface_id: &ObjectId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let client = this.get_client();
        let mut state = client.borrow_mut();
        let Some(window) = get_window(&mut state, surface_id) else {
            return;
        };

        drop(state);
        let should_close = window.handle_layersurface_event(event);

        if should_close {
            // The close logic will be handled in drop_window()
            window.close();
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for WaylandClientStatePtr {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: <xdg_wm_base::XdgWmBase as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, ()> for WaylandClientStatePtr {
    fn event(
        this: &mut Self,
        token: &xdg_activation_token_v1::XdgActivationTokenV1,
        event: <xdg_activation_token_v1::XdgActivationTokenV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let client = this.get_client();
        let mut state = client.borrow_mut();

        if let xdg_activation_token_v1::Event::Done { token } = event {
            let executor = state.common.background_executor.clone();
            match state.pending_activation.take() {
                Some(PendingActivation::Uri(uri)) => open_uri_internal(executor, &uri, Some(token)),
                Some(PendingActivation::Path(path)) => {
                    reveal_path_internal(executor, path, Some(token))
                }
                Some(PendingActivation::Window(window)) => {
                    let Some(window) = get_window(&mut state, &window) else {
                        return;
                    };
                    let activation = state.globals.activation.as_ref().unwrap();
                    activation.activate(token, &window.surface());
                }
                None => log::error!("activation token received with no pending activation"),
            }
        }

        token.destroy();
    }
}

use super::*;

impl WaylandClient {
    pub(crate) fn new() -> Self {
        let startup_activation_token = take_startup_activation_token_from_environment();
        let conn = Connection::connect_to_env().unwrap();

        let (globals, event_queue) = registry_queue_init::<WaylandClientStatePtr>(&conn).unwrap();
        let qh = event_queue.handle();

        let mut seat: Option<wl_seat::WlSeat> = None;
        #[allow(clippy::mutable_key_type)]
        let mut in_progress_outputs = HashMap::default();
        #[allow(clippy::mutable_key_type)]
        let mut wl_outputs: HashMap<ObjectId, wl_output::WlOutput> = HashMap::default();
        globals.contents().with_list(|list| {
            for global in list {
                match &global.interface[..] {
                    "wl_seat" => {
                        seat = Some(globals.registry().bind::<wl_seat::WlSeat, _, _>(
                            global.name,
                            wl_seat_version(global.version),
                            &qh,
                            (),
                        ));
                    }
                    "wl_output" => {
                        let output = globals.registry().bind::<wl_output::WlOutput, _, _>(
                            global.name,
                            wl_output_version(global.version),
                            &qh,
                            (),
                        );
                        in_progress_outputs.insert(output.id(), InProgressOutput::default());
                        wl_outputs.insert(output.id(), output);
                    }
                    _ => {}
                }
            }
        });

        let event_loop = EventLoop::<WaylandClientStatePtr>::try_new().unwrap();

        let (common, main_receiver, wake_receiver) = LinuxCommon::new(event_loop.get_signal());

        let handle = event_loop.handle();
        handle
            .insert_source(main_receiver, {
                let handle = handle.clone();
                move |event, _, _: &mut WaylandClientStatePtr| {
                    if let calloop::channel::Event::Msg(runnable) = event {
                        handle.insert_idle(|_| {
                            let location = runnable.metadata().location;
                            let spawned = runnable.metadata().spawned;
                            profiler::update_running_task(spawned, location);
                            runnable.run();
                            profiler::save_task_timing();
                        });
                    }
                }
            })
            .unwrap();

        handle
            .insert_source(
                wake_receiver,
                |event, _, client: &mut WaylandClientStatePtr| {
                    if let calloop::channel::Event::Msg(()) = event {
                        client.get_client().borrow_mut().common.handle_system_wake();
                    }
                },
            )
            .unwrap();

        let compositor_gpu = detect_compositor_gpu();
        let gpu_context = Rc::new(RefCell::new(None));

        let seat = seat.unwrap();
        let globals = Globals::new(
            globals,
            common.foreground_executor.clone(),
            qh.clone(),
            seat.clone(),
        );

        let data_device = globals
            .data_device_manager
            .as_ref()
            .map(|data_device_manager| data_device_manager.get_data_device(&seat, &qh, ()));

        let primary_selection = globals
            .primary_selection_manager
            .as_ref()
            .map(|primary_selection_manager| primary_selection_manager.get_device(&seat, &qh, ()));

        let cursor = Cursor::new(&conn, &globals, 24);

        handle
            .insert_source(XDPEventSource::new(&common.background_executor), {
                move |event, _, client| match event {
                    XDPEvent::WindowAppearance(appearance) => {
                        if let Some(client) = client.0.upgrade() {
                            let mut client = client.borrow_mut();

                            client.common.appearance = appearance;

                            for window in client.windows.values_mut() {
                                window.set_appearance(appearance);
                            }
                        }
                    }
                    XDPEvent::ButtonLayout(layout_str) => {
                        if let Some(client) = client.0.upgrade() {
                            let layout = WindowButtonLayout::parse(&layout_str)
                                .log_err()
                                .unwrap_or_else(WindowButtonLayout::linux_default);
                            let mut client = client.borrow_mut();
                            client.common.button_layout = layout;

                            for window in client.windows.values_mut() {
                                window.set_button_layout();
                            }
                        }
                    }
                    XDPEvent::CursorTheme(theme) => {
                        if let Some(client) = client.0.upgrade() {
                            let mut client = client.borrow_mut();
                            client.cursor.set_theme(theme);
                        }
                    }
                    XDPEvent::CursorSize(size) => {
                        if let Some(client) = client.0.upgrade() {
                            let mut client = client.borrow_mut();
                            client.cursor.set_size(size);
                        }
                    }
                }
            })
            .unwrap();

        let state = Rc::new(RefCell::new(WaylandClientState {
            serial_tracker: SerialTracker::new(),
            globals,
            gpu_context,
            compositor_gpu,
            wl_seat: seat,
            wl_pointer: None,
            wl_keyboard: None,
            pinch_gesture: None,
            pinch_scale: 1.0,
            cursor_shape_device: None,
            data_device,
            primary_selection,
            text_input: None,
            pre_edit_text: None,
            ime_pre_edit: None,
            composing: false,
            outputs: HashMap::default(),
            in_progress_outputs,
            wl_outputs,
            windows: HashMap::default(),
            common,
            keyboard_layout: LinuxKeyboardLayout::new(UNKNOWN_KEYBOARD_LAYOUT_NAME),
            keymap_state: None,
            compose_state: None,
            drag: DragState {
                data_offer: None,
                window: None,
                position: Point::default(),
            },
            click: ClickState {
                last_click: Instant::now(),
                last_mouse_button: None,
                last_location: Point::default(),
                current_count: 0,
            },
            repeat: KeyRepeat {
                characters_per_second: 16,
                delay: Duration::from_millis(500),
                current_id: 0,
                current_keycode: None,
            },
            modifiers: Modifiers {
                shift: false,
                control: false,
                alt: false,
                function: false,
                platform: false,
            },
            capslock: Capslock { on: false },
            scroll_event_received: false,
            axis_source: AxisSource::Wheel,
            mouse_location: None,
            continuous_scroll_delta: None,
            discrete_scroll_delta: None,
            vertical_modifier: -1.0,
            horizontal_modifier: -1.0,
            button_pressed: None,
            mouse_focused_window: None,
            keyboard_focused_window: None,
            loop_handle: handle.clone(),
            enter_token: None,
            cursor_style: None,
            cursor_hidden_window: None,
            clipboard: Clipboard::new(conn.clone(), handle.clone()),
            data_offers: Vec::new(),
            primary_data_offer: None,
            cursor,
            pending_activation: None,
            startup_activation_token,
            event_loop: Some(event_loop),
            ime_enabled: None,
        }));

        WaylandSource::new(conn, event_queue)
            .insert(handle)
            .unwrap();

        Self(state)
    }
}

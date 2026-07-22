use super::*;

impl App {
    pub(crate) fn new_app(
        platform: Rc<dyn Platform>,
        asset_source: Arc<dyn AssetSource>,
        http_client: Arc<dyn HttpClient>,
    ) -> Rc<AppCell> {
        let background_executor = platform.background_executor();
        let foreground_executor = platform.foreground_executor();
        assert!(
            background_executor.is_main_thread(),
            "must construct App on main thread"
        );
        let text_system = Arc::new(TextSystem::new(platform.text_system()));
        let entities = EntityMap::new();
        let keyboard_layout = platform.keyboard_layout();
        let keyboard_mapper = platform.keyboard_mapper();

        #[cfg(any(test, feature = "leak-detection"))]
        let _ref_counts = entities.ref_counts_drop_handle();

        let app = Rc::new_cyclic(|this| AppCell {
            app: RefCell::new(App {
                this: this.clone(),
                platform: platform.clone(),
                text_system,
                text_rendering_mode: Rc::new(Cell::new(TextRenderingMode::default())),
                mode: GpuiMode::Production,
                actions: Rc::new(ActionRegistry::default()),
                flushing_effects: false,
                pending_updates: 0,
                active_drag: None,
                background_executor,
                foreground_executor,
                svg_renderer: SvgRenderer::new(asset_source.clone()),
                loading_assets: Default::default(),
                asset_source,
                http_client,
                globals_by_type: Default::default(),
                entities,
                new_entity_observers: SubscriberSet::new(),
                windows: SlotMap::with_key(),
                window_update_stack: Vec::new(),
                window_handles: FxHashMap::default(),
                focus_handles: Arc::new(RwLock::new(SlotMap::with_key())),
                keymap: Rc::new(RefCell::new(Keymap::default())),
                keyboard_layout,
                keyboard_mapper,
                global_action_listeners: Default::default(),
                pending_effects: VecDeque::new(),
                pending_notifications: FxHashSet::default(),
                pending_global_notifications: Default::default(),
                observers: SubscriberSet::new(),
                tracked_entities: FxHashMap::default(),
                window_invalidators_by_entity: FxHashMap::default(),
                current_window_by_entity: FxHashMap::default(),
                event_listeners: SubscriberSet::new(),
                release_listeners: SubscriberSet::new(),
                keystroke_observers: SubscriberSet::new(),
                keystroke_interceptors: SubscriberSet::new(),
                keyboard_layout_observers: SubscriberSet::new(),
                thermal_state_observers: SubscriberSet::new(),
                global_observers: SubscriberSet::new(),
                quit_observers: SubscriberSet::new(),
                restart_observers: SubscriberSet::new(),
                restart_path: None,
                window_closed_observers: SubscriberSet::new(),
                layout_id_buffer: Default::default(),
                propagate_event: true,
                prompt_builder: Some(PromptBuilder::Default),
                #[cfg(any(feature = "inspector", debug_assertions))]
                inspector_renderer: None,
                #[cfg(any(feature = "inspector", debug_assertions))]
                inspector_element_registry: InspectorElementRegistry::default(),
                quit_mode: QuitMode::default(),
                quitting: false,
                cursor_hide_mode: CursorHideMode::default(),
                accessibility_force_disabled: false,

                #[cfg(any(test, feature = "test-support", debug_assertions))]
                name: None,
                element_arena: RefCell::new(Arena::new(1024 * 1024)),
                event_arena: Arena::new(1024 * 1024),

                #[cfg(any(test, feature = "leak-detection"))]
                _ref_counts,
            }),
        });

        init_app_menus(platform.as_ref(), &app.borrow());
        SystemWindowTabController::init(&mut app.borrow_mut());

        platform.on_keyboard_layout_change(Box::new({
            let app = Rc::downgrade(&app);
            move || {
                if let Some(app) = app.upgrade() {
                    let cx = &mut app.borrow_mut();
                    cx.keyboard_layout = cx.platform.keyboard_layout();
                    cx.keyboard_mapper = cx.platform.keyboard_mapper();
                    cx.keyboard_layout_observers
                        .clone()
                        .retain(&(), move |callback| (callback)(cx));
                }
            }
        }));

        platform.on_thermal_state_change(Box::new({
            let app = Rc::downgrade(&app);
            move || {
                if let Some(app) = app.upgrade() {
                    let cx = &mut app.borrow_mut();
                    cx.thermal_state_observers
                        .clone()
                        .retain(&(), move |callback| (callback)(cx));
                }
            }
        }));

        platform.on_quit(Box::new({
            let cx = Rc::downgrade(&app);
            move || {
                if let Some(cx) = cx.upgrade() {
                    cx.borrow_mut().shutdown();
                }
            }
        }));

        app
    }

    #[doc(hidden)]
    pub fn ref_counts_drop_handle(&self) -> impl Sized + use<> {
        self.entities.ref_counts_drop_handle()
    }

    /// Captures a snapshot of all entities that currently have alive handles.
    ///
    /// The returned [`LeakDetectorSnapshot`] can later be passed to
    /// [`assert_no_new_leaks`](Self::assert_no_new_leaks) to verify that no
    /// entities created after the snapshot are still alive.
    #[cfg(any(test, feature = "leak-detection"))]
    pub fn leak_detector_snapshot(&self) -> LeakDetectorSnapshot {
        self.entities.leak_detector_snapshot()
    }

    /// Asserts that no entities created after `snapshot` still have alive handles.
    ///
    /// Entities that were already tracked at the time of the snapshot are ignored,
    /// even if they still have handles. Only *new* entities (those whose
    /// `EntityId` was not present in the snapshot) are considered leaks.
    ///
    /// # Panics
    ///
    /// Panics if any new entity handles exist. The panic message lists every
    /// leaked entity with its type name, and includes allocation-site backtraces
    /// when `LEAK_BACKTRACE` is set.
    #[cfg(any(test, feature = "leak-detection"))]
    pub fn assert_no_new_leaks(&self, snapshot: &LeakDetectorSnapshot) {
        self.entities.assert_no_new_leaks(snapshot)
    }

    /// Quit the application gracefully. Handlers registered with [`Context::on_app_quit`]
    /// will be given `SHUTDOWN_TIMEOUT` to complete before exiting.
    pub fn shutdown(&mut self) {
        let mut futures = Vec::new();

        for observer in self.quit_observers.remove(&()) {
            futures.push(observer(self));
        }

        self.windows.clear();
        self.window_handles.clear();
        self.flush_effects();
        self.quitting = true;

        let futures = futures::future::join_all(futures);
        if self
            .foreground_executor
            .block_with_timeout(SHUTDOWN_TIMEOUT, futures)
            .is_err()
        {
            log::error!("timed out waiting on app_will_quit");
        }

        self.quitting = false;
    }

    /// Get the id of the current keyboard layout
    pub fn keyboard_layout(&self) -> &dyn PlatformKeyboardLayout {
        self.keyboard_layout.as_ref()
    }

    /// Get the current keyboard mapper.
    pub fn keyboard_mapper(&self) -> &Rc<dyn PlatformKeyboardMapper> {
        &self.keyboard_mapper
    }

    /// Invokes a handler when the current keyboard layout changes
    pub fn on_keyboard_layout_change<F>(&self, mut callback: F) -> Subscription
    where
        F: 'static + FnMut(&mut App),
    {
        let (subscription, activate) = self.keyboard_layout_observers.insert(
            (),
            Box::new(move |cx| {
                callback(cx);
                true
            }),
        );
        activate();
        subscription
    }

    /// Gracefully quit the application via the platform's standard routine.
    pub fn quit(&self) {
        self.platform.quit();
    }

    /// Returns the current policy for hiding the cursor in response to
    /// keyboard input.
    pub fn cursor_hide_mode(&self) -> CursorHideMode {
        self.cursor_hide_mode
    }

    /// Sets the policy controlling when GPUI hides the cursor in response
    /// to keyboard input.
    pub fn set_cursor_hide_mode(&mut self, mode: CursorHideMode) {
        self.cursor_hide_mode = mode;
    }

    /// Returns whether the cursor is currently visible according to the
    /// platform. This will report `false` after a keyboard input has hidden
    /// the cursor and the user has not yet moved the mouse to restore it.
    ///
    /// See [`App::set_cursor_hide_mode`].
    pub fn is_cursor_visible(&self) -> bool {
        self.platform.is_cursor_visible()
    }
}

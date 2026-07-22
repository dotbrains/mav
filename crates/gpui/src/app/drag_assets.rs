use super::*;

impl App {
    /// Is there currently something being dragged?
    pub fn has_active_drag(&self) -> bool {
        self.active_drag.is_some()
    }
    /// Gets the cursor style of the currently active drag operation.
    pub fn active_drag_cursor_style(&self) -> Option<CursorStyle> {
        self.active_drag.as_ref().and_then(|drag| drag.cursor_style)
    }

    /// Stops active drag and clears any related effects.
    pub fn stop_active_drag(&mut self, window: &mut Window) -> bool {
        if self.active_drag.is_some() {
            self.active_drag = None;
            window.refresh();
            true
        } else {
            false
        }
    }

    /// Sets the cursor style for the currently active drag operation.
    pub fn set_active_drag_cursor_style(
        &mut self,
        cursor_style: CursorStyle,
        window: &mut Window,
    ) -> bool {
        if let Some(ref mut drag) = self.active_drag {
            drag.cursor_style = Some(cursor_style);
            window.refresh();
            true
        } else {
            false
        }
    }

    /// Set the prompt renderer for GPUI. This will replace the default or platform specific
    /// prompts with this custom implementation.
    pub fn set_prompt_builder(
        &mut self,
        renderer: impl Fn(
            PromptLevel,
            &str,
            Option<&str>,
            &[PromptButton],
            PromptHandle,
            &mut Window,
            &mut App,
        ) -> RenderablePromptHandle
        + 'static,
    ) {
        self.prompt_builder = Some(PromptBuilder::Custom(Box::new(renderer)));
    }

    /// Reset the prompt builder to the default implementation.
    pub fn reset_prompt_builder(&mut self) {
        self.prompt_builder = Some(PromptBuilder::Default);
    }

    /// Remove an asset from GPUI's cache
    pub fn remove_asset<A: Asset>(&mut self, source: &A::Source) {
        let asset_id = (TypeId::of::<A>(), hash(source));
        self.loading_assets.remove(&asset_id);
    }

    /// Asynchronously load an asset, if the asset hasn't finished loading this will return None.
    ///
    /// Note that the multiple calls to this method will only result in one `Asset::load` call at a
    /// time, and the results of this call will be cached
    pub fn fetch_asset<A: Asset>(&mut self, source: &A::Source) -> (Shared<Task<A::Output>>, bool) {
        let asset_id = (TypeId::of::<A>(), hash(source));
        let mut is_first = false;
        let task = self
            .loading_assets
            .remove(&asset_id)
            .map(|boxed_task| *boxed_task.downcast::<Shared<Task<A::Output>>>().unwrap())
            .unwrap_or_else(|| {
                is_first = true;
                let future = A::load(source.clone(), self);

                self.background_executor().spawn(future).shared()
            });

        self.loading_assets.insert(asset_id, Box::new(task.clone()));

        (task, is_first)
    }

    /// Obtain a new [`FocusHandle`], which allows you to track and manipulate the keyboard focus
    /// for elements rendered within this window.
    #[track_caller]
    pub fn focus_handle(&self) -> FocusHandle {
        FocusHandle::new(&self.focus_handles)
    }

    /// Tell GPUI that an entity has changed and observers of it should be notified.
    pub fn notify(&mut self, entity_id: EntityId) {
        let window_invalidators = mem::take(
            self.window_invalidators_by_entity
                .entry(entity_id)
                .or_default(),
        );

        // `window_invalidators_by_entity` is monotonic, so an entry alone
        // doesn't mean the window is currently rendering the entity. Filter
        // through `tracked_entities` to keep invalidation tight to windows
        // that actually display this entity right now.
        let live_invalidators: SmallVec<[WindowInvalidator; 2]> = window_invalidators
            .iter()
            .filter(|(window_id, _)| {
                self.tracked_entities
                    .get(window_id)
                    .is_some_and(|set| set.contains(&entity_id))
            })
            .map(|(_, invalidator)| invalidator.clone())
            .collect();

        if live_invalidators.is_empty() {
            if self.pending_notifications.insert(entity_id) {
                self.pending_effects
                    .push_back(Effect::Notify { emitter: entity_id });
            }
        } else {
            for invalidator in &live_invalidators {
                invalidator.invalidate_view(entity_id, self);
            }
        }

        self.window_invalidators_by_entity
            .insert(entity_id, window_invalidators);
    }

    /// Returns the name for this [`App`].
    #[cfg(any(test, feature = "test-support", debug_assertions))]
    pub fn get_name(&self) -> Option<&'static str> {
        self.name
    }

    /// Returns `true` if the platform file picker supports selecting a mix of files and directories.
    pub fn can_select_mixed_files_and_dirs(&self) -> bool {
        self.platform.can_select_mixed_files_and_dirs()
    }

    /// Removes an image from the sprite atlas on all windows.
    ///
    /// If the current window is being updated, it will be removed from `App.windows`, you can use `current_window` to specify the current window.
    /// This is a no-op if the image is not in the sprite atlas.
    pub fn drop_image(&mut self, image: Arc<RenderImage>, current_window: Option<&mut Window>) {
        // remove the texture from all other windows
        for window in self.windows.values_mut().flatten() {
            _ = window.drop_image(image.clone());
        }

        // remove the texture from the current window
        if let Some(window) = current_window {
            _ = window.drop_image(image);
        }
    }

    /// Sets the renderer for the inspector.
    #[cfg(any(feature = "inspector", debug_assertions))]
    pub fn set_inspector_renderer(&mut self, f: crate::InspectorRenderer) {
        self.inspector_renderer = Some(f);
    }

    /// Registers a renderer specific to an inspector state.
    #[cfg(any(feature = "inspector", debug_assertions))]
    pub fn register_inspector_element<T: 'static, R: crate::IntoElement>(
        &mut self,
        f: impl 'static + Fn(crate::InspectorElementId, &T, &mut Window, &mut App) -> R,
    ) {
        self.inspector_element_registry.register(f);
    }

    /// Initializes gpui's default colors for the application.
    ///
    /// These colors can be accessed through `cx.default_colors()`.
    pub fn init_colors(&mut self) {
        self.set_global(GlobalColors(Arc::new(Colors::default())));
    }
}

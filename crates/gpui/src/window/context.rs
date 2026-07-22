use super::*;

impl Window {
    /// Schedules the given function to be run at the end of the current effect cycle, allowing entities
    /// that are currently on the stack to be returned to the app.
    pub fn defer(&self, cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
        let handle = self.handle;
        cx.defer(move |cx| {
            handle.update(cx, |_, window, cx| f(window, cx)).ok();
        });
    }

    /// Subscribe to events emitted by a entity.
    /// The entity to which you're subscribing must implement the [`EventEmitter`] trait.
    /// The callback will be invoked a handle to the emitting entity, the event, and a window context for the current window.
    pub fn observe<T: 'static>(
        &mut self,
        observed: &Entity<T>,
        cx: &mut App,
        mut on_notify: impl FnMut(Entity<T>, &mut Window, &mut App) + 'static,
    ) -> Subscription {
        let entity_id = observed.entity_id();
        let observed = observed.downgrade();
        let window_handle = self.handle;
        cx.new_observer(
            entity_id,
            Box::new(move |cx| {
                window_handle
                    .update(cx, |_, window, cx| {
                        if let Some(handle) = observed.upgrade() {
                            on_notify(handle, window, cx);
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false)
            }),
        )
    }

    /// Subscribe to events emitted by a entity.
    /// The entity to which you're subscribing must implement the [`EventEmitter`] trait.
    /// The callback will be invoked a handle to the emitting entity, the event, and a window context for the current window.
    pub fn subscribe<Emitter, Evt>(
        &mut self,
        entity: &Entity<Emitter>,
        cx: &mut App,
        mut on_event: impl FnMut(Entity<Emitter>, &Evt, &mut Window, &mut App) + 'static,
    ) -> Subscription
    where
        Emitter: EventEmitter<Evt>,
        Evt: 'static,
    {
        let entity_id = entity.entity_id();
        let handle = entity.downgrade();
        let window_handle = self.handle;
        cx.new_subscription(
            entity_id,
            (
                TypeId::of::<Evt>(),
                Box::new(move |event, cx| {
                    window_handle
                        .update(cx, |_, window, cx| {
                            if let Some(entity) = handle.upgrade() {
                                let event = event.downcast_ref().expect("invalid event type");
                                on_event(entity, event, window, cx);
                                true
                            } else {
                                false
                            }
                        })
                        .unwrap_or(false)
                }),
            ),
        )
    }

    /// Register a callback to be invoked when the given `Entity` is released.
    pub fn observe_release<T>(
        &self,
        entity: &Entity<T>,
        cx: &mut App,
        mut on_release: impl FnOnce(&mut T, &mut Window, &mut App) + 'static,
    ) -> Subscription
    where
        T: 'static,
    {
        let entity_id = entity.entity_id();
        let window_handle = self.handle;
        let (subscription, activate) = cx.release_listeners.insert(
            entity_id,
            Box::new(move |entity, cx| {
                let entity = entity.downcast_mut().expect("invalid entity type");
                let _ = window_handle.update(cx, |_, window, cx| on_release(entity, window, cx));
            }),
        );
        activate();
        subscription
    }

    /// Creates an [`AsyncWindowContext`], which has a static lifetime and can be held across
    /// await points in async code.
    pub fn to_async(&self, cx: &App) -> AsyncWindowContext {
        AsyncWindowContext::new_context(cx.to_async(), self.handle)
    }

    /// Schedule the given closure to be run directly after the current frame is rendered.
    pub fn on_next_frame(&self, callback: impl FnOnce(&mut Window, &mut App) + 'static) {
        RefCell::borrow_mut(&self.next_frame_callbacks).push(Box::new(callback));
    }

    /// Schedule a frame to be drawn on the next animation frame.
    ///
    /// This is useful for elements that need to animate continuously, such as a video player or an animated GIF.
    /// It will cause the window to redraw on the next frame, even if no other changes have occurred.
    ///
    /// If called from within a view, it will notify that view on the next frame. Otherwise, it will refresh the entire window.
    pub fn request_animation_frame(&self) {
        let entity = self.current_view();
        self.on_next_frame(move |_, cx| cx.notify(entity));
    }

    /// Spawn the future returned by the given closure on the application thread pool.
    /// The closure is provided a handle to the current window and an `AsyncWindowContext` for
    /// use within your future.
    #[track_caller]
    pub fn spawn<AsyncFn, R>(&self, cx: &App, f: AsyncFn) -> Task<R>
    where
        R: 'static,
        AsyncFn: AsyncFnOnce(&mut AsyncWindowContext) -> R + 'static,
    {
        let handle = self.handle;
        cx.spawn(async move |app| {
            let mut async_window_cx = AsyncWindowContext::new_context(app.clone(), handle);
            f(&mut async_window_cx).await
        })
    }

    /// Spawn the future returned by the given closure on the application thread
    /// pool, with the given priority. The closure is provided a handle to the
    /// current window and an `AsyncWindowContext` for use within your future.
    #[track_caller]
    pub fn spawn_with_priority<AsyncFn, R>(
        &self,
        priority: Priority,
        cx: &App,
        f: AsyncFn,
    ) -> Task<R>
    where
        R: 'static,
        AsyncFn: AsyncFnOnce(&mut AsyncWindowContext) -> R + 'static,
    {
        let handle = self.handle;
        cx.spawn_with_priority(priority, async move |app| {
            let mut async_window_cx = AsyncWindowContext::new_context(app.clone(), handle);
            f(&mut async_window_cx).await
        })
    }
}

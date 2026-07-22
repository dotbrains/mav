use super::*;

impl App {
    /// Schedules all windows in the application to be redrawn. This can be called
    /// multiple times in an update cycle and still result in a single redraw.
    pub fn refresh_windows(&mut self) {
        self.pending_effects.push_back(Effect::RefreshWindows);
    }
    pub(crate) fn update<R>(&mut self, update: impl FnOnce(&mut Self) -> R) -> R {
        self.start_update();
        let result = update(self);
        self.finish_update();
        result
    }

    pub(crate) fn start_update(&mut self) {
        self.pending_updates += 1;
    }

    pub(crate) fn finish_update(&mut self) {
        if !self.flushing_effects && self.pending_updates == 1 {
            self.flushing_effects = true;
            self.flush_effects();
            self.flushing_effects = false;
        }
        self.pending_updates -= 1;
    }

    /// Arrange a callback to be invoked when the given entity calls `notify` on its respective context.
    pub fn observe<W>(
        &mut self,
        entity: &Entity<W>,
        mut on_notify: impl FnMut(Entity<W>, &mut App) + 'static,
    ) -> Subscription
    where
        W: 'static,
    {
        self.observe_internal(entity, move |e, cx| {
            on_notify(e, cx);
            true
        })
    }

    pub(crate) fn detect_accessed_entities<R>(
        &mut self,
        callback: impl FnOnce(&mut App) -> R,
    ) -> (R, FxHashSet<EntityId>) {
        let accessed_entities_start = self.entities.accessed_entities.get_mut().clone();
        let result = callback(self);
        let entities_accessed_in_callback = self
            .entities
            .accessed_entities
            .get_mut()
            .difference(&accessed_entities_start)
            .copied()
            .collect::<FxHashSet<EntityId>>();
        (result, entities_accessed_in_callback)
    }

    pub(crate) fn record_entities_accessed(
        &mut self,
        window_handle: AnyWindowHandle,
        invalidator: WindowInvalidator,
        entities: &FxHashSet<EntityId>,
    ) {
        let mut tracked_entities =
            std::mem::take(self.tracked_entities.entry(window_handle.id).or_default());
        for entity in tracked_entities.iter() {
            self.window_invalidators_by_entity
                .entry(*entity)
                .and_modify(|windows| {
                    windows.remove(&window_handle.id);
                });
        }
        for entity in entities.iter() {
            self.window_invalidators_by_entity
                .entry(*entity)
                .or_default()
                .insert(window_handle.id, invalidator.clone());
            self.current_window_by_entity
                .insert(*entity, window_handle.id);
        }
        tracked_entities.clear();
        tracked_entities.extend(entities.iter().copied());
        self.tracked_entities
            .insert(window_handle.id, tracked_entities);
    }

    pub(crate) fn new_observer(&mut self, key: EntityId, value: Handler) -> Subscription {
        let (subscription, activate) = self.observers.insert(key, value);
        self.defer(move |_| activate());
        subscription
    }

    pub(crate) fn observe_internal<W>(
        &mut self,
        entity: &Entity<W>,
        mut on_notify: impl FnMut(Entity<W>, &mut App) -> bool + 'static,
    ) -> Subscription
    where
        W: 'static,
    {
        let entity_id = entity.entity_id();
        let handle = entity.downgrade();
        self.new_observer(
            entity_id,
            Box::new(move |cx| {
                if let Some(entity) = handle.upgrade() {
                    on_notify(entity, cx)
                } else {
                    false
                }
            }),
        )
    }

    /// Arrange for the given callback to be invoked whenever the given entity emits an event of a given type.
    /// The callback is provided a handle to the emitting entity and a reference to the emitted event.
    pub fn subscribe<T, Event>(
        &mut self,
        entity: &Entity<T>,
        mut on_event: impl FnMut(Entity<T>, &Event, &mut App) + 'static,
    ) -> Subscription
    where
        T: 'static + EventEmitter<Event>,
        Event: 'static,
    {
        self.subscribe_internal(entity, move |entity, event, cx| {
            on_event(entity, event, cx);
            true
        })
    }

    pub(crate) fn new_subscription(
        &mut self,
        key: EntityId,
        value: (TypeId, Listener),
    ) -> Subscription {
        let (subscription, activate) = self.event_listeners.insert(key, value);
        self.defer(move |_| activate());
        subscription
    }
    pub(crate) fn subscribe_internal<T, Evt>(
        &mut self,
        entity: &Entity<T>,
        mut on_event: impl FnMut(Entity<T>, &Evt, &mut App) -> bool + 'static,
    ) -> Subscription
    where
        T: 'static + EventEmitter<Evt>,
        Evt: 'static,
    {
        let entity_id = entity.entity_id();
        let handle = entity.downgrade();
        self.new_subscription(
            entity_id,
            (
                TypeId::of::<Evt>(),
                Box::new(move |event, cx| {
                    let event: &Evt = event.downcast_ref().expect("invalid event type");
                    if let Some(entity) = handle.upgrade() {
                        on_event(entity, event, cx)
                    } else {
                        false
                    }
                }),
            ),
        )
    }
}

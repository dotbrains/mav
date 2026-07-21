use super::*;

impl MultiBuffer {
    pub fn new(capability: Capability) -> Self {
        Self::new_(
            capability,
            MultiBufferSnapshot {
                show_headers: true,
                show_deleted_hunks: true,
                ..MultiBufferSnapshot::default()
            },
        )
    }

    pub fn without_headers(capability: Capability) -> Self {
        Self::new_(
            capability,
            MultiBufferSnapshot {
                show_deleted_hunks: true,
                ..MultiBufferSnapshot::default()
            },
        )
    }

    pub fn singleton(buffer: Entity<Buffer>, cx: &mut Context<Self>) -> Self {
        let mut this = Self::new_(
            buffer.read(cx).capability(),
            MultiBufferSnapshot {
                singleton: true,
                show_deleted_hunks: true,
                ..MultiBufferSnapshot::default()
            },
        );
        this.singleton = true;
        this.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer.clone(),
            [Point::zero()..buffer.read(cx).max_point()],
            0,
            cx,
        );
        this
    }

    #[inline]
    pub fn new_(capability: Capability, snapshot: MultiBufferSnapshot) -> Self {
        Self {
            snapshot: RefCell::new(snapshot),
            buffers: Default::default(),
            diffs: HashMap::default(),
            subscriptions: Topic::default(),
            singleton: false,
            capability,
            title: None,
            buffer_changed_since_sync: Default::default(),
            history: History::default(),
        }
    }

    pub fn clone(&self, new_cx: &mut Context<Self>) -> Self {
        let mut buffers = BTreeMap::default();
        let buffer_changed_since_sync = Rc::new(Cell::new(false));
        for (buffer_id, buffer_state) in self.buffers.iter() {
            buffer_state.buffer.update(new_cx, |buffer, _| {
                buffer.record_changes(Rc::downgrade(&buffer_changed_since_sync));
            });
            buffers.insert(
                *buffer_id,
                BufferState {
                    buffer: buffer_state.buffer.clone(),
                    _subscriptions: [
                        new_cx.observe(&buffer_state.buffer, |_, _, cx| cx.notify()),
                        new_cx.subscribe(&buffer_state.buffer, Self::on_buffer_event),
                    ],
                },
            );
        }
        let mut diff_bases = HashMap::default();
        for (buffer_id, diff) in self.diffs.iter() {
            diff_bases.insert(*buffer_id, DiffState::new(diff.diff.clone(), new_cx));
        }
        Self {
            snapshot: RefCell::new(self.snapshot.borrow().clone()),
            buffers,
            diffs: diff_bases,
            subscriptions: Default::default(),
            singleton: self.singleton,
            capability: self.capability,
            history: self.history.clone(),
            title: self.title.clone(),
            buffer_changed_since_sync,
        }
    }

    pub fn set_group_interval(&mut self, group_interval: Duration, cx: &mut Context<Self>) {
        self.history.set_group_interval(group_interval);
        if self.singleton {
            for BufferState { buffer, .. } in self.buffers.values() {
                buffer.update(cx, |buffer, _| {
                    buffer.set_group_interval(group_interval);
                });
            }
        }
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    pub fn read_only(&self) -> bool {
        !self.capability.editable()
    }

    pub fn capability(&self) -> Capability {
        self.capability
    }

    /// Returns an up-to-date snapshot of the MultiBuffer.
    #[ztracing::instrument(skip_all)]
    pub fn snapshot(&self, cx: &App) -> MultiBufferSnapshot {
        self.sync(cx);
        self.snapshot.borrow().clone()
    }

    pub fn read(&self, cx: &App) -> Ref<'_, MultiBufferSnapshot> {
        self.sync(cx);
        self.snapshot.borrow()
    }

    pub fn as_singleton(&self) -> Option<Entity<Buffer>> {
        if self.singleton {
            Some(self.buffers.values().next().unwrap().buffer.clone())
        } else {
            None
        }
    }

    pub fn is_singleton(&self) -> bool {
        self.singleton
    }

    pub fn subscribe(&mut self) -> Subscription<MultiBufferOffset> {
        self.subscriptions.subscribe()
    }

    pub fn is_dirty(&self, cx: &App) -> bool {
        self.read(cx).is_dirty()
    }

    pub fn has_deleted_file(&self, cx: &App) -> bool {
        self.read(cx).has_deleted_file()
    }

    pub fn has_conflict(&self, cx: &App) -> bool {
        self.read(cx).has_conflict()
    }

    // The `is_empty` signature doesn't match what clippy expects
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self, cx: &App) -> MultiBufferOffset {
        self.read(cx).len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }
}

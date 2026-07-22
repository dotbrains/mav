use super::*;

impl BufferDiff {
    pub fn new(
        buffer: &text::BufferSnapshot,
        language: Option<Arc<Language>>,
        language_registry: Option<Arc<LanguageRegistry>>,
        cx: &mut App,
    ) -> Self {
        let base_text = cx.new(|cx| {
            let mut base_buffer = language::Buffer::local("", cx);
            base_buffer.set_capability(Capability::ReadOnly, cx);
            if let Some(language_registry) = language_registry {
                base_buffer.set_language_registry(language_registry);
            }
            base_buffer.set_language_async(language, cx);
            base_buffer
        });

        BufferDiff {
            buffer_id: buffer.remote_id(),
            base_text_buffer: base_text,
            diff_snapshot: None,
            buffer_snapshot: buffer.clone(),
            secondary_diff: None,
        }
    }

    pub fn new_with_base_text_buffer(
        buffer: &text::BufferSnapshot,
        base_text_buffer: Entity<language::Buffer>,
        _cx: &mut App,
    ) -> Self {
        BufferDiff {
            buffer_id: buffer.remote_id(),
            base_text_buffer,
            diff_snapshot: None,
            buffer_snapshot: buffer.clone(),
            secondary_diff: None,
        }
    }

    pub fn new_unchanged(
        buffer: &text::BufferSnapshot,
        language: Option<Arc<Language>>,
        language_registry: Option<Arc<LanguageRegistry>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let base_text = buffer.text();
        let base_text = cx.new(|cx| {
            let mut base_buffer = language::Buffer::local(base_text, cx);
            base_buffer.set_capability(Capability::ReadOnly, cx);
            if let Some(language_registry) = language_registry {
                base_buffer.set_language_registry(language_registry);
            }
            base_buffer.set_language_async(language, cx);
            base_buffer
        });

        let base_text_snapshot = base_text.read(cx).snapshot();

        let diff_snapshot = BufferDiffSnapshot {
            hunks: SumTree::new(buffer),
            pending_hunks: SumTree::new(buffer),
            base_text: base_text_snapshot,
            base_text_exists: true,
            buffer_snapshot: buffer.clone(),
            secondary_diff: None,
        };

        BufferDiff {
            buffer_id: buffer.remote_id(),
            base_text_buffer: base_text,
            diff_snapshot: Some(diff_snapshot),
            buffer_snapshot: buffer.clone(),
            secondary_diff: None,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn new_with_base_text(
        base_text: &str,
        buffer: &text::BufferSnapshot,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = BufferDiff::new(buffer, None, None, cx);
        let mut base_text = base_text.to_owned();
        text::LineEnding::normalize(&mut base_text);
        let base_text_buffer = cx.new(|cx| {
            let mut buffer = language::Buffer::local(base_text, cx);
            buffer.set_capability(Capability::ReadOnly, cx);
            buffer
        });
        let base_text = base_text_buffer.read(cx).snapshot();
        this.base_text_buffer = base_text_buffer;
        let update = cx.foreground_executor().block_on(this.update_diff(
            buffer.clone(),
            &base_text,
            Some(Arc::from(base_text.text())),
            cx,
        ));
        this.set_snapshot(update, cx);
        this
    }

    pub fn set_secondary_diff(&mut self, diff: Entity<BufferDiff>) {
        self.secondary_diff = Some(diff);
    }

    pub fn secondary_diff(&self) -> Option<Entity<BufferDiff>> {
        self.secondary_diff.clone()
    }

    pub fn clear_pending_hunks(&mut self, cx: &mut Context<Self>) {
        let Some(diff_snapshot) = &mut self.diff_snapshot else {
            return;
        };
        if self.secondary_diff.is_some() {
            diff_snapshot.pending_hunks = SumTree::from_summary(DiffHunkSummary {
                buffer_range: Anchor::min_min_range_for_buffer(self.buffer_id),
                diff_base_byte_range: 0..0,
                added_rows: 0,
                removed_rows: 0,
            });
            let changed_range = Some(Anchor::min_max_range_for_buffer(self.buffer_id));
            let base_text_range = Some(0..self.base_text(cx).len());
            cx.emit(BufferDiffEvent::DiffChanged(DiffChanged {
                changed_range: changed_range.clone(),
                base_text_changed_range: base_text_range,
                extended_range: changed_range,
                base_text_changed: false,
            }));
        }
    }
}

use super::*;

impl Buffer {
    #[ztracing::instrument(skip_all)]
    pub fn preview_edits(
        &self,
        edits: Arc<[(Range<Anchor>, Arc<str>)]>,
        cx: &App,
    ) -> Task<EditPreview> {
        let registry = self.language_registry();
        let language = self.language().cloned();
        let old_snapshot = self.text.snapshot().clone();
        let mut branch_buffer = self.text.branch();
        let mut syntax_snapshot = self.syntax_map.lock().snapshot();
        cx.background_spawn(async move {
            if !edits.is_empty() {
                if let Some(language) = language.clone() {
                    syntax_snapshot.reparse(&old_snapshot, registry.clone(), language);
                }

                branch_buffer.edit(edits.iter().cloned());
                let snapshot = branch_buffer.snapshot();
                syntax_snapshot.interpolate(&snapshot);

                if let Some(language) = language {
                    syntax_snapshot.reparse(&snapshot, registry, language);
                }
            }
            EditPreview::new(old_snapshot, branch_buffer.into_snapshot(), syntax_snapshot)
        })
    }

    /// Applies all of the changes in this buffer that intersect any of the
    /// given `ranges` to its base buffer.
    ///
    /// If `ranges` is empty, then all changes will be applied. This buffer must
    /// be a branch buffer to call this method.
    pub fn merge_into_base(&mut self, ranges: Vec<Range<usize>>, cx: &mut Context<Self>) {
        let Some(base_buffer) = self.base_buffer() else {
            debug_panic!("not a branch buffer");
            return;
        };

        let mut ranges = if ranges.is_empty() {
            &[0..usize::MAX]
        } else {
            ranges.as_slice()
        }
        .iter()
        .peekable();

        let mut edits = Vec::new();
        for edit in self.edits_since::<usize>(&base_buffer.read(cx).version()) {
            let mut is_included = false;
            while let Some(range) = ranges.peek() {
                if range.end < edit.new.start {
                    ranges.next().unwrap();
                } else {
                    if range.start <= edit.new.end {
                        is_included = true;
                    }
                    break;
                }
            }

            if is_included {
                edits.push((
                    edit.old.clone(),
                    self.text_for_range(edit.new.clone()).collect::<String>(),
                ));
            }
        }

        let operation = base_buffer.update(cx, |base_buffer, cx| {
            // cx.emit(BufferEvent::DiffBaseChanged);
            base_buffer.edit(edits, None, cx)
        });

        if let Some(operation) = operation
            && let Some(BufferBranchState {
                merged_operations, ..
            }) = &mut self.branch_state
        {
            merged_operations.push(operation);
        }
    }

    pub(super) fn on_base_buffer_event(
        &mut self,
        _: Entity<Buffer>,
        event: &BufferEvent,
        cx: &mut Context<Self>,
    ) {
        let BufferEvent::Operation { operation, .. } = event else {
            return;
        };
        let Some(BufferBranchState {
            merged_operations, ..
        }) = &mut self.branch_state
        else {
            return;
        };

        let mut operation_to_undo = None;
        if let Operation::Buffer(text::Operation::Edit(operation)) = &operation
            && let Ok(ix) = merged_operations.binary_search(&operation.timestamp)
        {
            merged_operations.remove(ix);
            operation_to_undo = Some(operation.timestamp);
        }

        self.apply_ops([operation.clone()], cx);

        if let Some(timestamp) = operation_to_undo {
            let counts = [(timestamp, u32::MAX)].into_iter().collect();
            self.undo_operations(counts, cx);
        }
    }
}

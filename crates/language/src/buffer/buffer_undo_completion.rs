use super::*;

impl Buffer {
    pub fn undo(&mut self, cx: &mut Context<Self>) -> Option<TransactionId> {
        let was_dirty = self.is_dirty();
        let old_version = self.version.clone();

        if let Some((transaction_id, operation)) = self.text.undo() {
            self.send_operation(Operation::Buffer(operation), true, cx);
            self.did_edit(&old_version, was_dirty, BufferEditSource::User, cx);
            self.restore_encoding_for_transaction(transaction_id, was_dirty);
            Some(transaction_id)
        } else {
            None
        }
    }

    /// Manually undoes a specific transaction in the buffer's undo history.
    pub fn undo_transaction(
        &mut self,
        transaction_id: TransactionId,
        cx: &mut Context<Self>,
    ) -> bool {
        let was_dirty = self.is_dirty();
        let old_version = self.version.clone();
        if let Some(operation) = self.text.undo_transaction(transaction_id) {
            self.send_operation(Operation::Buffer(operation), true, cx);
            self.did_edit(&old_version, was_dirty, BufferEditSource::User, cx);
            true
        } else {
            false
        }
    }

    /// Manually undoes all changes after a given transaction in the buffer's undo history.
    pub fn undo_to_transaction(
        &mut self,
        transaction_id: TransactionId,
        cx: &mut Context<Self>,
    ) -> bool {
        let was_dirty = self.is_dirty();
        let old_version = self.version.clone();

        let operations = self.text.undo_to_transaction(transaction_id);
        let undone = !operations.is_empty();
        for operation in operations {
            self.send_operation(Operation::Buffer(operation), true, cx);
        }
        if undone {
            self.did_edit(&old_version, was_dirty, BufferEditSource::User, cx)
        }
        undone
    }

    pub fn undo_operations(&mut self, counts: HashMap<Lamport, u32>, cx: &mut Context<Buffer>) {
        let was_dirty = self.is_dirty();
        let operation = self.text.undo_operations(counts);
        let old_version = self.version.clone();
        self.send_operation(Operation::Buffer(operation), true, cx);
        self.did_edit(&old_version, was_dirty, BufferEditSource::User, cx);
    }

    /// Manually redoes a specific transaction in the buffer's redo history.
    pub fn redo(&mut self, cx: &mut Context<Self>) -> Option<TransactionId> {
        let was_dirty = self.is_dirty();
        let old_version = self.version.clone();

        if let Some((transaction_id, operation)) = self.text.redo() {
            self.send_operation(Operation::Buffer(operation), true, cx);
            self.did_edit(&old_version, was_dirty, BufferEditSource::User, cx);
            self.restore_encoding_for_transaction(transaction_id, was_dirty);
            Some(transaction_id)
        } else {
            None
        }
    }

    fn restore_encoding_for_transaction(&mut self, transaction_id: TransactionId, was_dirty: bool) {
        if let Some((old_encoding, old_has_bom)) =
            self.reload_with_encoding_txns.get(&transaction_id)
        {
            let current_encoding = self.encoding;
            let current_has_bom = self.has_bom;
            self.encoding = *old_encoding;
            self.has_bom = *old_has_bom;
            if !was_dirty {
                self.saved_version = self.version.clone();
                self.has_unsaved_edits
                    .set((self.saved_version.clone(), false));
            }
            self.reload_with_encoding_txns
                .insert(transaction_id, (current_encoding, current_has_bom));
        }
    }

    /// Manually undoes all changes until a given transaction in the buffer's redo history.
    pub fn redo_to_transaction(
        &mut self,
        transaction_id: TransactionId,
        cx: &mut Context<Self>,
    ) -> bool {
        let was_dirty = self.is_dirty();
        let old_version = self.version.clone();

        let operations = self.text.redo_to_transaction(transaction_id);
        let redone = !operations.is_empty();
        for operation in operations {
            self.send_operation(Operation::Buffer(operation), true, cx);
        }
        if redone {
            self.did_edit(&old_version, was_dirty, BufferEditSource::User, cx)
        }
        redone
    }

    /// Override current completion triggers with the user-provided completion triggers.
    pub fn set_completion_triggers(
        &mut self,
        server_id: LanguageServerId,
        triggers: BTreeSet<String>,
        cx: &mut Context<Self>,
    ) {
        self.completion_triggers_timestamp = self.text.lamport_clock.tick();
        if triggers.is_empty() {
            self.completion_triggers_per_language_server
                .remove(&server_id);
            self.completion_triggers = self
                .completion_triggers_per_language_server
                .values()
                .flat_map(|triggers| triggers.iter().cloned())
                .collect();
        } else {
            self.completion_triggers_per_language_server
                .insert(server_id, triggers.clone());
            self.completion_triggers.extend(triggers.iter().cloned());
        }
        self.send_operation(
            Operation::UpdateCompletionTriggers {
                triggers: triggers.into_iter().collect(),
                lamport_timestamp: self.completion_triggers_timestamp,
                server_id,
            },
            true,
            cx,
        );
        cx.notify();
    }

    /// Returns a list of strings which trigger a completion menu for this language.
    /// Usually this is driven by LSP server which returns a list of trigger characters for completions.
    pub fn completion_triggers(&self) -> &BTreeSet<String> {
        &self.completion_triggers
    }

    /// Call this directly after performing edits to prevent the preview tab
    /// from being dismissed by those edits. It causes `should_dismiss_preview`
    /// to return false until there are additional edits.
    pub fn refresh_preview(&mut self) {
        self.preview_version = self.version.clone();
    }

    /// Whether we should preserve the preview status of a tab containing this buffer.
    pub fn preserve_preview(&self) -> bool {
        !self.has_edits_since(&self.preview_version)
    }

    pub fn set_group_interval(&mut self, group_interval: Duration) {
        self.text.set_group_interval(group_interval);
    }

    // TODO: see if ep can use this instead of Buffer::branch
    pub fn snapshot_with_edits<I, S, T>(
        &mut self,
        edits: I,
        cx: &mut Context<Self>,
    ) -> Task<EditedBufferSnapshot>
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        let mut snapshot = self.snapshot();
        let text = snapshot.text.clone();
        let mut syntax = snapshot.syntax.clone();
        let language = self.language().cloned();
        let registry = self.language_registry();
        let new_text = self.text.snapshot_with_edits(edits);
        cx.background_spawn(async move {
            if let Some(language) = language.clone() {
                syntax.reparse(&text, registry.clone(), language);
            }

            syntax.interpolate(&new_text.snapshot);

            if let Some(language) = language {
                syntax.reparse(&new_text.snapshot, registry, language);
            }

            snapshot.text = new_text.snapshot.clone();
            snapshot.syntax = syntax;

            EditedBufferSnapshot {
                text: new_text,
                snapshot,
            }
        })
    }

    pub fn fast_forward(&mut self, edited: EditedBufferSnapshot, cx: &mut Context<Self>) {
        let base_version = edited.text.base_version.clone();
        let did_edit = edited.text.did_edit;
        self.text.fast_forward(edited.text);
        if edited.snapshot.language == self.language {
            self.reparse = None;
            self.did_finish_parsing(edited.snapshot.syntax, None, cx);
            if did_edit {
                cx.emit(BufferEvent::Edited {
                    source: BufferEditSource::User,
                });
            }
        } else {
            self.did_edit(&base_version, false, BufferEditSource::User, cx);
        }
    }
}

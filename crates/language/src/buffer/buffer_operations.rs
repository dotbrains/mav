use super::*;

impl Buffer {
    pub fn autoindent_ranges<I, T>(&mut self, ranges: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = Range<T>>,
        T: ToOffset + Copy,
    {
        let before_edit = self.snapshot();
        let entries = ranges
            .into_iter()
            .map(|range| AutoindentRequestEntry {
                range: before_edit.anchor_before(range.start)..before_edit.anchor_after(range.end),
                old_row: None,
                indent_size: before_edit.language_indent_size_at(range.start, cx),
                original_indent_column: None,
            })
            .collect();
        self.autoindent_requests.push(Arc::new(AutoindentRequest {
            before_edit,
            entries,
            is_block_mode: false,
            ignore_empty_lines: true,
        }));
        self.request_autoindent(cx, Some(Duration::from_micros(300)));
    }

    // Inserts newlines at the given position to create an empty line, returning the start of the new line.
    // You can also request the insertion of empty lines above and below the line starting at the returned point.
    pub fn insert_empty_line(
        &mut self,
        position: impl ToPoint,
        space_above: bool,
        space_below: bool,
        cx: &mut Context<Self>,
    ) -> Point {
        let mut position = position.to_point(self);

        self.start_transaction();

        self.edit(
            [(position..position, "\n")],
            Some(AutoindentMode::EachLine),
            cx,
        );

        if position.column > 0 {
            position += Point::new(1, 0);
        }

        if !self.is_line_blank(position.row) {
            self.edit(
                [(position..position, "\n")],
                Some(AutoindentMode::EachLine),
                cx,
            );
        }

        if space_above && position.row > 0 && !self.is_line_blank(position.row - 1) {
            self.edit(
                [(position..position, "\n")],
                Some(AutoindentMode::EachLine),
                cx,
            );
            position.row += 1;
        }

        if space_below
            && (position.row == self.max_point().row || !self.is_line_blank(position.row + 1))
        {
            self.edit(
                [(position..position, "\n")],
                Some(AutoindentMode::EachLine),
                cx,
            );
        }

        self.end_transaction(cx);

        position
    }

    /// Applies the given remote operations to the buffer.
    pub fn apply_ops<I: IntoIterator<Item = Operation>>(&mut self, ops: I, cx: &mut Context<Self>) {
        self.pending_autoindent.take();
        let was_dirty = self.is_dirty();
        let old_version = self.version.clone();
        let mut deferred_ops = Vec::new();
        let buffer_ops = ops
            .into_iter()
            .filter_map(|op| match op {
                Operation::Buffer(op) => Some(op),
                _ => {
                    if self.can_apply_op(&op) {
                        self.apply_op(op, cx);
                    } else {
                        deferred_ops.push(op);
                    }
                    None
                }
            })
            .collect::<Vec<_>>();
        for operation in buffer_ops.iter() {
            self.send_operation(Operation::Buffer(operation.clone()), false, cx);
        }
        self.text.apply_ops(buffer_ops);
        self.deferred_ops.insert(deferred_ops);
        self.flush_deferred_ops(cx);
        self.did_edit(&old_version, was_dirty, BufferEditSource::Remote, cx);
        // Notify independently of whether the buffer was edited as the operations could include a
        // selection update.
        cx.notify();
    }

    fn flush_deferred_ops(&mut self, cx: &mut Context<Self>) {
        let mut deferred_ops = Vec::new();
        for op in self.deferred_ops.drain().iter().cloned() {
            if self.can_apply_op(&op) {
                self.apply_op(op, cx);
            } else {
                deferred_ops.push(op);
            }
        }
        self.deferred_ops.insert(deferred_ops);
    }

    pub fn has_deferred_ops(&self) -> bool {
        !self.deferred_ops.is_empty() || self.text.has_deferred_ops()
    }

    fn can_apply_op(&self, operation: &Operation) -> bool {
        match operation {
            Operation::Buffer(_) => {
                unreachable!("buffer operations should never be applied at this layer")
            }
            Operation::UpdateDiagnostics {
                diagnostics: diagnostic_set,
                ..
            } => diagnostic_set.iter().all(|diagnostic| {
                self.text.can_resolve(&diagnostic.range.start)
                    && self.text.can_resolve(&diagnostic.range.end)
            }),
            Operation::UpdateSelections { selections, .. } => selections
                .iter()
                .all(|s| self.can_resolve(&s.start) && self.can_resolve(&s.end)),
            Operation::UpdateCompletionTriggers { .. } | Operation::UpdateLineEnding { .. } => true,
        }
    }

    fn apply_op(&mut self, operation: Operation, cx: &mut Context<Self>) {
        match operation {
            Operation::Buffer(_) => {
                unreachable!("buffer operations should never be applied at this layer")
            }
            Operation::UpdateDiagnostics {
                server_id,
                diagnostics: diagnostic_set,
                lamport_timestamp,
            } => {
                let snapshot = self.snapshot();
                self.apply_diagnostic_update(
                    server_id,
                    DiagnosticSet::from_sorted_entries(diagnostic_set.iter().cloned(), &snapshot),
                    lamport_timestamp,
                    cx,
                );
            }
            Operation::UpdateSelections {
                selections,
                lamport_timestamp,
                line_mode,
                cursor_shape,
            } => {
                if let Some(set) = self.remote_selections.get(&lamport_timestamp.replica_id)
                    && set.lamport_timestamp > lamport_timestamp
                {
                    return;
                }

                self.remote_selections.insert(
                    lamport_timestamp.replica_id,
                    SelectionSet {
                        selections,
                        lamport_timestamp,
                        line_mode,
                        cursor_shape,
                    },
                );
                self.text.lamport_clock.observe(lamport_timestamp);
                self.non_text_state_update_count += 1;
            }
            Operation::UpdateCompletionTriggers {
                triggers,
                lamport_timestamp,
                server_id,
            } => {
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
                        .insert(server_id, triggers.iter().cloned().collect());
                    self.completion_triggers.extend(triggers);
                }
                self.text.lamport_clock.observe(lamport_timestamp);
            }
            Operation::UpdateLineEnding {
                line_ending,
                lamport_timestamp,
            } => {
                self.text.set_line_ending(line_ending);
                self.text.lamport_clock.observe(lamport_timestamp);
            }
        }
    }

    pub(super) fn apply_diagnostic_update(
        &mut self,
        server_id: LanguageServerId,
        diagnostics: DiagnosticSet,
        lamport_timestamp: clock::Lamport,
        cx: &mut Context<Self>,
    ) {
        if lamport_timestamp > self.diagnostics_timestamp {
            if diagnostics.is_empty() {
                self.diagnostics.remove(&server_id);
            } else {
                self.diagnostics.insert(server_id, diagnostics);
            }
            self.diagnostics_timestamp = lamport_timestamp;
            self.non_text_state_update_count += 1;
            self.text.lamport_clock.observe(lamport_timestamp);
            cx.notify();
            cx.emit(BufferEvent::DiagnosticsUpdated);
        }
    }

    pub(super) fn send_operation(
        &mut self,
        operation: Operation,
        is_local: bool,
        cx: &mut Context<Self>,
    ) {
        self.was_changed();
        cx.emit(BufferEvent::Operation {
            operation,
            is_local,
        });
    }

    /// Removes the selections for a given peer.
    pub fn remove_peer(&mut self, replica_id: ReplicaId, cx: &mut Context<Self>) {
        self.remote_selections.remove(&replica_id);
        cx.notify();
    }
}

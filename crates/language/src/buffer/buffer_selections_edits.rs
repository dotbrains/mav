use super::*;

impl Buffer {
    pub fn set_active_selections(
        &mut self,
        selections: Arc<[Selection<Anchor>]>,
        line_mode: bool,
        cursor_shape: CursorShape,
        cx: &mut Context<Self>,
    ) {
        let lamport_timestamp = self.text.lamport_clock.tick();
        self.remote_selections.insert(
            self.text.replica_id(),
            SelectionSet {
                selections: selections.clone(),
                lamport_timestamp,
                line_mode,
                cursor_shape,
            },
        );
        self.send_operation(
            Operation::UpdateSelections {
                selections,
                line_mode,
                lamport_timestamp,
                cursor_shape,
            },
            true,
            cx,
        );
        self.non_text_state_update_count += 1;
        cx.notify();
    }

    /// Clears the selections, so that other replicas of the buffer do not see any selections for
    /// this replica.
    pub fn remove_active_selections(&mut self, cx: &mut Context<Self>) {
        if self
            .remote_selections
            .get(&self.text.replica_id())
            .is_none_or(|set| !set.selections.is_empty())
        {
            self.set_active_selections(Arc::default(), false, Default::default(), cx);
        }
    }

    pub fn set_agent_selections(
        &mut self,
        selections: Arc<[Selection<Anchor>]>,
        line_mode: bool,
        cursor_shape: CursorShape,
        cx: &mut Context<Self>,
    ) {
        let lamport_timestamp = self.text.lamport_clock.tick();
        self.remote_selections.insert(
            ReplicaId::AGENT,
            SelectionSet {
                selections,
                lamport_timestamp,
                line_mode,
                cursor_shape,
            },
        );
        self.non_text_state_update_count += 1;
        cx.notify();
    }

    pub fn remove_agent_selections(&mut self, cx: &mut Context<Self>) {
        self.set_agent_selections(Arc::default(), false, Default::default(), cx);
    }

    /// Replaces the buffer's entire text.
    pub fn set_text<T>(&mut self, text: T, cx: &mut Context<Self>) -> Option<clock::Lamport>
    where
        T: Into<Arc<str>>,
    {
        self.autoindent_requests.clear();
        self.edit([(0..self.len(), text)], None, cx)
    }

    /// Appends the given text to the end of the buffer.
    pub fn append<T>(&mut self, text: T, cx: &mut Context<Self>) -> Option<clock::Lamport>
    where
        T: Into<Arc<str>>,
    {
        self.edit([(self.len()..self.len(), text)], None, cx)
    }

    /// Applies the given edits to the buffer. Each edit is specified as a range of text to
    /// delete, and a string of text to insert at that location. Adjacent edits are coalesced.
    /// Inserted text is normalized to LF line endings before being applied.
    ///
    /// If an [`AutoindentMode`] is provided, then the buffer will enqueue an auto-indent
    /// request for the edited ranges, which will be processed when the buffer finishes
    /// parsing.
    ///
    /// Parsing takes place at the end of a transaction, and may compute synchronously
    /// or asynchronously, depending on the changes.
    pub fn edit<I, S, T>(
        &mut self,
        edits_iter: I,
        autoindent_mode: Option<AutoindentMode>,
        cx: &mut Context<Self>,
    ) -> Option<clock::Lamport>
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        self.edit_internal(edits_iter, autoindent_mode, true, cx)
    }

    /// Like [`edit`](Self::edit), but does not coalesce adjacent edits.
    pub fn edit_non_coalesce<I, S, T>(
        &mut self,
        edits_iter: I,
        autoindent_mode: Option<AutoindentMode>,
        cx: &mut Context<Self>,
    ) -> Option<clock::Lamport>
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        self.edit_internal(edits_iter, autoindent_mode, false, cx)
    }

    fn edit_internal<I, S, T>(
        &mut self,
        edits_iter: I,
        autoindent_mode: Option<AutoindentMode>,
        coalesce_adjacent: bool,
        cx: &mut Context<Self>,
    ) -> Option<clock::Lamport>
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        // Skip invalid edits and coalesce contiguous ones.
        let mut edits: Vec<(Range<usize>, Arc<str>)> = Vec::new();

        for (range, new_text) in edits_iter {
            let mut range = range.start.to_offset(self)..range.end.to_offset(self);

            if range.start > range.end {
                mem::swap(&mut range.start, &mut range.end);
            }
            let new_text = new_text.into();
            if !new_text.is_empty() || !range.is_empty() {
                let prev_edit = edits.last_mut();
                let should_coalesce = prev_edit.as_ref().is_some_and(|(prev_range, _)| {
                    if coalesce_adjacent {
                        prev_range.end >= range.start
                    } else {
                        prev_range.end > range.start
                    }
                });

                if let Some((prev_range, prev_text)) = prev_edit
                    && should_coalesce
                {
                    prev_range.end = cmp::max(prev_range.end, range.end);
                    *prev_text = format!("{prev_text}{new_text}").into();
                } else {
                    edits.push((range, new_text));
                }
            }
        }
        if edits.is_empty() {
            return None;
        }

        self.start_transaction();
        self.pending_autoindent.take();
        let autoindent_request = autoindent_mode
            .and_then(|mode| self.language.as_ref().map(|_| (self.snapshot(), mode)));

        let edit_operation = self.text.edit(edits.iter().cloned());
        let edit_id = edit_operation.timestamp();

        if let Some((before_edit, mode)) = autoindent_request {
            let mut delta = 0isize;
            let mut previous_setting = None;
            let entries: Vec<_> = edits
                .into_iter()
                .enumerate()
                .zip(&edit_operation.as_edit().unwrap().new_text)
                .filter(|((_, (range, _)), _)| {
                    let language = before_edit.language_at(range.start);
                    let language_id = language.map(|l| l.id());
                    if let Some((cached_language_id, apply_syntax_indent)) = previous_setting
                        && cached_language_id == language_id
                    {
                        apply_syntax_indent
                    } else {
                        // The auto-indent setting is not present in editorconfigs, hence
                        // we can avoid passing the file here.
                        let auto_indent_mode = LanguageSettings::resolve(
                            None,
                            language.map(|l| l.name()).as_ref(),
                            cx,
                        )
                        .auto_indent;
                        let apply_syntax_indent = auto_indent_mode == AutoIndentMode::SyntaxAware;
                        previous_setting = Some((language_id, apply_syntax_indent));
                        apply_syntax_indent
                    }
                })
                .map(|((ix, (range, _)), new_text)| {
                    let new_text_length = new_text.len();
                    let old_start = range.start.to_point(&before_edit);
                    let new_start = (delta + range.start as isize) as usize;
                    let range_len = range.end - range.start;
                    delta += new_text_length as isize - range_len as isize;

                    // Decide what range of the insertion to auto-indent, and whether
                    // the first line of the insertion should be considered a newly-inserted line
                    // or an edit to an existing line.
                    let mut range_of_insertion_to_indent = 0..new_text_length;
                    let mut first_line_is_new = true;

                    let old_line_start = before_edit.indent_size_for_line(old_start.row).len;
                    let old_line_end = before_edit.line_len(old_start.row);

                    if old_start.column > old_line_start {
                        first_line_is_new = false;
                    }

                    if !new_text.contains('\n')
                        && (old_start.column + (range_len as u32) < old_line_end
                            || old_line_end == old_line_start)
                    {
                        first_line_is_new = false;
                    }

                    // When inserting text starting with a newline, avoid auto-indenting the
                    // previous line.
                    if new_text.starts_with('\n') {
                        range_of_insertion_to_indent.start += 1;
                        first_line_is_new = true;
                    }

                    let mut original_indent_column = None;
                    if let AutoindentMode::Block {
                        original_indent_columns,
                    } = &mode
                    {
                        original_indent_column = Some(if new_text.starts_with('\n') {
                            indent_size_for_text(
                                new_text[range_of_insertion_to_indent.clone()].chars(),
                            )
                            .len
                        } else {
                            original_indent_columns
                                .get(ix)
                                .copied()
                                .flatten()
                                .unwrap_or_else(|| {
                                    indent_size_for_text(
                                        new_text[range_of_insertion_to_indent.clone()].chars(),
                                    )
                                    .len
                                })
                        });

                        // Avoid auto-indenting the line after the edit.
                        if new_text[range_of_insertion_to_indent.clone()].ends_with('\n') {
                            range_of_insertion_to_indent.end -= 1;
                        }
                    }

                    AutoindentRequestEntry {
                        original_indent_column,
                        old_row: if first_line_is_new {
                            None
                        } else {
                            Some(old_start.row)
                        },
                        indent_size: before_edit.language_indent_size_at(range.start, cx),
                        range: self.anchor_before(new_start + range_of_insertion_to_indent.start)
                            ..self.anchor_after(new_start + range_of_insertion_to_indent.end),
                    }
                })
                .collect();

            if !entries.is_empty() {
                self.autoindent_requests.push(Arc::new(AutoindentRequest {
                    before_edit,
                    entries,
                    is_block_mode: matches!(mode, AutoindentMode::Block { .. }),
                    ignore_empty_lines: false,
                }));
            }
        }

        self.end_transaction(cx);
        self.send_operation(Operation::Buffer(edit_operation), true, cx);
        Some(edit_id)
    }

    pub(super) fn did_edit(
        &mut self,
        old_version: &clock::Global,
        was_dirty: bool,
        source: BufferEditSource,
        cx: &mut Context<Self>,
    ) {
        self.was_changed();

        if self.edits_since::<usize>(old_version).next().is_none() {
            return;
        }

        self.reparse(cx, true);
        cx.emit(BufferEvent::Edited { source });
        let is_dirty = self.is_dirty();
        if was_dirty != is_dirty {
            cx.emit(BufferEvent::DirtyChanged);
        }
        if was_dirty && !is_dirty {
            if let Some(file) = self.file.as_ref() {
                if matches!(file.disk_state(), DiskState::Present { .. })
                    && file.disk_state().mtime() != self.saved_mtime
                {
                    cx.emit(BufferEvent::ReloadNeeded);
                }
            }
        }
        cx.notify();
    }
}

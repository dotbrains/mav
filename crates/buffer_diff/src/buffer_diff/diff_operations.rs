use super::*;

impl BufferDiff {
    pub fn stage_or_unstage_hunks(
        &mut self,
        stage: bool,
        hunks: &[DiffHunk],
        buffer: &text::BufferSnapshot,
        file_exists: bool,
        cx: &mut Context<Self>,
    ) -> Option<Rope> {
        let secondary_diff = self.secondary_diff.clone()?;
        let diff_snapshot = self.diff_snapshot.as_mut()?;
        let unstaged_diff_snapshot = secondary_diff.read_with(cx, |secondary_diff, _cx| {
            secondary_diff.diff_snapshot.clone()
        })?;
        let new_index_text = diff_snapshot.stage_or_unstage_hunks_impl(
            &unstaged_diff_snapshot,
            stage,
            hunks,
            buffer,
            file_exists,
        );

        cx.emit(BufferDiffEvent::HunksStagedOrUnstaged(
            new_index_text.clone(),
        ));
        if let Some((first, last)) = hunks.first().zip(hunks.last()) {
            let changed_range = Some(first.buffer_range.start..last.buffer_range.end);
            let base_text_changed_range =
                Some(first.diff_base_byte_range.start..last.diff_base_byte_range.end);
            cx.emit(BufferDiffEvent::DiffChanged(DiffChanged {
                changed_range: changed_range.clone(),
                base_text_changed_range,
                extended_range: changed_range,
                base_text_changed: false,
            }));
        }
        new_index_text
    }

    pub fn stage_or_unstage_all_hunks(
        &mut self,
        stage: bool,
        buffer: &text::BufferSnapshot,
        file_exists: bool,
        cx: &mut Context<Self>,
    ) {
        let hunks = self
            .snapshot(cx)
            .hunks_intersecting_range(Anchor::min_max_range_for_buffer(buffer.remote_id()), buffer)
            .collect::<Vec<_>>();
        let Some(diff_snapshot) = &mut self.diff_snapshot else {
            return;
        };
        let Some(secondary) = self.secondary_diff.clone() else {
            return;
        };
        let secondary = secondary.read(cx);
        let Some(secondary_snapshot) = &secondary.diff_snapshot else {
            return;
        };
        diff_snapshot.stage_or_unstage_hunks_impl(
            &secondary_snapshot,
            stage,
            &hunks,
            buffer,
            file_exists,
        );
        if let Some((first, last)) = hunks.first().zip(hunks.last()) {
            let changed_range = Some(first.buffer_range.start..last.buffer_range.end);
            let base_text_changed_range =
                Some(first.diff_base_byte_range.start..last.diff_base_byte_range.end);
            cx.emit(BufferDiffEvent::DiffChanged(DiffChanged {
                changed_range: changed_range.clone(),
                base_text_changed_range,
                extended_range: changed_range,
                base_text_changed: false,
            }));
        }
    }

    pub fn update_diff(
        &self,
        buffer: text::BufferSnapshot,
        base_text_snapshot: &language::BufferSnapshot,
        base_text: Option<Arc<str>>,
        cx: &App,
    ) -> Task<BufferDiffUpdate> {
        let base_text = base_text.map(|t| text::LineEnding::normalize_arc(t));
        debug_assert_eq!(
            base_text.as_deref().unwrap_or_default(),
            &base_text_snapshot.text()
        );
        debug_assert_eq!(
            base_text_snapshot.remote_id(),
            self.base_text_buffer.read(cx).remote_id()
        );

        let language = base_text_snapshot.language();
        let diff_options = build_diff_options(
            language.map(|l| l.name()),
            language.map(|l| l.default_scope()),
            cx,
        );
        let buffer_snapshot = buffer.clone();
        let base_text_snapshot = base_text_snapshot.clone();
        let base_text_exists = base_text.is_some();
        let unchanged_hunks = self.diff_snapshot.as_ref().and_then(|diff_snapshot| {
            if diff_snapshot.base_text_exists == base_text_exists
                && diff_snapshot.base_text.version() == base_text_snapshot.version()
                && diff_snapshot.buffer_snapshot.version() == buffer_snapshot.version()
            {
                Some(diff_snapshot.hunks.clone())
            } else {
                None
            }
        });

        cx.background_executor().spawn(async move {
            let hunks = if let Some(unchanged_hunks) = unchanged_hunks {
                unchanged_hunks
            } else if let Some(base_text) = base_text {
                compute_hunks(
                    Some((base_text, base_text_snapshot.as_rope().clone())),
                    &buffer,
                    diff_options,
                )
            } else {
                compute_hunks(None, &buffer, diff_options)
            };

            BufferDiffUpdate {
                hunks,
                base_text: base_text_snapshot,
                base_text_exists,
                buffer_snapshot,
            }
        })
    }

    pub fn set_snapshot_with_secondary(
        &mut self,
        update: BufferDiffUpdate,
        secondary_diff_change: Option<Range<Anchor>>,
        clear_pending_hunks: bool,
        cx: &mut Context<Self>,
    ) -> Option<Range<Anchor>> {
        log::debug!("set snapshot with secondary {secondary_diff_change:?}");

        let BufferDiffUpdate {
            hunks: new_hunks,
            base_text: new_base_text,
            base_text_exists: new_base_text_exists,
            buffer_snapshot: new_buffer_snapshot,
        } = update;
        let buffer = &new_buffer_snapshot;
        let old_snapshot = self
            .diff_snapshot
            .clone()
            .unwrap_or_else(|| BufferDiffSnapshot {
                hunks: SumTree::new(buffer),
                pending_hunks: SumTree::new(buffer),
                base_text: new_base_text.clone(),
                base_text_exists: false,
                buffer_snapshot: new_buffer_snapshot.clone(),
                secondary_diff: None,
            });
        let mut new_snapshot = BufferDiffSnapshot {
            hunks: new_hunks.clone(),
            base_text: new_base_text.clone(),
            base_text_exists: new_base_text_exists,
            buffer_snapshot: new_buffer_snapshot.clone(),
            pending_hunks: old_snapshot.pending_hunks.clone(),
            secondary_diff: None,
        };

        let old_base_text_exists = old_snapshot.base_text_exists;
        let old_buffer_snapshot = &old_snapshot.buffer_snapshot;
        let old_base_text = &old_snapshot.base_text;
        let base_text_changed = old_base_text_exists != new_base_text_exists
            || (new_base_text_exists
                && (old_base_text.remote_id() != new_base_text.remote_id()
                    || new_base_text
                        .version()
                        .changed_since(old_base_text.version())));
        let DiffChanged {
            mut changed_range,
            mut base_text_changed_range,
            mut extended_range,
            base_text_changed: _,
        } = match (old_base_text_exists, new_base_text_exists) {
            (false, false) if self.diff_snapshot.is_some() => DiffChanged::default(),
            (true, true) => compare_hunks(
                &new_hunks,
                &old_snapshot.hunks,
                old_buffer_snapshot,
                buffer,
                old_base_text,
                &new_base_text,
            ),
            _ => {
                let full_range = text::Anchor::min_max_range_for_buffer(self.buffer_id);
                let full_base_range = 0..new_base_text.len();
                DiffChanged {
                    changed_range: Some(full_range.clone()),
                    base_text_changed_range: Some(full_base_range),
                    extended_range: Some(full_range),
                    base_text_changed: false,
                }
            }
        };

        if base_text_changed || clear_pending_hunks {
            if let Some((first, last)) = old_snapshot
                .pending_hunks
                .first()
                .zip(old_snapshot.pending_hunks.last())
            {
                let pending_range = first.buffer_range.start..last.buffer_range.end;
                if let Some(range) = &mut changed_range {
                    range.start = *range.start.min(&pending_range.start, buffer);
                    range.end = *range.end.max(&pending_range.end, buffer);
                } else {
                    changed_range = Some(pending_range.clone());
                }

                if let Some(base_text_range) = base_text_changed_range.as_mut() {
                    base_text_range.start =
                        base_text_range.start.min(first.diff_base_byte_range.start);
                    base_text_range.end = base_text_range.end.max(last.diff_base_byte_range.end);
                } else {
                    base_text_changed_range =
                        Some(first.diff_base_byte_range.start..last.diff_base_byte_range.end);
                }

                if let Some(ext) = &mut extended_range {
                    ext.start = *ext.start.min(&pending_range.start, buffer);
                    ext.end = *ext.end.max(&pending_range.end, buffer);
                } else {
                    extended_range = Some(pending_range);
                }
            }
            new_snapshot.pending_hunks = SumTree::new(buffer);
        }

        if let Some(secondary_changed_range) = secondary_diff_change
            && let (Some(secondary_hunk_range), Some(secondary_base_range)) =
                old_snapshot.range_to_hunk_range(secondary_changed_range, buffer)
        {
            if let Some(range) = &mut changed_range {
                range.start = *secondary_hunk_range.start.min(&range.start, buffer);
                range.end = *secondary_hunk_range.end.max(&range.end, buffer);
            } else {
                changed_range = Some(secondary_hunk_range.clone());
            }

            if let Some(base_text_range) = base_text_changed_range.as_mut() {
                base_text_range.start = secondary_base_range.start.min(base_text_range.start);
                base_text_range.end = secondary_base_range.end.max(base_text_range.end);
            } else {
                base_text_changed_range = Some(secondary_base_range);
            }

            if let Some(ext) = &mut extended_range {
                ext.start = *ext.start.min(&secondary_hunk_range.start, buffer);
                ext.end = *ext.end.max(&secondary_hunk_range.end, buffer);
            } else {
                extended_range = Some(secondary_hunk_range);
            }
        }

        self.diff_snapshot = Some(new_snapshot);
        self.buffer_snapshot = new_buffer_snapshot;

        let result = DiffChanged {
            changed_range,
            base_text_changed_range,
            extended_range,
            base_text_changed,
        };
        if result.base_text_changed {
            cx.emit(BufferDiffEvent::BaseTextChanged);
        }
        let changed_range = result.changed_range.clone();
        cx.emit(BufferDiffEvent::DiffChanged(result));
        changed_range
    }

    pub fn set_snapshot(
        &mut self,
        new_state: BufferDiffUpdate,
        cx: &mut Context<Self>,
    ) -> Option<Range<Anchor>> {
        self.set_snapshot_with_secondary(new_state, None, false, cx)
    }

    pub fn base_text(&self, cx: &App) -> language::BufferSnapshot {
        self.base_text_buffer.read(cx).snapshot()
    }

    pub fn base_text_exists(&self) -> bool {
        self.diff_snapshot
            .as_ref()
            .is_some_and(|diff_snapshot| diff_snapshot.base_text_exists)
    }

    pub fn snapshot(&self, cx: &App) -> BufferDiffSnapshot {
        let mut snapshot = self.diff_snapshot.clone().unwrap_or_else(|| {
            let base_text = self.base_text_buffer.read(cx).snapshot();
            BufferDiffSnapshot {
                hunks: SumTree::new(&self.buffer_snapshot),
                pending_hunks: SumTree::new(&self.buffer_snapshot),
                base_text,
                base_text_exists: false,
                buffer_snapshot: self.buffer_snapshot.clone(),
                secondary_diff: None,
            }
        });
        snapshot.secondary_diff = self.secondary_diff.as_ref().map(|diff| {
            debug_assert!(diff.read(cx).secondary_diff.is_none());
            Arc::new(diff.read(cx).snapshot(cx))
        });
        snapshot
    }

    /// Used in cases where the change set isn't derived from git.
    ///
    /// Dropping the returned task cancels the update, leaving the diff
    /// unchanged. Calls must not overlap; to re-run this when the buffer or
    /// base text changes, store the task somewhere that the next call will
    /// overwrite, so that the previous call is cancelled.
    pub fn set_base_text(
        &mut self,
        base_text: Option<Arc<str>>,
        buffer: text::BufferSnapshot,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let base_text_exists = base_text.is_some();
            let base_text = base_text.unwrap_or_default();
            let Some(base_text_diff) = this
                .update(cx, |this, cx| {
                    this.base_text_buffer.update(cx, |base_text_buffer, cx| {
                        base_text_buffer.diff(base_text.clone(), cx)
                    })
                })
                .log_err()
            else {
                return;
            };
            let base_text_diff = base_text_diff.await;
            let Some(edited_base_text) = this
                .update(cx, |this, cx| {
                    if this.base_text_buffer.read(cx).version() != base_text_diff.base_version {
                        log::warn!("dropping concurrent diff update");
                        debug_panic!("incorrect concurrent call to set_base_text");
                        return None;
                    }
                    let edited_base_text =
                        this.base_text_buffer.update(cx, |base_text_buffer, cx| {
                            base_text_buffer.set_line_ending(base_text_diff.line_ending, cx);
                            assert!(base_text_buffer.version() == base_text_diff.base_version);
                            base_text_buffer.snapshot_with_edits(base_text_diff.edits, cx)
                        });
                    Some(edited_base_text)
                })
                .log_err()
                .flatten()
            else {
                return;
            };
            let edited_base_text = edited_base_text.await;
            let base_text_snapshot = edited_base_text.snapshot().clone();
            let Some(state) = this
                .update(cx, |this, cx| {
                    this.update_diff(
                        buffer.clone(),
                        &base_text_snapshot,
                        base_text_exists.then(|| base_text.clone()),
                        cx,
                    )
                })
                .log_err()
            else {
                return;
            };
            let state = state.await;
            this.update(cx, |this, cx| {
                if &this.base_text_buffer.read(cx).version() != edited_base_text.base_version() {
                    log::warn!("dropping concurrent diff update");
                    debug_panic!("incorrect concurrent call to set_base_text");
                    return;
                }

                this.base_text_buffer.update(cx, |base_text_buffer, cx| {
                    base_text_buffer.fast_forward(edited_base_text, cx)
                });
                this.set_snapshot(state, cx);
            })
            .log_err();
        })
    }

    pub fn base_text_string(&self, _cx: &App) -> Option<String> {
        self.diff_snapshot.as_ref().and_then(|diff_snapshot| {
            if diff_snapshot.base_text_exists {
                Some(diff_snapshot.base_text.text())
            } else {
                None
            }
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn recalculate_diff_sync(&mut self, buffer: &text::BufferSnapshot, cx: &mut Context<Self>) {
        let base_text = self.base_text(cx);
        let fut = self.update_diff(
            buffer.clone(),
            &base_text,
            self.base_text_exists().then(|| Arc::from(base_text.text())),
            cx,
        );
        let fg_executor = cx.foreground_executor().clone();
        let snapshot = fg_executor.block_on(fut);
        let _changed_range = self.set_snapshot(snapshot, cx);
    }

    pub fn base_text_buffer(&self) -> &Entity<language::Buffer> {
        &self.base_text_buffer
    }
}

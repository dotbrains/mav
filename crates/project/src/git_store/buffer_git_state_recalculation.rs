use super::*;

impl BufferGitState {
    #[ztracing::instrument(skip_all)]
    pub(super) fn recalculate_diffs(
        &mut self,
        buffer: text::BufferSnapshot,
        cx: &mut Context<Self>,
    ) {
        *self.recalculating_tx.borrow_mut() = true;

        let language = self.language.clone();
        let language_registry = self.language_registry.clone();
        let unstaged_diff = self.unstaged_diff();
        let staged_diff = self.staged_diff();
        let uncommitted_diff = self.uncommitted_diff();
        let head = self.head_text.clone();
        let index = self.index_text.clone();
        let head_text_buffer = self.head_text_buffer.upgrade();
        let index_text_buffer = self.index_text_buffer.upgrade();
        let index_text_buffer_language_enabled = self.index_text_buffer_language_enabled;
        let index_changed = self.index_changed;
        let head_changed = self.head_changed;
        let language_changed = self.language_changed;
        let prev_hunk_staging_operation_count = self.hunk_staging_operation_count_as_of_write;
        let index_matches_head = self.index_matches_head();

        let oid_diffs: Vec<(
            Option<git::Oid>,
            Entity<BufferDiff>,
            Entity<Buffer>,
            Option<Arc<str>>,
        )> = self
            .oid_diffs
            .iter()
            .filter_map(|(oid, weak)| {
                let diff = weak.upgrade()?;
                let base_text_buffer = diff.read(cx).base_text_buffer().clone();
                let base_text = match oid {
                    Some(oid) => Some(self.oid_texts.get(oid)?.clone()),
                    None => None,
                };
                Some((*oid, diff, base_text_buffer, base_text))
            })
            .collect();

        self.oid_diffs.retain(|oid, weak| {
            let alive = weak.upgrade().is_some();
            if !alive {
                if let Some(oid) = oid {
                    self.oid_texts.remove(oid);
                }
            }
            alive
        });
        if self
            .staged_diff
            .as_ref()
            .is_some_and(|(weak, _)| !weak.is_upgradable())
        {
            self.staged_diff = None;
        }
        self.recalculate_diff_task = Some(cx.spawn(async move |this, cx| {
            log::debug!(
                "start recalculating diffs for buffer {}",
                buffer.remote_id()
            );

            if index_text_buffer_language_enabled
                && let Some(index_text_buffer) = &index_text_buffer
            {
                index_text_buffer.update(cx, |index_text_buffer, cx| {
                    if let Some(language_registry) = language_registry.clone() {
                        index_text_buffer.set_language_registry(language_registry);
                    }
                    index_text_buffer.set_language_async(language.clone(), cx);
                });
            }
            if let Some(head_text_buffer) = &head_text_buffer {
                head_text_buffer.update(cx, |head_text_buffer, cx| {
                    if let Some(language_registry) = language_registry.clone() {
                        head_text_buffer.set_language_registry(language_registry);
                    }
                    head_text_buffer.set_language_async(language.clone(), cx);
                });
            }

            for (_, _, base_text_buffer, _) in &oid_diffs {
                base_text_buffer.update(cx, |base_text_buffer, cx| {
                    if let Some(language_registry) = language_registry.clone() {
                        base_text_buffer.set_language_registry(language_registry);
                    }
                    base_text_buffer.set_language_async(language.clone(), cx);
                });
            }

            let mut edited_index_text = None;

            let index_text_snapshot = if let Some(index_text_buffer) = &index_text_buffer
                && (unstaged_diff.is_some() || staged_diff.is_some())
            {
                let index_text_snapshot = if index_changed || language_changed {
                    let new_index_text = index.clone().unwrap_or_default();
                    let index_text_diff = index_text_buffer
                        .update(cx, |index_text_buffer, cx| {
                            index_text_buffer.diff(new_index_text.clone(), cx)
                        })
                        .await;
                    let edited = index_text_buffer
                        .update(cx, |index_text_buffer, cx| {
                            index_text_buffer.snapshot_with_edits(index_text_diff.edits, cx)
                        })
                        .await;
                    let snapshot = edited.snapshot().clone();
                    edited_index_text = Some(edited);
                    snapshot
                } else {
                    index_text_buffer.read_with(cx, |buffer, _| buffer.snapshot())
                };
                Some(index_text_snapshot)
            } else {
                None
            };

            let mut new_unstaged_diff = None;

            if let (Some(unstaged_diff), Some(index_text_snapshot)) =
                (unstaged_diff.as_ref(), index_text_snapshot.as_ref())
            {
                new_unstaged_diff = Some(
                    cx.update(|cx| {
                        unstaged_diff.read(cx).update_diff(
                            buffer.clone(),
                            index_text_snapshot,
                            index.clone(),
                            cx,
                        )
                    })
                    .await,
                );
            }

            // Dropping BufferDiff can be expensive, so yield back to the event loop
            // for a bit
            yield_now().await;

            let mut edited_head_text = None;
            let mut new_staged_diff = None;
            let mut new_uncommitted_diff = None;
            if let Some(head_text_buffer) = &head_text_buffer
                && (staged_diff.is_some() || uncommitted_diff.is_some())
            {
                let head_base_text_exists = head.is_some();
                let head_text_snapshot = if head_changed || language_changed {
                    let new_head_text = head.clone().unwrap_or_default();
                    let head_text_diff = head_text_buffer
                        .update(cx, |head_text_buffer, cx| {
                            head_text_buffer.diff(new_head_text.clone(), cx)
                        })
                        .await;
                    let edited = head_text_buffer
                        .update(cx, |base_text_buffer, cx| {
                            base_text_buffer.snapshot_with_edits(head_text_diff.edits, cx)
                        })
                        .await;
                    let snapshot = edited.snapshot().clone();
                    edited_head_text = Some(edited);
                    snapshot
                } else {
                    head_text_buffer.read_with(cx, |buffer, _| buffer.snapshot())
                };
                if let (Some(staged_diff), Some(index_base_text_snapshot)) =
                    (staged_diff.as_ref(), index_text_snapshot.as_ref())
                {
                    new_staged_diff = Some(
                        cx.update(|cx| {
                            staged_diff.read(cx).update_diff(
                                index_base_text_snapshot.text.clone(),
                                &head_text_snapshot,
                                head.clone(),
                                cx,
                            )
                        })
                        .await,
                    );
                }

                if let Some(uncommitted_diff) = &uncommitted_diff {
                    new_uncommitted_diff = if index_matches_head {
                        new_unstaged_diff.clone().map(|mut update| {
                            update.set_base_text_snapshot(
                                head_text_snapshot.clone(),
                                head_base_text_exists,
                            );
                            update
                        })
                    } else {
                        None
                    };
                    if new_uncommitted_diff.is_none() {
                        new_uncommitted_diff = Some(
                            cx.update(|cx| {
                                uncommitted_diff.read(cx).update_diff(
                                    buffer.clone(),
                                    &head_text_snapshot,
                                    head.clone(),
                                    cx,
                                )
                            })
                            .await,
                        );
                    }
                }
            }

            // Dropping BufferDiff can be expensive, so yield back to the event loop
            // for a bit
            yield_now().await;

            let cancel = this.update(cx, |this, _| {
                // This checks whether all pending stage/unstage operations
                // have quiesced (i.e. both the corresponding write and the
                // read of that write have completed). If not, then we cancel
                // this recalculation attempt to avoid invalidating pending
                // state too quickly; another recalculation will come along
                // later and clear the pending state once the state of the index has settled.
                if this.hunk_staging_operation_count > prev_hunk_staging_operation_count {
                    *this.recalculating_tx.borrow_mut() = false;
                    true
                } else {
                    false
                }
            })?;
            if cancel {
                log::debug!(
                    concat!(
                        "aborting recalculating diffs for buffer {}",
                        "due to subsequent hunk operations",
                    ),
                    buffer.remote_id()
                );
                return Ok(());
            }

            this.update(cx, |_, cx| {
                if let (Some(staged_diff), Some(new_staged_diff)) =
                    (staged_diff.as_ref(), new_staged_diff.clone())
                {
                    staged_diff.update(cx, |diff, cx| {
                        if let Some(edited_base_text) = edited_index_text.take()
                            && let Some(index_text_buffer) = &index_text_buffer
                        {
                            index_text_buffer.update(cx, |index_text_buffer, cx| {
                                index_text_buffer.fast_forward(edited_base_text, cx)
                            });
                        }
                        if let Some(edited_head_text) = edited_head_text.take()
                            && let Some(head_text_buffer) = &head_text_buffer
                        {
                            head_text_buffer.update(cx, |head_text_buffer, cx| {
                                head_text_buffer.fast_forward(edited_head_text, cx)
                            });
                        }
                        diff.set_snapshot(new_staged_diff, cx)
                    });
                }

                let unstaged_changed_range = if let (Some(unstaged_diff), Some(new_unstaged_diff)) =
                    (unstaged_diff.as_ref(), new_unstaged_diff.clone())
                {
                    Some(unstaged_diff.update(cx, |diff, cx| {
                        if let Some(edited_index_text) = edited_index_text.take()
                            && let Some(index_text_buffer) = &index_text_buffer
                        {
                            index_text_buffer.update(cx, |index_text_buffer, cx| {
                                index_text_buffer.fast_forward(edited_index_text, cx)
                            });
                        }
                        diff.set_snapshot(new_unstaged_diff, cx)
                    }))
                } else {
                    None
                };

                if let (Some(uncommitted_diff), Some(new_uncommitted_diff)) =
                    (uncommitted_diff.as_ref(), new_uncommitted_diff.clone())
                {
                    uncommitted_diff.update(cx, |diff, cx| {
                        if let Some(edited_base_text) = edited_head_text.take()
                            && let Some(head_text_buffer) = &head_text_buffer
                        {
                            head_text_buffer.update(cx, |head_text_buffer, cx| {
                                head_text_buffer.fast_forward(edited_base_text, cx)
                            });
                        }
                        diff.set_snapshot_with_secondary(
                            new_uncommitted_diff,
                            unstaged_changed_range.flatten(),
                            true,
                            cx,
                        )
                    });
                }
            })?;

            yield_now().await;

            for (oid, oid_diff, base_text_buffer, base_text) in oid_diffs {
                let base_text_snapshot =
                    base_text_buffer.read_with(cx, |buffer, _| buffer.snapshot());
                let new_oid_diff = cx
                    .update(|cx| {
                        oid_diff.read(cx).update_diff(
                            buffer.clone(),
                            &base_text_snapshot,
                            base_text.clone(),
                            cx,
                        )
                    })
                    .await;

                oid_diff.update(cx, |diff, cx| diff.set_snapshot(new_oid_diff, cx));

                log::debug!(
                    "finished recalculating oid diff for buffer {} oid {:?}",
                    buffer.remote_id(),
                    oid
                );

                yield_now().await;
            }

            log::debug!(
                "finished recalculating diffs for buffer {}",
                buffer.remote_id()
            );

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, _| {
                    this.index_changed = false;
                    this.head_changed = false;
                    this.language_changed = false;
                    *this.recalculating_tx.borrow_mut() = false;
                });
            }

            Ok(())
        }));
    }
}

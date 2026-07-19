use super::*;

impl MultiBuffer {
    pub(crate) fn set_merged_excerpt_ranges_for_path<T>(
        &mut self,
        path: PathKey,
        buffer: Entity<Buffer>,
        buffer_snapshot: &BufferSnapshot,
        new: Vec<ExcerptRange<T>>,
        cx: &mut Context<Self>,
    ) -> (bool, PathKeyIndex)
    where
        T: language::ToOffset,
    {
        let anchor_ranges = new
            .into_iter()
            .map(|r| ExcerptRange {
                context: buffer_snapshot.anchor_before(r.context.start)
                    ..buffer_snapshot.anchor_after(r.context.end),
                primary: buffer_snapshot.anchor_before(r.primary.start)
                    ..buffer_snapshot.anchor_after(r.primary.end),
            })
            .collect::<Vec<_>>();
        let inserted =
            self.update_path_excerpts(path.clone(), buffer, buffer_snapshot, &anchor_ranges, cx);
        let path_key_index = self.get_or_create_path_key_index(&path);
        (inserted, path_key_index)
    }

    pub(crate) fn get_or_create_path_key_index(&mut self, path_key: &PathKey) -> PathKeyIndex {
        let mut snapshot = self.snapshot.borrow_mut();

        if let Some(existing) = snapshot.path_keys.get_index_of(path_key) {
            return PathKeyIndex(existing as u64);
        }

        PathKeyIndex(
            Arc::make_mut(&mut snapshot.path_keys)
                .insert_full(path_key.clone())
                .0 as u64,
        )
    }

    pub fn update_path_excerpts(
        &mut self,
        path_key: PathKey,
        buffer: Entity<Buffer>,
        buffer_snapshot: &BufferSnapshot,
        to_insert: &Vec<ExcerptRange<text::Anchor>>,
        cx: &mut Context<Self>,
    ) -> bool {
        let path_key_index = self.get_or_create_path_key_index(&path_key);
        if let Some(old_path_key) = self
            .snapshot(cx)
            .path_for_buffer(buffer_snapshot.remote_id())
            && old_path_key != &path_key
        {
            self.remove_excerpts(old_path_key.clone(), cx);
        }

        if to_insert.len() == 0 {
            self.remove_excerpts(path_key.clone(), cx);

            return false;
        }
        assert_eq!(self.history.transaction_depth(), 0);
        self.sync_mut(cx);

        let buffer_id = buffer_snapshot.remote_id();

        let mut snapshot = self.snapshot.get_mut();
        let mut cursor = snapshot
            .excerpts
            .cursor::<Dimensions<PathKey, ExcerptOffset>>(());
        let mut new_excerpts = SumTree::new(());

        let new_ranges = to_insert.clone();
        let mut to_insert = to_insert.iter().peekable();
        let mut patch = Patch::empty();
        let mut added_new_excerpt = false;

        new_excerpts.append(cursor.slice(&path_key, Bias::Left), ());

        // handle the case where the path key used to be associated
        // with a different buffer by removing its excerpts.
        if let Some(excerpt) = cursor.item()
            && &excerpt.path_key == &path_key
            && excerpt.buffer_id != buffer_id
        {
            let old_buffer_id = excerpt.buffer_id;
            self.buffers.remove(&old_buffer_id);
            snapshot.buffers.remove(&old_buffer_id);
            remove_diff_state(&mut snapshot.diffs, old_buffer_id);
            self.diffs.remove(&old_buffer_id);
            let before = cursor.position.1;
            cursor.seek_forward(&path_key, Bias::Right);
            let after = cursor.position.1;
            patch.push(Edit {
                old: before..after,
                new: new_excerpts.summary().len()..new_excerpts.summary().len(),
            });
            cx.emit(Event::BuffersRemoved {
                removed_buffer_ids: vec![old_buffer_id],
            });
        }

        while let Some(excerpt) = cursor.item()
            && excerpt.path_key == path_key
        {
            assert_eq!(excerpt.buffer_id, buffer_id);
            let Some(next_excerpt) = to_insert.peek() else {
                break;
            };
            if &excerpt.range == *next_excerpt {
                let before = new_excerpts.summary().len();
                new_excerpts.update_last(
                    |prev_excerpt| {
                        if !prev_excerpt.has_trailing_newline {
                            prev_excerpt.has_trailing_newline = true;
                            patch.push(Edit {
                                old: cursor.position.1..cursor.position.1,
                                new: before..before + MultiBufferOffset(1),
                            });
                        }
                    },
                    (),
                );
                new_excerpts.push(excerpt.clone(), ());
                to_insert.next();
                cursor.next();
                continue;
            }

            if excerpt
                .range
                .context
                .start
                .cmp(&next_excerpt.context.start, &buffer_snapshot)
                .is_le()
            {
                // remove old excerpt
                let before = cursor.position.1;
                cursor.next();
                let after = cursor.position.1;
                patch.push(Edit {
                    old: before..after,
                    new: new_excerpts.summary().len()..new_excerpts.summary().len(),
                });
            } else {
                // insert new excerpt
                let next_excerpt = to_insert.next().unwrap();
                added_new_excerpt = true;
                let before = new_excerpts.summary().len();
                new_excerpts.update_last(
                    |prev_excerpt| {
                        prev_excerpt.has_trailing_newline = true;
                    },
                    (),
                );
                new_excerpts.push(
                    Excerpt::new(
                        path_key.clone(),
                        path_key_index,
                        &buffer_snapshot,
                        next_excerpt.clone(),
                        false,
                    ),
                    (),
                );
                let after = new_excerpts.summary().len();
                patch.push_maybe_empty(Edit {
                    old: cursor.position.1..cursor.position.1,
                    new: before..after,
                });
            }
        }

        // remove any further trailing excerpts
        let mut before = cursor.position.1;
        cursor.seek_forward(&path_key, Bias::Right);
        let after = cursor.position.1;
        // if we removed the previous last excerpt, remove the trailing newline from the new last excerpt
        if cursor.item().is_none() && to_insert.peek().is_none() {
            new_excerpts.update_last(
                |excerpt| {
                    if excerpt.has_trailing_newline {
                        before.0.0 = before
                            .0
                            .0
                            .checked_sub(1)
                            .expect("should have preceding excerpt");
                        excerpt.has_trailing_newline = false;
                    }
                },
                (),
            );
        }
        patch.push(Edit {
            old: before..after,
            new: new_excerpts.summary().len()..new_excerpts.summary().len(),
        });

        while let Some(next_excerpt) = to_insert.next() {
            added_new_excerpt = true;
            let before = new_excerpts.summary().len();
            new_excerpts.update_last(
                |prev_excerpt| {
                    prev_excerpt.has_trailing_newline = true;
                },
                (),
            );
            new_excerpts.push(
                Excerpt::new(
                    path_key.clone(),
                    path_key_index,
                    &buffer_snapshot,
                    next_excerpt.clone(),
                    false,
                ),
                (),
            );
            let after = new_excerpts.summary().len();
            patch.push_maybe_empty(Edit {
                old: cursor.position.1..cursor.position.1,
                new: before..after,
            });
        }

        let suffix_start = cursor.position.1;
        let suffix = cursor.suffix();
        let changed_trailing_excerpt = suffix.is_empty();
        if !suffix.is_empty() {
            let before = new_excerpts.summary().len();
            new_excerpts.update_last(
                |prev_excerpt| {
                    if !prev_excerpt.has_trailing_newline {
                        prev_excerpt.has_trailing_newline = true;
                        patch.push(Edit {
                            old: suffix_start..suffix_start,
                            new: before..before + MultiBufferOffset(1),
                        });
                    }
                },
                (),
            );
        }
        new_excerpts.append(suffix, ());
        drop(cursor);

        snapshot.excerpts = new_excerpts;
        snapshot.buffers.insert(
            buffer_id,
            BufferStateSnapshot {
                path_key: path_key.clone(),
                path_key_index,
                buffer_snapshot: buffer_snapshot.clone(),
            },
        );

        self.buffers.entry(buffer_id).or_insert_with(|| {
            self.buffer_changed_since_sync.replace(true);
            buffer.update(cx, |buffer, _| {
                buffer.record_changes(Rc::downgrade(&self.buffer_changed_since_sync));
            });
            BufferState {
                _subscriptions: [
                    cx.observe(&buffer, |_, _, cx| cx.notify()),
                    cx.subscribe(&buffer, Self::on_buffer_event),
                ],
                buffer: buffer.clone(),
            }
        });

        if changed_trailing_excerpt {
            snapshot.trailing_excerpt_update_count += 1;
        }

        let edits = Self::sync_diff_transforms(
            &mut snapshot,
            patch.into_inner(),
            DiffChangeKind::BufferEdited,
        );
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
            cx.emit(Event::Edited {
                edited_buffer: None,
                source: BufferEditSource::User,
            });
            cx.emit(Event::BufferRangesUpdated {
                buffer,
                path_key: path_key.clone(),
                ranges: new_ranges,
            });
            cx.notify();
        }

        added_new_excerpt
    }

    pub fn remove_excerpts_for_buffer(&mut self, buffer: BufferId, cx: &mut Context<Self>) {
        let snapshot = self.sync_mut(cx);
        let Some(path) = snapshot.path_for_buffer(buffer).cloned() else {
            return;
        };
        self.remove_excerpts(path, cx);
    }

    pub fn remove_excerpts(&mut self, path: PathKey, cx: &mut Context<Self>) {
        assert_eq!(self.history.transaction_depth(), 0);
        self.sync_mut(cx);

        let mut snapshot = self.snapshot.get_mut();
        let mut cursor = snapshot
            .excerpts
            .cursor::<Dimensions<PathKey, ExcerptOffset>>(());
        let mut new_excerpts = SumTree::new(());
        new_excerpts.append(cursor.slice(&path, Bias::Left), ());
        let mut edit_start = cursor.position.1;
        let mut buffer_id = None;
        if let Some(excerpt) = cursor.item()
            && excerpt.path_key == path
        {
            buffer_id = Some(excerpt.buffer_id);
        }
        cursor.seek(&path, Bias::Right);
        let edit_end = cursor.position.1;
        let suffix = cursor.suffix();
        let changed_trailing_excerpt = suffix.is_empty();
        new_excerpts.append(suffix, ());

        if let Some(buffer_id) = buffer_id {
            snapshot.buffers.remove(&buffer_id);
            remove_diff_state(&mut snapshot.diffs, buffer_id);
            self.buffers.remove(&buffer_id);
            self.diffs.remove(&buffer_id);
            cx.emit(Event::BuffersRemoved {
                removed_buffer_ids: vec![buffer_id],
            })
        }
        drop(cursor);
        if changed_trailing_excerpt {
            snapshot.trailing_excerpt_update_count += 1;
            new_excerpts.update_last(
                |excerpt| {
                    if excerpt.has_trailing_newline {
                        excerpt.has_trailing_newline = false;
                        edit_start.0.0 = edit_start
                            .0
                            .0
                            .checked_sub(1)
                            .expect("should have at least one excerpt");
                    }
                },
                (),
            )
        }

        let edit = Edit {
            old: edit_start..edit_end,
            new: edit_start..edit_start,
        };
        snapshot.excerpts = new_excerpts;

        let edits =
            Self::sync_diff_transforms(&mut snapshot, vec![edit], DiffChangeKind::BufferEdited);
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }

        cx.emit(Event::Edited {
            edited_buffer: None,
            source: BufferEditSource::User,
        });
        cx.notify();
    }
}

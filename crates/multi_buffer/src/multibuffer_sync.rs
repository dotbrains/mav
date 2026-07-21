use super::*;

impl MultiBuffer {
    #[ztracing::instrument(skip_all)]
    pub(super) fn sync(&self, cx: &App) {
        let changed = self.buffer_changed_since_sync.replace(false);
        if !changed {
            return;
        }
        let edits = Self::sync_from_buffer_changes(
            &mut self.snapshot.borrow_mut(),
            &self.buffers,
            &self.diffs,
            cx,
        );
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
    }

    pub(super) fn sync_mut(&mut self, cx: &App) -> &mut MultiBufferSnapshot {
        let snapshot = self.snapshot.get_mut();
        let changed = self.buffer_changed_since_sync.replace(false);
        if !changed {
            return snapshot;
        }
        let edits = Self::sync_from_buffer_changes(snapshot, &self.buffers, &self.diffs, cx);

        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }

        snapshot
    }

    pub(super) fn sync_from_buffer_changes(
        snapshot: &mut MultiBufferSnapshot,
        buffers: &BTreeMap<BufferId, BufferState>,
        diffs: &HashMap<BufferId, DiffState>,
        cx: &App,
    ) -> Vec<Edit<MultiBufferOffset>> {
        let MultiBufferSnapshot {
            excerpts,
            diffs: buffer_diff,
            buffers: buffer_snapshots,
            path_keys: _,
            diff_transforms: _,
            non_text_state_update_count,
            edit_count,
            is_dirty,
            has_deleted_file,
            has_conflict,
            has_inverted_diff: _,
            singleton: _,
            trailing_excerpt_update_count: _,
            all_diff_hunks_expanded: _,
            show_deleted_hunks: _,
            use_extended_diff_range: _,
            show_headers: _,
        } = snapshot;
        *is_dirty = false;
        *has_deleted_file = false;
        *has_conflict = false;

        if !diffs.is_empty() {
            let mut diffs_to_add = Vec::new();
            for (id, diff) in diffs {
                if find_diff_state(buffer_diff, *id).is_none_or(|existing_diff| {
                    if existing_diff.main_buffer.is_none() {
                        return false;
                    }
                    let base_text = diff.diff.read(cx).base_text_buffer().read(cx);
                    base_text.remote_id() != existing_diff.base_text().remote_id()
                        || base_text
                            .version()
                            .changed_since(existing_diff.base_text().version())
                }) {
                    if diffs_to_add.capacity() == 0 {
                        diffs_to_add.reserve(diffs.len());
                    }
                    diffs_to_add.push(sum_tree::Edit::Insert(diff.snapshot(*id, cx)));
                }
            }
            buffer_diff.edit(diffs_to_add, ());
        }

        let mut paths_to_edit = Vec::new();
        let mut non_text_state_updated = false;
        let mut edited = false;
        for buffer_state in buffers.values() {
            let buffer = buffer_state.buffer.read(cx);
            let last_snapshot = buffer_snapshots
                .get(&buffer.remote_id())
                .expect("each buffer should have a snapshot");
            let current_version = buffer.version();
            let non_text_state_update_count = buffer.non_text_state_update_count();

            let buffer_edited =
                current_version.changed_since(last_snapshot.buffer_snapshot.version());
            let buffer_non_text_state_updated = non_text_state_update_count
                > last_snapshot.buffer_snapshot.non_text_state_update_count();
            if buffer_edited || buffer_non_text_state_updated {
                paths_to_edit.push((
                    last_snapshot.path_key.clone(),
                    last_snapshot.path_key_index,
                    buffer_state.buffer.clone(),
                    if buffer_edited {
                        Some(last_snapshot.buffer_snapshot.version().clone())
                    } else {
                        None
                    },
                ));
            }

            edited |= buffer_edited;
            non_text_state_updated |= buffer_non_text_state_updated;
            *is_dirty |= buffer.is_dirty();
            *has_deleted_file |= buffer
                .file()
                .is_some_and(|file| file.disk_state().is_deleted());
            *has_conflict |= buffer.has_conflict();
        }
        if edited {
            *edit_count += 1;
        }
        if non_text_state_updated {
            *non_text_state_update_count += 1;
        }

        paths_to_edit.sort_unstable_by_key(|(path, _, _, _)| path.clone());

        let mut edits = Vec::new();
        let mut new_excerpts = SumTree::default();
        let mut cursor = excerpts.cursor::<ExcerptSummary>(());

        for (path, path_key_index, buffer, prev_version) in paths_to_edit {
            new_excerpts.append(cursor.slice(&path, Bias::Left), ());
            let buffer = buffer.read(cx);
            let buffer_id = buffer.remote_id();

            buffer_snapshots.insert(
                buffer_id,
                BufferStateSnapshot {
                    path_key: path.clone(),
                    path_key_index,
                    buffer_snapshot: buffer.snapshot(),
                },
            );

            if let Some(prev_version) = &prev_version {
                while let Some(old_excerpt) = cursor.item()
                    && &old_excerpt.path_key == &path
                {
                    edits.extend(
                        buffer
                            .edits_since_in_range::<usize>(
                                prev_version,
                                old_excerpt.range.context.clone(),
                            )
                            .map(|edit| {
                                let excerpt_old_start = cursor.start().len();
                                let excerpt_new_start =
                                    ExcerptDimension(new_excerpts.summary().text.len);
                                let old_start = excerpt_old_start + edit.old.start;
                                let old_end = excerpt_old_start + edit.old.end;
                                let new_start = excerpt_new_start + edit.new.start;
                                let new_end = excerpt_new_start + edit.new.end;
                                Edit {
                                    old: old_start..old_end,
                                    new: new_start..new_end,
                                }
                            }),
                    );

                    let excerpt = Excerpt::new(
                        old_excerpt.path_key.clone(),
                        old_excerpt.path_key_index,
                        &buffer.snapshot(),
                        old_excerpt.range.clone(),
                        old_excerpt.has_trailing_newline,
                    );
                    new_excerpts.push(excerpt, ());
                    cursor.next();
                }
            } else {
                new_excerpts.append(cursor.slice(&path, Bias::Right), ());
            };
        }
        new_excerpts.append(cursor.suffix(), ());

        drop(cursor);
        *excerpts = new_excerpts;

        Self::sync_diff_transforms(snapshot, edits, DiffChangeKind::BufferEdited)
    }
}

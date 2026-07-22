use super::*;

impl GitBlame {
    pub(super) fn sync_all(&mut self, cx: &mut App) {
        let Some(multi_buffer) = self.multi_buffer.upgrade() else {
            return;
        };
        let snapshot = multi_buffer.read(cx).snapshot(cx);
        for id in snapshot.all_buffer_ids() {
            self.sync(cx, id)
        }
    }
    pub(super) fn sync(&mut self, cx: &mut App, buffer_id: BufferId) {
        let Some(blame_buffer) = self.buffers.get_mut(&buffer_id) else {
            return;
        };
        let Some(buffer) = self
            .multi_buffer
            .upgrade()
            .and_then(|multi_buffer| multi_buffer.read(cx).buffer(buffer_id))
        else {
            return;
        };
        let edits = blame_buffer.buffer_edits.consume();
        let new_snapshot = buffer.read(cx).snapshot();

        let mut row_edits = edits
            .into_iter()
            .map(|edit| {
                let old_point_range = blame_buffer.buffer_snapshot.offset_to_point(edit.old.start)
                    ..blame_buffer.buffer_snapshot.offset_to_point(edit.old.end);
                let new_point_range = new_snapshot.offset_to_point(edit.new.start)
                    ..new_snapshot.offset_to_point(edit.new.end);

                if old_point_range.start.column
                    == blame_buffer
                        .buffer_snapshot
                        .line_len(old_point_range.start.row)
                    && (new_snapshot.chars_at(edit.new.start).next() == Some('\n')
                        || blame_buffer
                            .buffer_snapshot
                            .line_len(old_point_range.end.row)
                            == 0)
                {
                    Edit {
                        old: old_point_range.start.row + 1..old_point_range.end.row + 1,
                        new: new_point_range.start.row + 1..new_point_range.end.row + 1,
                    }
                } else if old_point_range.start.column == 0
                    && old_point_range.end.column == 0
                    && new_point_range.end.column == 0
                {
                    Edit {
                        old: old_point_range.start.row..old_point_range.end.row,
                        new: new_point_range.start.row..new_point_range.end.row,
                    }
                } else {
                    Edit {
                        old: old_point_range.start.row..old_point_range.end.row + 1,
                        new: new_point_range.start.row..new_point_range.end.row + 1,
                    }
                }
            })
            .peekable();

        let mut new_entries = SumTree::default();
        let mut cursor = blame_buffer.entries.cursor::<u32>(());

        while let Some(mut edit) = row_edits.next() {
            while let Some(next_edit) = row_edits.peek() {
                if edit.old.end >= next_edit.old.start {
                    edit.old.end = next_edit.old.end;
                    edit.new.end = next_edit.new.end;
                    row_edits.next();
                } else {
                    break;
                }
            }

            new_entries.append(cursor.slice(&edit.old.start, Bias::Right), ());

            if edit.new.start > new_entries.summary().rows {
                new_entries.push(
                    GitBlameEntry {
                        rows: edit.new.start - new_entries.summary().rows,
                        blame: cursor.item().and_then(|entry| entry.blame.clone()),
                    },
                    (),
                );
            }

            cursor.seek(&edit.old.end, Bias::Right);
            if !edit.new.is_empty() {
                new_entries.push(
                    GitBlameEntry {
                        rows: edit.new.len() as u32,
                        blame: None,
                    },
                    (),
                );
            }

            let old_end = cursor.end();
            if row_edits
                .peek()
                .is_none_or(|next_edit| next_edit.old.start >= old_end)
                && let Some(entry) = cursor.item()
            {
                if old_end > edit.old.end {
                    new_entries.push(
                        GitBlameEntry {
                            rows: cursor.end() - edit.old.end,
                            blame: entry.blame.clone(),
                        },
                        (),
                    );
                }

                cursor.next();
            }
        }
        new_entries.append(cursor.suffix(), ());
        drop(cursor);

        blame_buffer.buffer_snapshot = new_snapshot;
        blame_buffer.entries = new_entries;
    }

    #[cfg(test)]
    pub(super) fn check_invariants(&mut self, cx: &mut Context<Self>) {
        self.sync_all(cx);
        for (&id, buffer) in &self.buffers {
            assert_eq!(
                buffer.entries.summary().rows,
                self.multi_buffer
                    .upgrade()
                    .unwrap()
                    .read(cx)
                    .buffer(id)
                    .unwrap()
                    .read(cx)
                    .max_point()
                    .row
                    + 1
            );
        }
    }
}

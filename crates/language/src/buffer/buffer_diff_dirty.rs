use super::*;

impl Buffer {
    pub fn diff<T>(&self, new_text: T, cx: &App) -> Task<Diff>
    where
        T: AsRef<str> + Send + 'static,
    {
        let old_text = self.as_rope().clone();
        let base_version = self.version();
        cx.background_spawn(async move {
            let old_text = old_text.to_string();
            let mut new_text = new_text.as_ref().to_owned();
            let line_ending = LineEnding::detect(&new_text);
            LineEnding::normalize(&mut new_text);
            let edits = text_diff(&old_text, &new_text);
            Diff {
                base_version,
                line_ending,
                edits,
            }
        })
    }

    /// Spawns a background task that searches the buffer for any whitespace
    /// at the ends of a lines, and returns a `Diff` that removes that whitespace.
    pub fn remove_trailing_whitespace(&self, cx: &App) -> Task<Diff> {
        let old_text = self.as_rope().clone();
        let line_ending = self.line_ending();
        let base_version = self.version();
        cx.background_spawn(async move {
            let ranges = trailing_whitespace_ranges(&old_text);
            let empty = Arc::<str>::from("");
            Diff {
                base_version,
                line_ending,
                edits: ranges
                    .into_iter()
                    .map(|range| (range, empty.clone()))
                    .collect(),
            }
        })
    }

    /// Ensures that the buffer ends with a single newline character, and
    /// no other whitespace. Skips if the buffer is empty.
    pub fn ensure_final_newline(&mut self, cx: &mut Context<Self>) {
        let len = self.len();
        if len == 0 {
            return;
        }
        let mut offset = len;
        for chunk in self.as_rope().reversed_chunks_in_range(0..len) {
            let non_whitespace_len = chunk
                .trim_end_matches(|c: char| c.is_ascii_whitespace())
                .len();
            offset -= chunk.len();
            offset += non_whitespace_len;
            if non_whitespace_len != 0 {
                if offset == len - 1 && chunk.get(non_whitespace_len..) == Some("\n") {
                    return;
                }
                break;
            }
        }
        self.edit([(offset..len, "\n")], None, cx);
    }

    /// Applies a diff to the buffer. If the buffer has changed since the given diff was
    /// calculated, then adjust the diff to account for those changes, and discard any
    /// parts of the diff that conflict with those changes.
    pub fn apply_diff(&mut self, diff: Diff, cx: &mut Context<Self>) -> Option<TransactionId> {
        let snapshot = self.snapshot();
        let mut edits_since = snapshot.edits_since::<usize>(&diff.base_version).peekable();
        let mut delta = 0;
        let adjusted_edits = diff.edits.into_iter().filter_map(|(range, new_text)| {
            while let Some(edit_since) = edits_since.peek() {
                // If the edit occurs after a diff hunk, then it does not
                // affect that hunk.
                if edit_since.old.start > range.end {
                    break;
                }
                // If the edit precedes the diff hunk, then adjust the hunk
                // to reflect the edit.
                else if edit_since.old.end < range.start {
                    delta += edit_since.new_len() as i64 - edit_since.old_len() as i64;
                    edits_since.next();
                }
                // If the edit intersects a diff hunk, then discard that hunk.
                else {
                    return None;
                }
            }

            let start = (range.start as i64 + delta) as usize;
            let end = (range.end as i64 + delta) as usize;
            Some((start..end, new_text))
        });

        self.start_transaction();
        self.text.set_line_ending(diff.line_ending);
        self.edit(adjusted_edits, None, cx);
        self.end_transaction(cx)
    }

    pub fn has_unsaved_edits(&self) -> bool {
        let (last_version, has_unsaved_edits) = self.has_unsaved_edits.take();

        if last_version == self.version {
            self.has_unsaved_edits
                .set((last_version, has_unsaved_edits));
            return has_unsaved_edits;
        }

        let has_edits = self.has_edits_since(&self.saved_version);
        self.has_unsaved_edits
            .set((self.version.clone(), has_edits));
        has_edits
    }

    /// Checks if the buffer has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        if self.capability == Capability::ReadOnly {
            return false;
        }
        if self.has_conflict {
            return true;
        }
        match self.file.as_ref().map(|f| f.disk_state()) {
            Some(DiskState::New) | Some(DiskState::Deleted) => {
                !self.is_empty() && self.has_unsaved_edits()
            }
            _ => self.has_unsaved_edits(),
        }
    }

    /// Marks the buffer as having a conflict regardless of current buffer state.
    pub fn set_conflict(&mut self) {
        self.has_conflict = true;
    }

    /// Checks if the buffer and its file have both changed since the buffer
    /// was last saved or reloaded.
    pub fn has_conflict(&self) -> bool {
        if self.has_conflict {
            return true;
        }
        let Some(file) = self.file.as_ref() else {
            return false;
        };
        match file.disk_state() {
            DiskState::New => false,
            DiskState::Present { mtime, .. } => match self.saved_mtime {
                Some(saved_mtime) => {
                    mtime.bad_is_greater_than(saved_mtime) && self.has_unsaved_edits()
                }
                None => true,
            },
            DiskState::Deleted => false,
            DiskState::Historic { .. } => false,
        }
    }

    /// Gets a [`Subscription`] that tracks all of the changes to the buffer's text.
    pub fn subscribe(&mut self) -> Subscription<usize> {
        self.text.subscribe()
    }

    /// Adds a bit to the list of bits that are set when the buffer's text changes.
    ///
    /// This allows downstream code to check if the buffer's text has changed without
    /// waiting for an effect cycle, which would be required if using eents.
    pub fn record_changes(&mut self, bit: rc::Weak<Cell<bool>>) {
        if let Err(ix) = self
            .change_bits
            .binary_search_by_key(&rc::Weak::as_ptr(&bit), rc::Weak::as_ptr)
        {
            self.change_bits.insert(ix, bit);
        }
    }

    /// Set the change bit for all "listeners".
    pub(super) fn was_changed(&mut self) {
        self.change_bits.retain(|change_bit| {
            change_bit
                .upgrade()
                .inspect(|bit| {
                    _ = bit.replace(true);
                })
                .is_some()
        });
    }
}

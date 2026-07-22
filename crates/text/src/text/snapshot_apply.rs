use super::*;

impl BufferSnapshot {
    pub(crate) fn apply_edit_internal(
        &mut self,
        edits: Vec<(Range<usize>, Arc<str>)>,
        timestamp: clock::Lamport,
    ) -> (EditOperation, Patch<usize>) {
        let mut edits_patch = Patch::default();
        let mut edit_op = EditOperation {
            timestamp,
            version: self.version.clone(),
            ranges: Vec::with_capacity(edits.len()),
            new_text: Vec::with_capacity(edits.len()),
        };
        let mut new_insertions = Vec::new();
        let mut insertion_offset: u32 = 0;
        let mut insertion_slices = Vec::new();

        let mut edits = edits.into_iter().peekable();

        if edits.peek().is_none() {
            return (edit_op, edits_patch);
        }

        let mut new_ropes =
            RopeBuilder::new(self.visible_text.cursor(0), self.deleted_text.cursor(0));
        let mut old_fragments = self.fragments.cursor::<FragmentTextSummary>(&None);
        let mut new_fragments =
            FragmentBuilder::new(old_fragments.slice(&edits.peek().unwrap().0.start, Bias::Right));
        new_ropes.append(new_fragments.summary().text);

        let mut fragment_start = old_fragments.start().visible;
        for (range, new_text) in edits {
            let new_text: Arc<str> = LineEnding::normalize_arc(new_text);
            let fragment_end = old_fragments.end().visible;

            if fragment_end < range.start {
                if fragment_start > old_fragments.start().visible {
                    if fragment_end > fragment_start {
                        let mut suffix = old_fragments.item().unwrap().clone();
                        suffix.len = (fragment_end - fragment_start) as u32;
                        suffix.insertion_offset +=
                            (fragment_start - old_fragments.start().visible) as u32;
                        new_insertions.push(InsertionFragment::insert_new(&suffix));
                        new_ropes.push_fragment(&suffix, suffix.visible);
                        new_fragments.push(suffix, &None);
                    }
                    old_fragments.next();
                }

                let slice = old_fragments.slice(&range.start, Bias::Right);
                new_ropes.append(slice.summary().text);
                new_fragments.append(slice, &None);
                fragment_start = old_fragments.start().visible;
            }

            let full_range_start = FullOffset(range.start + old_fragments.start().deleted);

            if fragment_start < range.start {
                let mut prefix = old_fragments.item().unwrap().clone();
                prefix.len = (range.start - fragment_start) as u32;
                prefix.insertion_offset += (fragment_start - old_fragments.start().visible) as u32;
                prefix.id = Locator::between(&new_fragments.summary().max_id, &prefix.id);
                new_insertions.push(InsertionFragment::insert_new(&prefix));
                new_ropes.push_fragment(&prefix, prefix.visible);
                new_fragments.push(prefix, &None);
                fragment_start = range.start;
            }

            if !new_text.is_empty() {
                let new_start = new_fragments.summary().text.visible;

                let next_fragment_id = old_fragments
                    .item()
                    .map_or(Locator::max_ref(), |old_fragment| &old_fragment.id);
                push_fragments_for_insertion(
                    new_text.as_ref(),
                    timestamp,
                    &mut insertion_offset,
                    &mut new_fragments,
                    &mut new_insertions,
                    &mut insertion_slices,
                    &mut new_ropes,
                    next_fragment_id,
                    timestamp,
                );
                edits_patch.push(Edit {
                    old: fragment_start..fragment_start,
                    new: new_start..new_start + new_text.len(),
                });
            }

            while fragment_start < range.end {
                let fragment = old_fragments.item().unwrap();
                let fragment_end = old_fragments.end().visible;
                let mut intersection = fragment.clone();
                let intersection_end = cmp::min(range.end, fragment_end);
                if fragment.visible {
                    intersection.len = (intersection_end - fragment_start) as u32;
                    intersection.insertion_offset +=
                        (fragment_start - old_fragments.start().visible) as u32;
                    intersection.id =
                        Locator::between(&new_fragments.summary().max_id, &intersection.id);
                    intersection.deletions.push(timestamp);
                    intersection.visible = false;
                }
                if intersection.len > 0 {
                    if fragment.visible && !intersection.visible {
                        let new_start = new_fragments.summary().text.visible;
                        edits_patch.push(Edit {
                            old: fragment_start..intersection_end,
                            new: new_start..new_start,
                        });
                        insertion_slices
                            .push(InsertionSlice::from_fragment(timestamp, &intersection));
                    }
                    new_insertions.push(InsertionFragment::insert_new(&intersection));
                    new_ropes.push_fragment(&intersection, fragment.visible);
                    new_fragments.push(intersection, &None);
                    fragment_start = intersection_end;
                }
                if fragment_end <= range.end {
                    old_fragments.next();
                }
            }

            let full_range_end = FullOffset(range.end + old_fragments.start().deleted);
            edit_op.ranges.push(full_range_start..full_range_end);
            edit_op.new_text.push(new_text);
        }

        if fragment_start > old_fragments.start().visible {
            let fragment_end = old_fragments.end().visible;
            if fragment_end > fragment_start {
                let mut suffix = old_fragments.item().unwrap().clone();
                suffix.len = (fragment_end - fragment_start) as u32;
                suffix.insertion_offset += (fragment_start - old_fragments.start().visible) as u32;
                new_insertions.push(InsertionFragment::insert_new(&suffix));
                new_ropes.push_fragment(&suffix, suffix.visible);
                new_fragments.push(suffix, &None);
            }
            old_fragments.next();
        }

        let suffix = old_fragments.suffix();
        new_ropes.append(suffix.summary().text);
        new_fragments.append(suffix, &None);
        let (visible_text, deleted_text) = new_ropes.finish();
        drop(old_fragments);

        self.fragments = new_fragments.to_sum_tree(&None);
        self.insertions.edit(new_insertions, ());
        self.visible_text = visible_text;
        self.deleted_text = deleted_text;
        self.insertion_slices.extend(insertion_slices);
        (edit_op, edits_patch)
    }
}

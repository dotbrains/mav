use super::*;

impl Buffer {
    pub(crate) fn apply_remote_edit(
        &mut self,
        version: &clock::Global,
        ranges: &[Range<FullOffset>],
        new_text: &[Arc<str>],
        timestamp: clock::Lamport,
    ) {
        if ranges.is_empty() {
            return;
        }

        let edits = ranges.iter().zip(new_text.iter());
        let mut edits_patch = Patch::default();
        let mut insertion_slices = Vec::new();
        let cx = Some(version.clone());
        let mut new_insertions = Vec::new();
        let mut insertion_offset: u32 = 0;
        let mut new_ropes =
            RopeBuilder::new(self.visible_text.cursor(0), self.deleted_text.cursor(0));
        let mut old_fragments = self
            .fragments
            .cursor::<Dimensions<VersionedFullOffset, usize>>(&cx);
        let mut new_fragments = FragmentBuilder::new(
            old_fragments.slice(&VersionedFullOffset::Offset(ranges[0].start), Bias::Left),
        );
        new_ropes.append(new_fragments.summary().text);

        let mut fragment_start = old_fragments.start().0.full_offset();
        for (range, new_text) in edits {
            let fragment_end = old_fragments.end().0.full_offset();

            // If the current fragment ends before this range, then jump ahead to the first fragment
            // that extends past the start of this range, reusing any intervening fragments.
            if fragment_end < range.start {
                // If the current fragment has been partially consumed, then consume the rest of it
                // and advance to the next fragment before slicing.
                if fragment_start > old_fragments.start().0.full_offset() {
                    if fragment_end > fragment_start {
                        let mut suffix = old_fragments.item().unwrap().clone();
                        suffix.len = (fragment_end.0 - fragment_start.0) as u32;
                        suffix.insertion_offset +=
                            (fragment_start - old_fragments.start().0.full_offset()) as u32;
                        new_insertions.push(InsertionFragment::insert_new(&suffix));
                        new_ropes.push_fragment(&suffix, suffix.visible);
                        new_fragments.push(suffix, &None);
                    }
                    old_fragments.next();
                }

                let slice =
                    old_fragments.slice(&VersionedFullOffset::Offset(range.start), Bias::Left);
                new_ropes.append(slice.summary().text);
                new_fragments.append(slice, &None);
                fragment_start = old_fragments.start().0.full_offset();
            }

            // If we are at the end of a non-concurrent fragment, advance to the next one.
            let fragment_end = old_fragments.end().0.full_offset();
            if fragment_end == range.start && fragment_end > fragment_start {
                let mut fragment = old_fragments.item().unwrap().clone();
                fragment.len = (fragment_end.0 - fragment_start.0) as u32;
                fragment.insertion_offset +=
                    (fragment_start - old_fragments.start().0.full_offset()) as u32;
                new_insertions.push(InsertionFragment::insert_new(&fragment));
                new_ropes.push_fragment(&fragment, fragment.visible);
                new_fragments.push(fragment, &None);
                old_fragments.next();
                fragment_start = old_fragments.start().0.full_offset();
            }

            // Skip over insertions that are concurrent to this edit, but have a higher lamport
            // timestamp.
            while let Some(fragment) = old_fragments.item() {
                if fragment_start == range.start && fragment.timestamp > timestamp {
                    new_ropes.push_fragment(fragment, fragment.visible);
                    new_fragments.push(fragment.clone(), &None);
                    old_fragments.next();
                    debug_assert_eq!(fragment_start, range.start);
                } else {
                    break;
                }
            }
            debug_assert!(fragment_start <= range.start);

            // Preserve any portion of the current fragment that precedes this range.
            if fragment_start < range.start {
                let mut prefix = old_fragments.item().unwrap().clone();
                prefix.len = (range.start.0 - fragment_start.0) as u32;
                prefix.insertion_offset +=
                    (fragment_start - old_fragments.start().0.full_offset()) as u32;
                prefix.id = Locator::between(&new_fragments.summary().max_id, &prefix.id);
                new_insertions.push(InsertionFragment::insert_new(&prefix));
                fragment_start = range.start;
                new_ropes.push_fragment(&prefix, prefix.visible);
                new_fragments.push(prefix, &None);
            }

            // Insert the new text before any existing fragments within the range.
            if !new_text.is_empty() {
                let mut old_start = old_fragments.start().1;
                if old_fragments.item().is_some_and(|f| f.visible) {
                    old_start += fragment_start.0 - old_fragments.start().0.full_offset().0;
                }
                let new_start = new_fragments.summary().text.visible;
                let next_fragment_id = old_fragments
                    .item()
                    .map_or(Locator::max_ref(), |old_fragment| &old_fragment.id);
                push_fragments_for_insertion(
                    new_text,
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
                    old: old_start..old_start,
                    new: new_start..new_start + new_text.len(),
                });
            }

            // Advance through every fragment that intersects this range, marking the intersecting
            // portions as deleted.
            while fragment_start < range.end {
                let fragment = old_fragments.item().unwrap();
                let fragment_end = old_fragments.end().0.full_offset();
                let mut intersection = fragment.clone();
                let intersection_end = cmp::min(range.end, fragment_end);
                if version.observed(fragment.timestamp) {
                    intersection.len = (intersection_end.0 - fragment_start.0) as u32;
                    intersection.insertion_offset +=
                        (fragment_start - old_fragments.start().0.full_offset()) as u32;
                    intersection.id =
                        Locator::between(&new_fragments.summary().max_id, &intersection.id);
                    if fragment.was_visible(version, &self.undo_map) {
                        intersection.deletions.push(timestamp);
                        intersection.visible = false;
                        insertion_slices
                            .push(InsertionSlice::from_fragment(timestamp, &intersection));
                    }
                }
                if intersection.len > 0 {
                    if fragment.visible && !intersection.visible {
                        let old_start = old_fragments.start().1
                            + (fragment_start.0 - old_fragments.start().0.full_offset().0);
                        let new_start = new_fragments.summary().text.visible;
                        edits_patch.push(Edit {
                            old: old_start..old_start + intersection.len as usize,
                            new: new_start..new_start,
                        });
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
        }

        // If the current fragment has been partially consumed, then consume the rest of it
        // and advance to the next fragment before slicing.
        if fragment_start > old_fragments.start().0.full_offset() {
            let fragment_end = old_fragments.end().0.full_offset();
            if fragment_end > fragment_start {
                let mut suffix = old_fragments.item().unwrap().clone();
                suffix.len = (fragment_end.0 - fragment_start.0) as u32;
                suffix.insertion_offset +=
                    (fragment_start - old_fragments.start().0.full_offset()) as u32;
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

        self.snapshot.fragments = new_fragments.to_sum_tree(&None);
        self.snapshot.visible_text = visible_text;
        self.snapshot.deleted_text = deleted_text;
        self.snapshot.insertions.edit(new_insertions, ());
        self.snapshot.insertion_slices.extend(insertion_slices);
        self.subscriptions.publish_mut(&edits_patch)
    }

    pub(crate) fn fragment_ids_for_edits<'a>(
        &'a self,
        edit_ids: impl Iterator<Item = &'a clock::Lamport>,
    ) -> Vec<&'a Locator> {
        // Get all of the insertion slices changed by the given edits.
        let mut insertion_slices = Vec::new();
        for edit_id in edit_ids {
            let insertion_slice = InsertionSlice {
                edit_id_value: edit_id.value,
                edit_id_replica_id: edit_id.replica_id,
                insertion_id_value: Lamport::MIN.value,
                insertion_id_replica_id: Lamport::MIN.replica_id,
                range: 0..0,
            };
            let slices = self
                .snapshot
                .insertion_slices
                .iter_from(&insertion_slice)
                .take_while(|slice| {
                    Lamport {
                        value: slice.edit_id_value,
                        replica_id: slice.edit_id_replica_id,
                    } == *edit_id
                });
            insertion_slices.extend(slices)
        }
        insertion_slices.sort_unstable_by_key(|s| {
            (
                Lamport {
                    value: s.insertion_id_value,
                    replica_id: s.insertion_id_replica_id,
                },
                s.range.start,
                Reverse(s.range.end),
            )
        });

        // Get all of the fragments corresponding to these insertion slices.
        let mut fragment_ids = Vec::new();
        let mut insertions_cursor = self.insertions.cursor::<InsertionFragmentKey>(());
        for insertion_slice in &insertion_slices {
            let insertion_id = Lamport {
                value: insertion_slice.insertion_id_value,
                replica_id: insertion_slice.insertion_id_replica_id,
            };
            if insertion_id != insertions_cursor.start().timestamp
                || insertion_slice.range.start > insertions_cursor.start().split_offset
            {
                insertions_cursor.seek_forward(
                    &InsertionFragmentKey {
                        timestamp: insertion_id,
                        split_offset: insertion_slice.range.start,
                    },
                    Bias::Left,
                );
            }
            while let Some(item) = insertions_cursor.item() {
                if item.timestamp != insertion_id || item.split_offset >= insertion_slice.range.end
                {
                    break;
                }
                fragment_ids.push(&item.fragment_id);
                insertions_cursor.next();
            }
        }
        fragment_ids.sort_unstable();
        fragment_ids
    }
}

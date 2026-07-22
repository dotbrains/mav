use super::*;

pub(super) fn push_fragments_for_insertion(
    new_text: &str,
    timestamp: clock::Lamport,
    insertion_offset: &mut u32,
    new_fragments: &mut FragmentBuilder,
    new_insertions: &mut Vec<sum_tree::Edit<InsertionFragment>>,
    insertion_slices: &mut Vec<InsertionSlice>,
    new_ropes: &mut RopeBuilder,
    next_fragment_id: &Locator,
    edit_timestamp: clock::Lamport,
) {
    let mut text_offset = 0;
    while text_offset < new_text.len() {
        let target_end = new_text.len().min(text_offset + MAX_INSERTION_LEN);
        let chunk_end = if target_end == new_text.len() {
            target_end
        } else {
            new_text.floor_char_boundary(target_end)
        };
        if chunk_end == text_offset {
            break;
        }
        let chunk_len = chunk_end - text_offset;

        let fragment = Fragment {
            id: Locator::between(&new_fragments.summary().max_id, next_fragment_id),
            timestamp,
            insertion_offset: *insertion_offset,
            len: chunk_len as u32,
            deletions: Default::default(),
            max_undos: Default::default(),
            visible: true,
        };
        insertion_slices.push(InsertionSlice::from_fragment(edit_timestamp, &fragment));
        new_insertions.push(InsertionFragment::insert_new(&fragment));
        new_fragments.push(fragment, &None);

        *insertion_offset += chunk_len as u32;
        text_offset = chunk_end;
    }
    new_ropes.push_str(new_text);
}

impl Deref for Buffer {
    type Target = BufferSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

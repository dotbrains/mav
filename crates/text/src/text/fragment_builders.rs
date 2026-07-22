use super::*;

/// A chunk of fragments accumulated by [`FragmentBuilder`]. `Tree` chunks are
/// subtrees sliced off the previous fragment tree and are kept intact so they
/// continue to share nodes with it; `Loose` chunks batch individually pushed
/// fragments so they can be turned into a subtree in one shot.
pub(super) enum FragmentChunk {
    Tree(SumTree<Fragment>),
    Loose(Vec<Fragment>),
}

pub(super) struct FragmentBuilder {
    chunks: Vec<FragmentChunk>,
    summary: FragmentSummary,
}

impl FragmentBuilder {
    pub(super) fn new(init: SumTree<Fragment>) -> Self {
        let summary = init.summary().clone();
        let mut chunks = Vec::new();
        if !init.is_empty() {
            chunks.push(FragmentChunk::Tree(init));
        }
        Self { chunks, summary }
    }
    pub(super) fn append(&mut self, items: SumTree<Fragment>, cx: &Option<clock::Global>) {
        if !items.is_empty() {
            self.summary.add_summary(items.summary(), cx);
            self.chunks.push(FragmentChunk::Tree(items));
        }
    }
    pub(super) fn push(&mut self, fragment: Fragment, cx: &Option<clock::Global>) {
        self.summary
            .add_summary(&sum_tree::Item::summary(&fragment, cx), cx);
        match self.chunks.last_mut() {
            Some(FragmentChunk::Loose(fragments)) => fragments.push(fragment),
            _ => self.chunks.push(FragmentChunk::Loose(vec![fragment])),
        }
    }
    pub(super) fn to_sum_tree(self, cx: &Option<clock::Global>) -> SumTree<Fragment> {
        // Appending a `Tree` chunk only touches the right spine and grafts the
        // subtree by cloning `Arc`s, so the untouched regions stay shared with
        // the previous fragment tree. `Loose` runs (newly inserted or rewritten
        // fragments) are built in one pass, parallelizing the large ones.
        let mut tree = SumTree::new(cx);
        for chunk in self.chunks {
            match chunk {
                FragmentChunk::Tree(subtree) => tree.append(subtree, cx),
                FragmentChunk::Loose(fragments) => {
                    if fragments.len() > 1024 {
                        tree.append(SumTree::from_par_iter(fragments, cx), cx);
                    } else {
                        tree.append(SumTree::from_iter(fragments, cx), cx);
                    }
                }
            }
        }
        tree
    }
    pub(super) fn summary(&self) -> &FragmentSummary {
        &self.summary
    }
}

pub(super) struct RopeBuilder<'a> {
    old_visible_cursor: rope::Cursor<'a>,
    old_deleted_cursor: rope::Cursor<'a>,
    new_visible: Rope,
    new_deleted: Rope,
}

impl<'a> RopeBuilder<'a> {
    pub(super) fn new(
        old_visible_cursor: rope::Cursor<'a>,
        old_deleted_cursor: rope::Cursor<'a>,
    ) -> Self {
        Self {
            old_visible_cursor,
            old_deleted_cursor,
            new_visible: Rope::new(),
            new_deleted: Rope::new(),
        }
    }

    pub(super) fn append(&mut self, len: FragmentTextSummary) {
        self.push(len.visible, true, true);
        self.push(len.deleted, false, false);
    }

    pub(super) fn push_fragment(&mut self, fragment: &Fragment, was_visible: bool) {
        debug_assert!(fragment.len > 0);
        self.push(fragment.len as usize, was_visible, fragment.visible)
    }

    pub(super) fn push(&mut self, len: usize, was_visible: bool, is_visible: bool) {
        let text = if was_visible {
            self.old_visible_cursor
                .slice(self.old_visible_cursor.offset() + len)
        } else {
            self.old_deleted_cursor
                .slice(self.old_deleted_cursor.offset() + len)
        };
        if is_visible {
            self.new_visible.append(text);
        } else {
            self.new_deleted.append(text);
        }
    }

    pub(super) fn push_str(&mut self, text: &str) {
        self.new_visible.push(text);
    }

    pub(super) fn finish(mut self) -> (Rope, Rope) {
        self.new_visible.append(self.old_visible_cursor.suffix());
        self.new_deleted.append(self.old_deleted_cursor.suffix());
        (self.new_visible, self.new_deleted)
    }
}

impl<D: TextDimension + Ord, F: FnMut(&FragmentSummary) -> bool> Iterator for Edits<'_, D, F> {
    type Item = (Edit<D>, Range<Anchor>);

    fn next(&mut self) -> Option<Self::Item> {
        let mut pending_edit: Option<Self::Item> = None;
        let cursor = self.fragments_cursor.as_mut()?;

        while let Some(fragment) = cursor.item() {
            if fragment.id < *self.range.start.0 {
                cursor.next();
                continue;
            } else if fragment.id > *self.range.end.0 {
                break;
            }

            if cursor.start().visible > self.visible_cursor.offset() {
                let summary = self.visible_cursor.summary(cursor.start().visible);
                self.old_end.add_assign(&summary);
                self.new_end.add_assign(&summary);
            }

            if pending_edit
                .as_ref()
                .is_some_and(|(change, _)| change.new.end < self.new_end)
            {
                break;
            }

            let start_anchor = Anchor::new(
                fragment.timestamp,
                fragment.insertion_offset,
                Bias::Right,
                self.buffer_id,
            );
            let end_anchor = Anchor::new(
                fragment.timestamp,
                fragment.insertion_offset + fragment.len,
                Bias::Left,
                self.buffer_id,
            );

            if !fragment.was_visible(self.since, self.undos) && fragment.visible {
                let mut visible_end = cursor.end().visible;
                if fragment.id == *self.range.end.0 {
                    visible_end = cmp::min(
                        visible_end,
                        cursor.start().visible
                            + (self.range.end.1 - fragment.insertion_offset) as usize,
                    );
                }

                let fragment_summary = self.visible_cursor.summary(visible_end);
                let mut new_end = self.new_end;
                new_end.add_assign(&fragment_summary);
                if let Some((edit, range)) = pending_edit.as_mut() {
                    edit.new.end = new_end;
                    range.end = end_anchor;
                } else {
                    pending_edit = Some((
                        Edit {
                            old: self.old_end..self.old_end,
                            new: self.new_end..new_end,
                        },
                        start_anchor..end_anchor,
                    ));
                }

                self.new_end = new_end;
            } else if fragment.was_visible(self.since, self.undos) && !fragment.visible {
                let mut deleted_end = cursor.end().deleted;
                if fragment.id == *self.range.end.0 {
                    deleted_end = cmp::min(
                        deleted_end,
                        cursor.start().deleted
                            + (self.range.end.1 - fragment.insertion_offset) as usize,
                    );
                }

                if cursor.start().deleted > self.deleted_cursor.offset() {
                    self.deleted_cursor.seek_forward(cursor.start().deleted);
                }
                let fragment_summary = self.deleted_cursor.summary(deleted_end);
                let mut old_end = self.old_end;
                old_end.add_assign(&fragment_summary);
                if let Some((edit, range)) = pending_edit.as_mut() {
                    edit.old.end = old_end;
                    range.end = end_anchor;
                } else {
                    pending_edit = Some((
                        Edit {
                            old: self.old_end..old_end,
                            new: self.new_end..self.new_end,
                        },
                        start_anchor..end_anchor,
                    ));
                }

                self.old_end = old_end;
            }

            cursor.next();
        }

        pending_edit
    }
}

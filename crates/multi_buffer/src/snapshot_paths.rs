use super::*;

impl MultiBufferSnapshot {
    pub fn path_for_buffer(&self, buffer_id: BufferId) -> Option<&PathKey> {
        Some(&self.buffers.get(&buffer_id)?.path_key)
    }

    pub(crate) fn path_key_index_for_buffer(&self, buffer_id: BufferId) -> Option<PathKeyIndex> {
        let snapshot = self.buffers.get(&buffer_id)?;
        Some(snapshot.path_key_index)
    }

    pub(super) fn first_excerpt_for_buffer(&self, buffer_id: BufferId) -> Option<&Excerpt> {
        let path_key = &self.buffers.get(&buffer_id)?.path_key;
        self.first_excerpt_for_path(path_key)
    }

    pub(super) fn first_excerpt_for_path(&self, path_key: &PathKey) -> Option<&Excerpt> {
        let (_, _, first_excerpt) = self.excerpts.find::<PathKey, _>((), path_key, Bias::Left);
        first_excerpt
    }

    pub fn buffer_for_id(&self, id: BufferId) -> Option<&BufferSnapshot> {
        self.buffers.get(&id).map(|state| &state.buffer_snapshot)
    }

    pub(super) fn try_path_for_anchor(&self, anchor: ExcerptAnchor) -> Option<&PathKey> {
        self.path_keys.get_index(anchor.path.0 as usize)
    }

    pub fn path_for_anchor(&self, anchor: ExcerptAnchor) -> &PathKey {
        self.try_path_for_anchor(anchor)
            .expect("invalid anchor: path was never added to multibuffer")
    }

    /// Returns the excerpt containing range and its offset start within the multibuffer or none if `range` spans multiple excerpts
    pub fn excerpt_containing<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> Option<(&BufferSnapshot, ExcerptRange<text::Anchor>)> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let mut cursor = self.cursor::<MultiBufferOffset, BufferOffset>();
        cursor.seek(&range.start);

        let start_excerpt = cursor.excerpt()?;
        if range.end != range.start {
            cursor.seek_forward(&range.end);
            if cursor.excerpt()? != start_excerpt {
                return None;
            }
        }

        Some((
            start_excerpt.buffer_snapshot(self),
            start_excerpt.range.clone(),
        ))
    }

    pub fn selections_in_range<'a>(
        &'a self,
        range: &'a Range<Anchor>,
        include_local: bool,
    ) -> impl 'a + Iterator<Item = (ReplicaId, bool, CursorShape, Selection<Anchor>)> {
        let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
        cursor.seek(&range.start.seek_target(self), Bias::Left);
        cursor
            .take_while(move |excerpt| {
                let excerpt_start =
                    Anchor::in_buffer(excerpt.path_key_index, excerpt.range.context.start);
                excerpt_start.cmp(&range.end, self).is_le()
            })
            .flat_map(move |excerpt| {
                let buffer_snapshot = excerpt.buffer_snapshot(self);
                let mut query_range = excerpt.range.context.start..excerpt.range.context.end;
                if let Some(excerpt_anchor) = range.start.excerpt_anchor()
                    && excerpt.contains(&excerpt_anchor, self)
                {
                    query_range.start = excerpt_anchor.text_anchor();
                }
                if let Some(excerpt_anchor) = range.end.excerpt_anchor()
                    && excerpt.contains(&excerpt_anchor, self)
                {
                    query_range.end = excerpt_anchor.text_anchor();
                }

                buffer_snapshot
                    .selections_in_range(query_range, include_local)
                    .flat_map(move |(replica_id, line_mode, cursor_shape, selections)| {
                        selections.map(move |selection| {
                            let mut start =
                                Anchor::in_buffer(excerpt.path_key_index, selection.start);
                            let mut end = Anchor::in_buffer(excerpt.path_key_index, selection.end);
                            if range.start.cmp(&start, self).is_gt() {
                                start = range.start;
                            }
                            if range.end.cmp(&end, self).is_lt() {
                                end = range.end;
                            }

                            (
                                replica_id,
                                line_mode,
                                cursor_shape,
                                Selection {
                                    id: selection.id,
                                    start,
                                    end,
                                    reversed: selection.reversed,
                                    goal: selection.goal,
                                },
                            )
                        })
                    })
            })
    }
}

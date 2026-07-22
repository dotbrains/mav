use super::*;

pub(crate) struct FoldMapWriter<'a>(pub(crate) &'a mut FoldMap);

impl FoldMapWriter<'_> {
    #[ztracing::instrument(skip_all)]
    pub(crate) fn fold<T: ToOffset>(
        &mut self,
        ranges: impl IntoIterator<Item = (Range<T>, FoldPlaceholder)>,
    ) -> (FoldSnapshot, Vec<FoldEdit>) {
        let mut edits = Vec::new();
        let mut folds = Vec::new();
        let snapshot = self.0.snapshot.inlay_snapshot.clone();
        for (range, fold_text) in ranges.into_iter() {
            let buffer = &snapshot.buffer;
            let range = range.start.to_offset(buffer)..range.end.to_offset(buffer);

            // Ignore any empty ranges.
            if range.start == range.end {
                continue;
            }

            let fold_range = buffer.anchor_after(range.start)..buffer.anchor_before(range.end);
            // For now, ignore any ranges that span an excerpt boundary.
            if buffer
                .anchor_range_to_buffer_anchor_range(fold_range.clone())
                .is_none()
            {
                continue;
            }

            folds.push(Fold {
                id: FoldId(post_inc(&mut self.0.next_fold_id.0)),
                range: FoldRange(fold_range),
                placeholder: fold_text,
            });

            let inlay_range =
                snapshot.to_inlay_offset(range.start)..snapshot.to_inlay_offset(range.end);
            edits.push(InlayEdit {
                old: inlay_range.clone(),
                new: inlay_range,
            });
        }

        let buffer = &snapshot.buffer;
        folds.sort_unstable_by(|a, b| sum_tree::SeekTarget::cmp(&a.range, &b.range, buffer));

        self.0.snapshot.folds = {
            let mut new_tree = SumTree::new(buffer);
            let mut cursor = self.0.snapshot.folds.cursor::<FoldRange>(buffer);
            for fold in folds {
                self.0.snapshot.fold_metadata_by_id.insert(
                    fold.id,
                    FoldMetadata {
                        range: fold.range.clone(),
                        width: None,
                    },
                );
                new_tree.append(cursor.slice(&fold.range, Bias::Right), buffer);
                new_tree.push(fold, buffer);
            }
            new_tree.append(cursor.suffix(), buffer);
            new_tree
        };

        let edits = consolidate_inlay_edits(edits);
        let edits = self.0.sync(snapshot.clone(), edits);
        (self.0.snapshot.clone(), edits)
    }

    /// Removes any folds with the given ranges.
    #[ztracing::instrument(skip_all)]
    pub(crate) fn remove_folds<T: ToOffset>(
        &mut self,
        ranges: impl IntoIterator<Item = Range<T>>,
        type_id: TypeId,
    ) -> (FoldSnapshot, Vec<FoldEdit>) {
        self.remove_folds_with(
            ranges,
            |fold| fold.placeholder.type_tag == Some(type_id),
            false,
        )
    }

    /// Removes any folds whose ranges intersect the given ranges.
    #[ztracing::instrument(skip_all)]
    pub(crate) fn unfold_intersecting<T: ToOffset>(
        &mut self,
        ranges: impl IntoIterator<Item = Range<T>>,
        inclusive: bool,
    ) -> (FoldSnapshot, Vec<FoldEdit>) {
        self.remove_folds_with(ranges, |_| true, inclusive)
    }

    /// Removes any folds that intersect the given ranges and for which the given predicate
    /// returns true.
    #[ztracing::instrument(skip_all)]
    fn remove_folds_with<T: ToOffset>(
        &mut self,
        ranges: impl IntoIterator<Item = Range<T>>,
        should_unfold: impl Fn(&Fold) -> bool,
        inclusive: bool,
    ) -> (FoldSnapshot, Vec<FoldEdit>) {
        let mut edits = Vec::new();
        let mut fold_ixs_to_delete = Vec::new();
        let snapshot = self.0.snapshot.inlay_snapshot.clone();
        let buffer = &snapshot.buffer;
        for range in ranges.into_iter() {
            let range = range.start.to_offset(buffer)..range.end.to_offset(buffer);
            let mut folds_cursor =
                intersecting_folds(&snapshot, &self.0.snapshot.folds, range.clone(), inclusive);
            while let Some(fold) = folds_cursor.item() {
                let offset_range =
                    fold.range.start.to_offset(buffer)..fold.range.end.to_offset(buffer);
                if should_unfold(fold) {
                    if offset_range.end > offset_range.start {
                        let inlay_range = snapshot.to_inlay_offset(offset_range.start)
                            ..snapshot.to_inlay_offset(offset_range.end);
                        edits.push(InlayEdit {
                            old: inlay_range.clone(),
                            new: inlay_range,
                        });
                    }
                    fold_ixs_to_delete.push(*folds_cursor.start());
                    self.0.snapshot.fold_metadata_by_id.remove(&fold.id);
                }
                folds_cursor.next();
            }
        }

        fold_ixs_to_delete.sort_unstable();
        fold_ixs_to_delete.dedup();

        self.0.snapshot.folds = {
            let mut cursor = self.0.snapshot.folds.cursor::<MultiBufferOffset>(buffer);
            let mut folds = SumTree::new(buffer);
            for fold_ix in fold_ixs_to_delete {
                folds.append(cursor.slice(&fold_ix, Bias::Right), buffer);
                cursor.next();
            }
            folds.append(cursor.suffix(), buffer);
            folds
        };

        let edits = consolidate_inlay_edits(edits);
        let edits = self.0.sync(snapshot.clone(), edits);
        (self.0.snapshot.clone(), edits)
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn update_fold_widths(
        &mut self,
        new_widths: impl IntoIterator<Item = (ChunkRendererId, Pixels)>,
    ) -> (FoldSnapshot, Vec<FoldEdit>) {
        let mut edits = Vec::new();
        let inlay_snapshot = self.0.snapshot.inlay_snapshot.clone();
        let buffer = &inlay_snapshot.buffer;

        for (id, new_width) in new_widths {
            let ChunkRendererId::Fold(id) = id else {
                continue;
            };
            if let Some(metadata) = self.0.snapshot.fold_metadata_by_id.get(&id).cloned()
                && Some(new_width) != metadata.width
            {
                let buffer_start = metadata.range.start.to_offset(buffer);
                let buffer_end = metadata.range.end.to_offset(buffer);
                let inlay_range = inlay_snapshot.to_inlay_offset(buffer_start)
                    ..inlay_snapshot.to_inlay_offset(buffer_end);
                edits.push(InlayEdit {
                    old: inlay_range.clone(),
                    new: inlay_range.clone(),
                });

                self.0.snapshot.fold_metadata_by_id.insert(
                    id,
                    FoldMetadata {
                        range: metadata.range,
                        width: Some(new_width),
                    },
                );
            }
        }

        let edits = consolidate_inlay_edits(edits);
        let edits = self.0.sync(inlay_snapshot, edits);
        (self.0.snapshot.clone(), edits)
    }
}

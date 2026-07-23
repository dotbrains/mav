use super::*;

/// Decides where the fold indicators should be; also tracks parts of a source file that are currently folded.
///
/// See the [`display_map` module documentation](crate::display_map) for more information.
pub struct FoldMap {
    pub(crate) snapshot: FoldSnapshot,
    pub(crate) next_fold_id: FoldId,
}

impl FoldMap {
    #[ztracing::instrument(skip_all)]
    pub fn new(inlay_snapshot: InlaySnapshot) -> (Self, FoldSnapshot) {
        let this = Self {
            snapshot: FoldSnapshot {
                folds: SumTree::new(&inlay_snapshot.buffer),
                transforms: SumTree::from_item(
                    Transform {
                        summary: TransformSummary {
                            input: inlay_snapshot.text_summary(),
                            output: inlay_snapshot.text_summary(),
                        },
                        placeholder: None,
                    },
                    (),
                ),
                inlay_snapshot: inlay_snapshot,
                version: 0,
                fold_metadata_by_id: TreeMap::default(),
            },
            next_fold_id: FoldId::default(),
        };
        let snapshot = this.snapshot.clone();
        (this, snapshot)
    }

    #[ztracing::instrument(skip_all)]
    pub fn read(
        &mut self,
        inlay_snapshot: InlaySnapshot,
        edits: Vec<InlayEdit>,
    ) -> (FoldSnapshot, Vec<FoldEdit>) {
        let edits = self.sync(inlay_snapshot, edits);
        self.check_invariants();
        (self.snapshot.clone(), edits)
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn write(
        &mut self,
        inlay_snapshot: InlaySnapshot,
        edits: Vec<InlayEdit>,
    ) -> (FoldMapWriter<'_>, FoldSnapshot, Vec<FoldEdit>) {
        let (snapshot, edits) = self.read(inlay_snapshot, edits);
        (FoldMapWriter(self), snapshot, edits)
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn check_invariants(&self) {
        if cfg!(test) {
            assert_eq!(
                self.snapshot.transforms.summary().input.len,
                self.snapshot.inlay_snapshot.len().0,
                "transform tree does not match inlay snapshot's length"
            );

            let mut prev_transform_isomorphic = false;
            for transform in self.snapshot.transforms.iter() {
                if !transform.is_fold() && prev_transform_isomorphic {
                    panic!(
                        "found adjacent isomorphic transforms: {:?}",
                        self.snapshot.transforms.items(())
                    );
                }
                prev_transform_isomorphic = !transform.is_fold();
            }

            let mut folds = self.snapshot.folds.iter().peekable();
            while let Some(fold) = folds.next() {
                if let Some(next_fold) = folds.peek() {
                    let comparison = fold.range.cmp(&next_fold.range, self.snapshot.buffer());
                    assert!(comparison.is_le());
                }
            }
        }
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn sync(
        &mut self,
        inlay_snapshot: InlaySnapshot,
        inlay_edits: Vec<InlayEdit>,
    ) -> Vec<FoldEdit> {
        if inlay_edits.is_empty() {
            if self.snapshot.inlay_snapshot.version != inlay_snapshot.version {
                self.snapshot.version += 1;
            }
            self.snapshot.inlay_snapshot = inlay_snapshot;
            Vec::new()
        } else {
            let mut inlay_edits_iter = inlay_edits.iter().cloned().peekable();

            let mut new_transforms = SumTree::<Transform>::default();
            let mut cursor = self.snapshot.transforms.cursor::<InlayOffset>(());
            cursor.seek(&InlayOffset(MultiBufferOffset(0)), Bias::Right);

            while let Some(mut edit) = inlay_edits_iter.next() {
                if let Some(item) = cursor.item()
                    && !item.is_fold()
                {
                    new_transforms.update_last(
                        |transform| {
                            if !transform.is_fold() {
                                transform.summary.add_summary(&item.summary, ());
                                cursor.next();
                            }
                        },
                        (),
                    );
                }
                new_transforms.append(cursor.slice(&edit.old.start, Bias::Left), ());
                edit.new.start -= edit.old.start - *cursor.start();
                edit.old.start = *cursor.start();

                cursor.seek(&edit.old.end, Bias::Right);
                cursor.next();

                let mut delta = edit.new_len() as isize - edit.old_len() as isize;
                loop {
                    edit.old.end = *cursor.start();

                    if let Some(next_edit) = inlay_edits_iter.peek() {
                        if next_edit.old.start > edit.old.end {
                            break;
                        }

                        let next_edit = inlay_edits_iter.next().unwrap();
                        delta += next_edit.new_len() as isize - next_edit.old_len() as isize;

                        if next_edit.old.end >= edit.old.end {
                            edit.old.end = next_edit.old.end;
                            cursor.seek(&edit.old.end, Bias::Right);
                            cursor.next();
                        }
                    } else {
                        break;
                    }
                }

                edit.new.end = InlayOffset(MultiBufferOffset(
                    ((edit.new.start + edit.old_len()).0.0 as isize + delta) as usize,
                ));

                let anchor = inlay_snapshot
                    .buffer
                    .anchor_before(inlay_snapshot.to_buffer_offset(edit.new.start));
                let mut folds_cursor = self
                    .snapshot
                    .folds
                    .cursor::<FoldRange>(&inlay_snapshot.buffer);
                folds_cursor.seek(&FoldRange(anchor..Anchor::Max), Bias::Left);

                let mut folds = iter::from_fn({
                    let inlay_snapshot = &inlay_snapshot;
                    move || {
                        let item = folds_cursor.item().map(|fold| {
                            let buffer_start = fold.range.start.to_offset(&inlay_snapshot.buffer);
                            let buffer_end = fold.range.end.to_offset(&inlay_snapshot.buffer);
                            (
                                fold.clone(),
                                inlay_snapshot.to_inlay_offset(buffer_start)
                                    ..inlay_snapshot.to_inlay_offset(buffer_end),
                            )
                        });
                        folds_cursor.next();
                        item
                    }
                })
                .peekable();

                while folds
                    .peek()
                    .is_some_and(|(_, fold_range)| fold_range.start < edit.new.end)
                {
                    let (fold, mut fold_range) = folds.next().unwrap();
                    let sum = new_transforms.summary();

                    assert!(fold_range.start.0 >= sum.input.len);

                    while folds.peek().is_some_and(|(next_fold, next_fold_range)| {
                        next_fold_range.start < fold_range.end
                            || (next_fold_range.start == fold_range.end
                                && fold.placeholder.merge_adjacent
                                && next_fold.placeholder.merge_adjacent)
                    }) {
                        let (_, next_fold_range) = folds.next().unwrap();
                        if next_fold_range.end > fold_range.end {
                            fold_range.end = next_fold_range.end;
                        }
                    }

                    if fold_range.start.0 > sum.input.len {
                        let text_summary = inlay_snapshot
                            .text_summary_for_range(InlayOffset(sum.input.len)..fold_range.start);
                        push_isomorphic(&mut new_transforms, text_summary);
                    }

                    if fold_range.end > fold_range.start {
                        pub(crate) const ELLIPSIS: &str = "⋯";

                        let placeholder_text: SharedString = fold
                            .placeholder
                            .collapsed_text
                            .clone()
                            .unwrap_or_else(|| ELLIPSIS.into());
                        let chars_bitmap = placeholder_text
                            .char_indices()
                            .fold(0u128, |bitmap, (idx, _)| {
                                bitmap | 1u128.unbounded_shl(idx as u32)
                            });

                        let fold_id = fold.id;
                        new_transforms.push(
                            Transform {
                                summary: TransformSummary {
                                    output: MBTextSummary::from(placeholder_text.as_ref()),
                                    input: inlay_snapshot
                                        .text_summary_for_range(fold_range.start..fold_range.end),
                                },
                                placeholder: Some(TransformPlaceholder {
                                    text: placeholder_text,
                                    chars: chars_bitmap,
                                    renderer: ChunkRenderer {
                                        id: ChunkRendererId::Fold(fold.id),
                                        render: Arc::new(move |cx| {
                                            (fold.placeholder.render)(
                                                fold_id,
                                                fold.range.0.clone(),
                                                cx.context,
                                            )
                                        }),
                                        constrain_width: fold.placeholder.constrain_width,
                                        measured_width: self.snapshot.fold_width(&fold_id),
                                    },
                                }),
                            },
                            (),
                        );
                    }
                }

                let sum = new_transforms.summary();
                if sum.input.len < edit.new.end.0 {
                    let text_summary = inlay_snapshot
                        .text_summary_for_range(InlayOffset(sum.input.len)..edit.new.end);
                    push_isomorphic(&mut new_transforms, text_summary);
                }
            }

            new_transforms.append(cursor.suffix(), ());
            if new_transforms.is_empty() {
                let text_summary = inlay_snapshot.text_summary();
                push_isomorphic(&mut new_transforms, text_summary);
            }

            drop(cursor);

            let mut fold_edits = Vec::with_capacity(inlay_edits.len());
            {
                let mut old_transforms = self
                    .snapshot
                    .transforms
                    .cursor::<Dimensions<InlayOffset, FoldOffset>>(());
                let mut new_transforms =
                    new_transforms.cursor::<Dimensions<InlayOffset, FoldOffset>>(());

                for mut edit in inlay_edits {
                    old_transforms.seek(&edit.old.start, Bias::Left);
                    if old_transforms.item().is_some_and(|t| t.is_fold()) {
                        edit.old.start = old_transforms.start().0;
                    }
                    let old_start =
                        old_transforms.start().1.0 + (edit.old.start - old_transforms.start().0);

                    old_transforms.seek_forward(&edit.old.end, Bias::Right);
                    if old_transforms.item().is_some_and(|t| t.is_fold()) {
                        old_transforms.next();
                        edit.old.end = old_transforms.start().0;
                    }
                    let old_end =
                        old_transforms.start().1.0 + (edit.old.end - old_transforms.start().0);

                    new_transforms.seek(&edit.new.start, Bias::Left);
                    if new_transforms.item().is_some_and(|t| t.is_fold()) {
                        edit.new.start = new_transforms.start().0;
                    }
                    let new_start =
                        new_transforms.start().1.0 + (edit.new.start - new_transforms.start().0);

                    new_transforms.seek_forward(&edit.new.end, Bias::Right);
                    if new_transforms.item().is_some_and(|t| t.is_fold()) {
                        new_transforms.next();
                        edit.new.end = new_transforms.start().0;
                    }
                    let new_end =
                        new_transforms.start().1.0 + (edit.new.end - new_transforms.start().0);

                    fold_edits.push(FoldEdit {
                        old: FoldOffset(old_start)..FoldOffset(old_end),
                        new: FoldOffset(new_start)..FoldOffset(new_end),
                    });
                }

                fold_edits = consolidate_fold_edits(fold_edits);
            }

            self.snapshot.transforms = new_transforms;
            self.snapshot.inlay_snapshot = inlay_snapshot;
            self.snapshot.version += 1;
            fold_edits
        }
    }
}

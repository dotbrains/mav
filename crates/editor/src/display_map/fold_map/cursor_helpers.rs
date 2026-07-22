use super::*;

pub struct FoldPointCursor<'transforms> {
    cursor: Cursor<'transforms, 'static, Transform, Dimensions<InlayPoint, FoldPoint>>,
}

impl FoldPointCursor<'_> {
    /// Resets the cursor to the start so it can seek backward again.
    pub fn reset(&mut self) {
        self.cursor.reset();
    }

    #[ztracing::instrument(skip_all)]
    pub fn map(&mut self, point: InlayPoint, bias: Bias) -> FoldPoint {
        let cursor = &mut self.cursor;
        if cursor.did_seek() {
            cursor.seek_forward(&point, Bias::Right);
        } else {
            cursor.seek(&point, Bias::Right);
        }
        if cursor.item().is_some_and(|t| t.is_fold()) {
            if bias == Bias::Left || point == cursor.start().0 {
                cursor.start().1
            } else {
                cursor.end().1
            }
        } else {
            let overshoot = point.0 - cursor.start().0.0;
            FoldPoint(cmp::min(cursor.start().1.0 + overshoot, cursor.end().1.0))
        }
    }
}

pub(crate) fn push_isomorphic(transforms: &mut SumTree<Transform>, summary: MBTextSummary) {
    let mut did_merge = false;
    transforms.update_last(
        |last| {
            if !last.is_fold() {
                last.summary.input += summary;
                last.summary.output += summary;
                did_merge = true;
            }
        },
        (),
    );
    if !did_merge {
        transforms.push(
            Transform {
                summary: TransformSummary {
                    input: summary,
                    output: summary,
                },
                placeholder: None,
            },
            (),
        )
    }
}

pub(crate) fn intersecting_folds<'a>(
    inlay_snapshot: &'a InlaySnapshot,
    folds: &'a SumTree<Fold>,
    range: Range<MultiBufferOffset>,
    inclusive: bool,
) -> FilterCursor<'a, 'a, impl 'a + FnMut(&FoldSummary) -> bool, Fold, MultiBufferOffset> {
    let buffer = &inlay_snapshot.buffer;
    let start = buffer.anchor_before(range.start.to_offset(buffer));
    let end = buffer.anchor_after(range.end.to_offset(buffer));
    let mut cursor = folds.filter::<_, MultiBufferOffset>(buffer, move |summary| {
        let start_cmp = start.cmp(&summary.max_end, buffer);
        let end_cmp = end.cmp(&summary.min_start, buffer);

        if inclusive {
            start_cmp <= Ordering::Equal && end_cmp >= Ordering::Equal
        } else {
            start_cmp == Ordering::Less && end_cmp == Ordering::Greater
        }
    });
    cursor.next();
    cursor
}

pub(crate) fn consolidate_inlay_edits(mut edits: Vec<InlayEdit>) -> Vec<InlayEdit> {
    edits.sort_unstable_by(|a, b| {
        a.old
            .start
            .cmp(&b.old.start)
            .then_with(|| b.old.end.cmp(&a.old.end))
    });

    let _old_alloc_ptr = edits.as_ptr();
    let mut inlay_edits = edits.into_iter();

    if let Some(mut first_edit) = inlay_edits.next() {
        // This code relies on reusing allocations from the Vec<_> - at the time of writing .flatten() prevents them.
        #[allow(clippy::filter_map_identity)]
        let mut v: Vec<_> = inlay_edits
            .scan(&mut first_edit, |prev_edit, edit| {
                if prev_edit.old.end >= edit.old.start {
                    prev_edit.old.end = prev_edit.old.end.max(edit.old.end);
                    prev_edit.new.start = prev_edit.new.start.min(edit.new.start);
                    prev_edit.new.end = prev_edit.new.end.max(edit.new.end);
                    Some(None) // Skip this edit, it's merged
                } else {
                    let prev = std::mem::replace(*prev_edit, edit);
                    Some(Some(prev)) // Yield the previous edit
                }
            })
            .filter_map(|x| x)
            .collect();
        v.push(first_edit.clone());
        debug_assert_eq!(_old_alloc_ptr, v.as_ptr(), "Inlay edits were reallocated");
        v
    } else {
        vec![]
    }
}

pub(crate) fn consolidate_fold_edits(mut edits: Vec<FoldEdit>) -> Vec<FoldEdit> {
    edits.sort_unstable_by(|a, b| {
        a.old
            .start
            .cmp(&b.old.start)
            .then_with(|| b.old.end.cmp(&a.old.end))
    });
    let _old_alloc_ptr = edits.as_ptr();
    let mut fold_edits = edits.into_iter();

    if let Some(mut first_edit) = fold_edits.next() {
        // This code relies on reusing allocations from the Vec<_> - at the time of writing .flatten() prevents them.
        #[allow(clippy::filter_map_identity)]
        let mut v: Vec<_> = fold_edits
            .scan(&mut first_edit, |prev_edit, edit| {
                if prev_edit.old.end >= edit.old.start {
                    prev_edit.old.end = prev_edit.old.end.max(edit.old.end);
                    prev_edit.new.start = prev_edit.new.start.min(edit.new.start);
                    prev_edit.new.end = prev_edit.new.end.max(edit.new.end);
                    Some(None) // Skip this edit, it's merged
                } else {
                    let prev = std::mem::replace(*prev_edit, edit);
                    Some(Some(prev)) // Yield the previous edit
                }
            })
            .filter_map(|x| x)
            .collect();
        v.push(first_edit.clone());
        v
    } else {
        vec![]
    }
}

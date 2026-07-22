use super::*;

pub struct InlayPointCursor<'transforms> {
    cursor: Cursor<'transforms, 'static, Transform, Dimensions<Point, InlayPoint>>,
    transforms: &'transforms SumTree<Transform>,
}

impl InlayPointCursor<'_> {
    #[ztracing::instrument(skip_all)]
    pub fn map(&mut self, point: Point, bias: Bias) -> InlayPoint {
        let cursor = &mut self.cursor;
        if cursor.did_seek() {
            cursor.seek_forward(&point, Bias::Left);
        } else {
            cursor.seek(&point, Bias::Left);
        }
        loop {
            match cursor.item() {
                Some(Transform::Isomorphic(_)) => {
                    if point == cursor.end().0 {
                        while let Some(Transform::Inlay(inlay)) = cursor.next_item() {
                            if bias == Bias::Left && inlay.position.bias() == Bias::Right {
                                break;
                            } else {
                                cursor.next();
                            }
                        }
                        return cursor.end().1;
                    } else {
                        let overshoot = point - cursor.start().0;
                        return InlayPoint(cursor.start().1.0 + overshoot);
                    }
                }
                Some(Transform::Inlay(inlay)) => {
                    if inlay.position.bias() == Bias::Left || bias == Bias::Right {
                        cursor.next();
                    } else {
                        return cursor.start().1;
                    }
                }
                None => {
                    return InlayPoint(self.transforms.summary().output.lines);
                }
            }
        }
    }
}

/// Forward-only cursor that maps buffer-offset ranges to the inlay-point ranges
/// covering only actual buffer text (excluding inlay text), reusing its tree
/// position across calls.
///
/// This is the streaming equivalent of
/// [`InlaySnapshot::buffer_offset_to_inlay_ranges`] composed with
/// [`InlaySnapshot::to_point`]. Because the cursor only seeks forward, callers
/// must provide ranges with non-decreasing offsets.
pub struct BufferOffsetToInlayPointCursor<'a> {
    snapshot: &'a InlaySnapshot,
    cursor: Cursor<'a, 'static, Transform, Dimensions<MultiBufferOffset, InlayPoint>>,
}

impl BufferOffsetToInlayPointCursor<'_> {
    /// Resets the cursor to the start so it can seek backward again.
    pub fn reset(&mut self) {
        self.cursor.reset();
    }

    pub fn map(&mut self, range: Range<MultiBufferOffset>) -> SmallVec<[Range<InlayPoint>; 1]> {
        let buffer = &self.snapshot.buffer;
        let cursor = &mut self.cursor;
        if cursor.did_seek() {
            cursor.seek_forward(&range.start, Bias::Right);
        } else {
            cursor.seek(&range.start, Bias::Right);
        }

        let mut result = SmallVec::new();
        loop {
            match cursor.item() {
                Some(Transform::Isomorphic(_)) => {
                    let seg_buffer_start = cursor.start().0;
                    let seg_buffer_end = cursor.end().0;
                    let seg_inlay_point_start = cursor.start().1;

                    let overlap_start = cmp::max(range.start, seg_buffer_start);
                    let overlap_end = cmp::min(range.end, seg_buffer_end);

                    if overlap_start < overlap_end {
                        let seg_point_start = buffer.offset_to_point(seg_buffer_start);
                        let start = InlayPoint(
                            seg_inlay_point_start.0
                                + (buffer.offset_to_point(overlap_start) - seg_point_start),
                        );
                        let end = InlayPoint(
                            seg_inlay_point_start.0
                                + (buffer.offset_to_point(overlap_end) - seg_point_start),
                        );
                        result.push(start..end);
                    }

                    // Leave the cursor on the transform containing `range.end`
                    // rather than advancing past it, so a subsequent call with a
                    // larger (but possibly same-transform) start does not seek
                    // backward.
                    if seg_buffer_end >= range.end {
                        break;
                    }
                    cursor.next();
                }
                Some(Transform::Inlay(_)) => cursor.next(),
                None => break,
            }
        }
        result
    }
}

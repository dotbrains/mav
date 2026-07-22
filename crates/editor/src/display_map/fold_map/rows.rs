use super::*;

#[derive(Clone)]
pub struct FoldRows<'a> {
    cursor: Cursor<'a, 'static, Transform, Dimensions<FoldPoint, InlayPoint>>,
    input_rows: InlayBufferRows<'a>,
    fold_point: FoldPoint,
}

impl FoldRows<'_> {
    #[ztracing::instrument(skip_all)]
    pub(crate) fn seek(&mut self, row: u32) {
        let fold_point = FoldPoint::new(row, 0);
        self.cursor.seek(&fold_point, Bias::Left);
        let overshoot = fold_point.0 - self.cursor.start().0.0;
        let inlay_point = InlayPoint(self.cursor.start().1.0 + overshoot);
        self.input_rows.seek(inlay_point.row());
        self.fold_point = fold_point;
    }
}

impl Iterator for FoldRows<'_> {
    type Item = RowInfo;

    #[ztracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        let mut traversed_fold = false;
        while self.fold_point > self.cursor.end().0 {
            self.cursor.next();
            traversed_fold = true;
            if self.cursor.item().is_none() {
                break;
            }
        }

        if self.cursor.item().is_some() {
            if traversed_fold {
                self.input_rows.seek(self.cursor.start().1.0.row);
                self.input_rows.next();
            }
            *self.fold_point.row_mut() += 1;
            self.input_rows.next()
        } else {
            None
        }
    }
}

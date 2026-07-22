use super::*;

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct FoldPoint(pub Point);

impl FoldPoint {
    pub fn new(row: u32, column: u32) -> Self {
        Self(Point::new(row, column))
    }

    pub fn row(self) -> u32 {
        self.0.row
    }

    pub fn column(self) -> u32 {
        self.0.column
    }

    pub fn row_mut(&mut self) -> &mut u32 {
        &mut self.0.row
    }

    #[cfg(test)]
    pub fn column_mut(&mut self) -> &mut u32 {
        &mut self.0.column
    }

    #[ztracing::instrument(skip_all)]
    pub fn to_inlay_point(self, snapshot: &FoldSnapshot) -> InlayPoint {
        let (start, _, _) = snapshot
            .transforms
            .find::<Dimensions<FoldPoint, InlayPoint>, _>((), &self, Bias::Right);
        let overshoot = self.0 - start.0.0;
        InlayPoint(start.1.0 + overshoot)
    }

    #[ztracing::instrument(skip_all)]
    pub fn to_offset(self, snapshot: &FoldSnapshot) -> FoldOffset {
        let (start, _, item) = snapshot
            .transforms
            .find::<Dimensions<FoldPoint, TransformSummary>, _>((), &self, Bias::Right);
        let overshoot = self.0 - start.1.output.lines;
        let mut offset = start.1.output.len;
        if !overshoot.is_zero() {
            let transform = item.expect("display point out of range");
            assert!(transform.placeholder.is_none());
            let end_inlay_offset = snapshot
                .inlay_snapshot
                .to_offset(InlayPoint(start.1.input.lines + overshoot));
            offset += end_inlay_offset.0 - start.1.input.len;
        }
        FoldOffset(offset)
    }
}

impl<'a> sum_tree::Dimension<'a, TransformSummary> for FoldPoint {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a TransformSummary, _: ()) {
        self.0 += &summary.output.lines;
    }
}

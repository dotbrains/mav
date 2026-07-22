use super::*;

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct FoldOffset(pub MultiBufferOffset);

impl FoldOffset {
    #[ztracing::instrument(skip_all)]
    pub fn to_point(self, snapshot: &FoldSnapshot) -> FoldPoint {
        let (start, _, item) = snapshot
            .transforms
            .find::<Dimensions<FoldOffset, TransformSummary>, _>((), &self, Bias::Right);
        let overshoot = if item.is_none_or(|t| t.is_fold()) {
            Point::new(0, (self.0 - start.0.0) as u32)
        } else {
            let inlay_offset = start.1.input.len + (self - start.0);
            let inlay_point = snapshot.inlay_snapshot.to_point(InlayOffset(inlay_offset));
            inlay_point.0 - start.1.input.lines
        };
        FoldPoint(start.1.output.lines + overshoot)
    }

    #[cfg(test)]
    #[ztracing::instrument(skip_all)]
    pub fn to_inlay_offset(self, snapshot: &FoldSnapshot) -> InlayOffset {
        let (start, _, _) = snapshot
            .transforms
            .find::<Dimensions<FoldOffset, InlayOffset>, _>((), &self, Bias::Right);
        let overshoot = self - start.0;
        InlayOffset(start.1.0 + overshoot)
    }
}

impl Add for FoldOffset {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for FoldOffset {
    type Output = <MultiBufferOffset as Sub>::Output;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl<T> SubAssign<T> for FoldOffset
where
    MultiBufferOffset: SubAssign<T>,
{
    fn sub_assign(&mut self, rhs: T) {
        self.0 -= rhs;
    }
}

impl<T> Add<T> for FoldOffset
where
    MultiBufferOffset: Add<T, Output = MultiBufferOffset>,
{
    type Output = Self;

    fn add(self, rhs: T) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl AddAssign for FoldOffset {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl<T> AddAssign<T> for FoldOffset
where
    MultiBufferOffset: AddAssign<T>,
{
    fn add_assign(&mut self, rhs: T) {
        self.0 += rhs;
    }
}

impl<'a> sum_tree::Dimension<'a, TransformSummary> for FoldOffset {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a TransformSummary, _: ()) {
        self.0 += summary.output.len;
    }
}

impl<'a> sum_tree::Dimension<'a, TransformSummary> for InlayPoint {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a TransformSummary, _: ()) {
        self.0 += &summary.input.lines;
    }
}

impl<'a> sum_tree::Dimension<'a, TransformSummary> for InlayOffset {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a TransformSummary, _: ()) {
        self.0 += summary.input.len;
    }
}

pub type FoldEdit = Edit<FoldOffset>;

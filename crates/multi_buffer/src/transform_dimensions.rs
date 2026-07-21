use super::*;

/// Offset into the non-deleted text of the multibuffer.
pub(super) type ExcerptOffset = ExcerptDimension<MultiBufferOffset>;

#[derive(Clone)]
pub(super) struct DiffTransforms<MBD> {
    pub(super) output_dimension: OutputDimension<MBD>,
    pub(super) excerpt_dimension: ExcerptDimension<MBD>,
}

impl<'a, MBD: MultiBufferDimension> Dimension<'a, DiffTransformSummary> for DiffTransforms<MBD> {
    fn zero(cx: <DiffTransformSummary as sum_tree::Summary>::Context<'_>) -> Self {
        Self {
            output_dimension: OutputDimension::zero(cx),
            excerpt_dimension: <ExcerptDimension<MBD> as Dimension<'a, DiffTransformSummary>>::zero(
                cx,
            ),
        }
    }

    fn add_summary(
        &mut self,
        summary: &'a DiffTransformSummary,
        cx: <DiffTransformSummary as sum_tree::Summary>::Context<'_>,
    ) {
        self.output_dimension.add_summary(summary, cx);
        self.excerpt_dimension.add_summary(summary, cx);
    }
}

impl DiffTransformSummary {
    pub(super) fn excerpt_len(&self) -> ExcerptOffset {
        ExcerptDimension(self.input.len)
    }
}

impl sum_tree::ContextLessSummary for DiffTransformSummary {
    fn zero() -> Self {
        DiffTransformSummary {
            input: MBTextSummary::default(),
            output: MBTextSummary::default(),
        }
    }

    fn add_summary(&mut self, other: &Self) {
        self.input += other.input;
        self.output += other.output;
    }
}

impl sum_tree::Dimension<'_, ExcerptSummary> for PathKey {
    fn zero(_: <ExcerptSummary as sum_tree::Summary>::Context<'_>) -> Self {
        PathKey::min()
    }

    fn add_summary(
        &mut self,
        summary: &'_ ExcerptSummary,
        _cx: <ExcerptSummary as sum_tree::Summary>::Context<'_>,
    ) {
        *self = summary.path_key.clone();
    }
}

impl sum_tree::Dimension<'_, ExcerptSummary> for MultiBufferOffset {
    fn zero(_: <ExcerptSummary as sum_tree::Summary>::Context<'_>) -> Self {
        MultiBufferOffset::ZERO
    }

    fn add_summary(
        &mut self,
        summary: &'_ ExcerptSummary,
        _cx: <ExcerptSummary as sum_tree::Summary>::Context<'_>,
    ) {
        *self += summary.text.len
    }
}

impl sum_tree::ContextLessSummary for ExcerptSummary {
    fn zero() -> Self {
        Self::min()
    }

    fn add_summary(&mut self, summary: &Self) {
        debug_assert!(
            summary.path_key >= self.path_key,
            "Path keys must be in ascending order: {:?} > {:?}",
            summary.path_key,
            self.path_key
        );

        self.path_key = summary.path_key.clone();
        self.path_key_index = summary.path_key_index;
        self.max_anchor = summary.max_anchor;
        self.text += summary.text;
        self.widest_line_number = cmp::max(self.widest_line_number, summary.widest_line_number);
        self.count += summary.count;
    }
}

impl sum_tree::SeekTarget<'_, ExcerptSummary, ExcerptSummary> for AnchorSeekTarget<'_> {
    fn cmp(
        &self,
        cursor_location: &ExcerptSummary,
        _cx: <ExcerptSummary as sum_tree::Summary>::Context<'_>,
    ) -> cmp::Ordering {
        match self {
            AnchorSeekTarget::Missing { path_key } => {
                // Want to end up after any excerpts for (a different buffer at) the original path
                match Ord::cmp(*path_key, &cursor_location.path_key) {
                    Ordering::Less => Ordering::Less,
                    Ordering::Equal | Ordering::Greater => Ordering::Greater,
                }
            }
            AnchorSeekTarget::Excerpt {
                path_key,
                path_key_index,
                anchor,
                snapshot,
            } => {
                if Some(*path_key_index) != cursor_location.path_key_index {
                    Ord::cmp(*path_key, &cursor_location.path_key)
                } else if let Some(max_anchor) = cursor_location.max_anchor {
                    debug_assert_eq!(max_anchor.buffer_id, snapshot.remote_id());
                    anchor.cmp(&max_anchor, snapshot)
                } else {
                    Ordering::Greater
                }
            }
            AnchorSeekTarget::Empty => Ordering::Greater,
        }
    }
}

impl sum_tree::ContextLessSummary for PathKey {
    fn zero() -> Self {
        PathKey::min()
    }

    fn add_summary(&mut self, summary: &Self) {
        debug_assert!(
            summary >= self,
            "Path keys must be in ascending order: {:?} > {:?}",
            summary,
            self
        );

        *self = summary.clone();
    }
}

impl sum_tree::SeekTarget<'_, ExcerptSummary, ExcerptSummary> for PathKey {
    fn cmp(
        &self,
        cursor_location: &ExcerptSummary,
        _cx: <ExcerptSummary as sum_tree::Summary>::Context<'_>,
    ) -> cmp::Ordering {
        Ord::cmp(self, &cursor_location.path_key)
    }
}

impl<'a, MBD> sum_tree::Dimension<'a, ExcerptSummary> for ExcerptDimension<MBD>
where
    MBD: MultiBufferDimension + Default,
{
    fn zero(_: ()) -> Self {
        ExcerptDimension(MBD::default())
    }

    fn add_summary(&mut self, summary: &'a ExcerptSummary, _: ()) {
        MultiBufferDimension::add_mb_text_summary(&mut self.0, &summary.text)
    }
}

#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Debug)]
pub(super) struct OutputDimension<T>(pub(super) T);

impl<T: PartialEq> PartialEq<T> for OutputDimension<T> {
    fn eq(&self, other: &T) -> bool {
        self.0 == *other
    }
}

impl<T: PartialOrd> PartialOrd<T> for OutputDimension<T> {
    fn partial_cmp(&self, other: &T) -> Option<cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

impl<R, T, U> ops::Sub<OutputDimension<U>> for OutputDimension<T>
where
    T: ops::Sub<U, Output = R>,
{
    type Output = R;

    fn sub(self, other: OutputDimension<U>) -> Self::Output {
        self.0 - other.0
    }
}

impl<R, T, U> ops::Add<U> for OutputDimension<T>
where
    T: ops::Add<U, Output = R>,
{
    type Output = OutputDimension<R>;

    fn add(self, other: U) -> Self::Output {
        OutputDimension(self.0 + other)
    }
}

impl<T, U> AddAssign<U> for OutputDimension<T>
where
    T: AddAssign<U>,
{
    fn add_assign(&mut self, other: U) {
        self.0 += other;
    }
}

impl<T, U> SubAssign<U> for OutputDimension<T>
where
    T: SubAssign<U>,
{
    fn sub_assign(&mut self, other: U) {
        self.0 -= other;
    }
}

#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Debug, Default)]
pub(super) struct ExcerptDimension<T>(pub(super) T);

impl<T: PartialEq> PartialEq<T> for ExcerptDimension<T> {
    fn eq(&self, other: &T) -> bool {
        self.0 == *other
    }
}

impl<T: PartialOrd> PartialOrd<T> for ExcerptDimension<T> {
    fn partial_cmp(&self, other: &T) -> Option<cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

impl ExcerptOffset {
    pub(super) fn saturating_sub(self, other: ExcerptOffset) -> usize {
        self.0.saturating_sub(other.0)
    }
}

impl<R, T, U> ops::Sub<ExcerptDimension<U>> for ExcerptDimension<T>
where
    T: ops::Sub<U, Output = R>,
{
    type Output = R;

    fn sub(self, other: ExcerptDimension<U>) -> Self::Output {
        self.0 - other.0
    }
}

impl<R, T, U> ops::Add<U> for ExcerptDimension<T>
where
    T: ops::Add<U, Output = R>,
{
    type Output = ExcerptDimension<R>;

    fn add(self, other: U) -> Self::Output {
        ExcerptDimension(self.0 + other)
    }
}

impl<T, U> AddAssign<U> for ExcerptDimension<T>
where
    T: AddAssign<U>,
{
    fn add_assign(&mut self, other: U) {
        self.0 += other;
    }
}

impl<T, U> SubAssign<U> for ExcerptDimension<T>
where
    T: SubAssign<U>,
{
    fn sub_assign(&mut self, other: U) {
        self.0 -= other;
    }
}

impl<'a> sum_tree::Dimension<'a, DiffTransformSummary> for MultiBufferOffset {
    fn zero(_: ()) -> Self {
        MultiBufferOffset::ZERO
    }

    fn add_summary(&mut self, summary: &'a DiffTransformSummary, _: ()) {
        *self += summary.output.len;
    }
}

impl<MBD> sum_tree::SeekTarget<'_, DiffTransformSummary, DiffTransformSummary>
    for ExcerptDimension<MBD>
where
    MBD: MultiBufferDimension + Ord,
{
    fn cmp(&self, cursor_location: &DiffTransformSummary, _: ()) -> cmp::Ordering {
        Ord::cmp(&self.0, &MBD::from_summary(&cursor_location.input))
    }
}

impl<'a, MBD> sum_tree::SeekTarget<'a, DiffTransformSummary, DiffTransforms<MBD>>
    for ExcerptDimension<MBD>
where
    MBD: MultiBufferDimension + Ord,
{
    fn cmp(&self, cursor_location: &DiffTransforms<MBD>, _: ()) -> cmp::Ordering {
        Ord::cmp(&self.0, &cursor_location.excerpt_dimension.0)
    }
}

impl<'a, MBD: MultiBufferDimension> sum_tree::Dimension<'a, DiffTransformSummary>
    for ExcerptDimension<MBD>
{
    fn zero(_: ()) -> Self {
        ExcerptDimension(MBD::default())
    }

    fn add_summary(&mut self, summary: &'a DiffTransformSummary, _: ()) {
        self.0.add_mb_text_summary(&summary.input)
    }
}

impl<'a, MBD> sum_tree::SeekTarget<'a, DiffTransformSummary, DiffTransforms<MBD>>
    for OutputDimension<MBD>
where
    MBD: MultiBufferDimension + Ord,
{
    fn cmp(&self, cursor_location: &DiffTransforms<MBD>, _: ()) -> cmp::Ordering {
        Ord::cmp(&self.0, &cursor_location.output_dimension.0)
    }
}

impl<'a, MBD: MultiBufferDimension> sum_tree::Dimension<'a, DiffTransformSummary>
    for OutputDimension<MBD>
{
    fn zero(_: ()) -> Self {
        OutputDimension(MBD::default())
    }

    fn add_summary(&mut self, summary: &'a DiffTransformSummary, _: ()) {
        self.0.add_mb_text_summary(&summary.output)
    }
}

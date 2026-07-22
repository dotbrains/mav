pub struct InlayMap {
    snapshot: InlaySnapshot,
    inlays: Vec<Inlay>,
}

#[derive(Clone)]
pub struct InlaySnapshot {
    pub buffer: MultiBufferSnapshot,
    transforms: SumTree<Transform>,
    pub version: usize,
}

impl std::ops::Deref for InlaySnapshot {
    type Target = MultiBufferSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

#[derive(Clone, Debug)]
enum Transform {
    Isomorphic(MBTextSummary),
    Inlay(Inlay),
}

impl sum_tree::Item for Transform {
    type Summary = TransformSummary;

    #[ztracing::instrument(skip_all)]
    fn summary(&self, _: ()) -> Self::Summary {
        match self {
            Transform::Isomorphic(summary) => TransformSummary {
                input: *summary,
                output: *summary,
            },
            Transform::Inlay(inlay) => TransformSummary {
                input: MBTextSummary::default(),
                output: MBTextSummary::from(inlay.text().summary()),
            },
        }
    }
}

#[derive(Clone, Debug, Default)]
struct TransformSummary {
    /// Summary of the text before inlays have been applied.
    input: MBTextSummary,
    /// Summary of the text after inlays have been applied.
    output: MBTextSummary,
}

impl TransformSummary {
    fn has_inlays(&self) -> bool {
        self.input.len != self.output.len
    }
}

impl sum_tree::ContextLessSummary for TransformSummary {
    fn zero() -> Self {
        Default::default()
    }

    fn add_summary(&mut self, other: &Self) {
        self.input += other.input;
        self.output += other.output;
    }
}

pub type InlayEdit = Edit<InlayOffset>;

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct InlayOffset(pub MultiBufferOffset);

impl Add for InlayOffset {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for InlayOffset {
    type Output = <MultiBufferOffset as Sub>::Output;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl<T> SubAssign<T> for InlayOffset
where
    MultiBufferOffset: SubAssign<T>,
{
    fn sub_assign(&mut self, rhs: T) {
        self.0 -= rhs;
    }
}

impl<T> Add<T> for InlayOffset
where
    MultiBufferOffset: Add<T, Output = MultiBufferOffset>,
{
    type Output = Self;

    fn add(self, rhs: T) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl AddAssign for InlayOffset {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl<T> AddAssign<T> for InlayOffset
where
    MultiBufferOffset: AddAssign<T>,
{
    fn add_assign(&mut self, rhs: T) {
        self.0 += rhs;
    }
}

impl<'a> sum_tree::Dimension<'a, TransformSummary> for InlayOffset {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a TransformSummary, _: ()) {
        self.0 += summary.output.len;
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct InlayPoint(pub Point);

impl Add for InlayPoint {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for InlayPoint {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl<'a> sum_tree::Dimension<'a, TransformSummary> for InlayPoint {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a TransformSummary, _: ()) {
        self.0 += &summary.output.lines;
    }
}

impl<'a> sum_tree::Dimension<'a, TransformSummary> for MultiBufferOffset {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a TransformSummary, _: ()) {
        *self += summary.input.len;
    }
}

impl<'a> sum_tree::Dimension<'a, TransformSummary> for Point {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a TransformSummary, _: ()) {
        *self += &summary.input.lines;
    }
}

#[derive(Clone)]
pub struct InlayBufferRows<'a> {
    transforms: Cursor<'a, 'static, Transform, Dimensions<InlayPoint, Point>>,
    buffer_rows: MultiBufferRows<'a>,
    inlay_row: u32,
    max_buffer_row: MultiBufferRow,
}

pub struct InlayChunks<'a> {
    transforms: Cursor<'a, 'static, Transform, Dimensions<InlayOffset, MultiBufferOffset>>,
    buffer_chunks: CustomHighlightsChunks<'a>,
    buffer_chunk: Option<Chunk<'a>>,
    inlay_chunks: Option<text::ChunkWithBitmaps<'a>>,
    /// text, char bitmap, tabs bitmap
    inlay_chunk: Option<ChunkBitmaps<'a>>,
    output_offset: InlayOffset,
    max_output_offset: InlayOffset,
    highlight_styles: HighlightStyles,
    highlights: Highlights<'a>,
    snapshot: &'a InlaySnapshot,
}

#[derive(Clone)]
pub struct InlayChunk<'a> {
    pub chunk: Chunk<'a>,
    /// Whether the inlay should be customly rendered.
    pub renderer: Option<ChunkRenderer>,
}

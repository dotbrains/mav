use super::*;

pub type MultiBufferPoint = Point;
/// ExcerptOffset is offset into the non-deleted text of the multibuffer
pub(super) type ExcerptOffset = ExcerptDimension<MultiBufferOffset>;
/// ExcerptOffset is based on the non-deleted text of the multibuffer

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq, Hash, serde::Deserialize)]
#[serde(transparent)]
pub struct MultiBufferRow(pub u32);

impl MultiBufferRow {
    pub const MIN: Self = Self(0);
    pub const MAX: Self = Self(u32::MAX);
}

impl ops::Add<usize> for MultiBufferRow {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        MultiBufferRow(self.0 + rhs as u32)
    }
}

pub trait MultiBufferDimension: 'static + Copy + Default + std::fmt::Debug {
    type TextDimension: TextDimension;
    fn from_summary(summary: &MBTextSummary) -> Self;

    fn add_text_dim(&mut self, summary: &Self::TextDimension);

    fn add_mb_text_summary(&mut self, summary: &MBTextSummary);
}

// todo(lw): MultiBufferPoint
impl MultiBufferDimension for Point {
    type TextDimension = Point;
    fn from_summary(summary: &MBTextSummary) -> Self {
        summary.lines
    }

    fn add_text_dim(&mut self, other: &Self::TextDimension) {
        *self += *other;
    }

    fn add_mb_text_summary(&mut self, summary: &MBTextSummary) {
        *self += summary.lines;
    }
}

// todo(lw): MultiBufferPointUtf16
impl MultiBufferDimension for PointUtf16 {
    type TextDimension = PointUtf16;
    fn from_summary(summary: &MBTextSummary) -> Self {
        summary.lines_utf16()
    }

    fn add_text_dim(&mut self, other: &Self::TextDimension) {
        *self += *other;
    }

    fn add_mb_text_summary(&mut self, summary: &MBTextSummary) {
        *self += summary.lines_utf16();
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq, Hash, serde::Deserialize)]
pub struct MultiBufferOffset(pub usize);

impl fmt::Display for MultiBufferOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl rand::distr::uniform::SampleUniform for MultiBufferOffset {
    type Sampler = MultiBufferOffsetUniformSampler;
}

pub struct MultiBufferOffsetUniformSampler {
    sampler: rand::distr::uniform::UniformUsize,
}

impl rand::distr::uniform::UniformSampler for MultiBufferOffsetUniformSampler {
    type X = MultiBufferOffset;

    fn new<B1, B2>(low_b: B1, high_b: B2) -> Result<Self, rand::distr::uniform::Error>
    where
        B1: rand::distr::uniform::SampleBorrow<Self::X> + Sized,
        B2: rand::distr::uniform::SampleBorrow<Self::X> + Sized,
    {
        let low = *low_b.borrow();
        let high = *high_b.borrow();
        let sampler = rand::distr::uniform::UniformUsize::new(low.0, high.0);
        sampler.map(|sampler| MultiBufferOffsetUniformSampler { sampler })
    }

    #[inline] // if the range is constant, this helps LLVM to do the
    // calculations at compile-time.
    fn new_inclusive<B1, B2>(low_b: B1, high_b: B2) -> Result<Self, rand::distr::uniform::Error>
    where
        B1: rand::distr::uniform::SampleBorrow<Self::X> + Sized,
        B2: rand::distr::uniform::SampleBorrow<Self::X> + Sized,
    {
        let low = *low_b.borrow();
        let high = *high_b.borrow();
        let sampler = rand::distr::uniform::UniformUsize::new_inclusive(low.0, high.0);
        sampler.map(|sampler| MultiBufferOffsetUniformSampler { sampler })
    }

    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> Self::X {
        MultiBufferOffset(self.sampler.sample(rng))
    }
}
impl MultiBufferDimension for MultiBufferOffset {
    type TextDimension = usize;
    fn from_summary(summary: &MBTextSummary) -> Self {
        summary.len
    }

    fn add_text_dim(&mut self, other: &Self::TextDimension) {
        self.0 += *other;
    }

    fn add_mb_text_summary(&mut self, summary: &MBTextSummary) {
        *self += summary.len;
    }
}
impl MultiBufferDimension for MultiBufferOffsetUtf16 {
    type TextDimension = OffsetUtf16;
    fn from_summary(summary: &MBTextSummary) -> Self {
        MultiBufferOffsetUtf16(summary.len_utf16)
    }

    fn add_text_dim(&mut self, other: &Self::TextDimension) {
        self.0 += *other;
    }

    fn add_mb_text_summary(&mut self, summary: &MBTextSummary) {
        self.0 += summary.len_utf16;
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq, Hash, serde::Deserialize)]
pub struct BufferOffset(pub usize);

impl TextDimension for BufferOffset {
    fn from_text_summary(summary: &TextSummary) -> Self {
        BufferOffset(usize::from_text_summary(summary))
    }
    fn from_chunk(chunk: rope::ChunkSlice) -> Self {
        BufferOffset(usize::from_chunk(chunk))
    }
    fn add_assign(&mut self, other: &Self) {
        TextDimension::add_assign(&mut self.0, &other.0);
    }
}
impl<'a> sum_tree::Dimension<'a, rope::ChunkSummary> for BufferOffset {
    fn zero(cx: ()) -> Self {
        BufferOffset(<usize as sum_tree::Dimension<'a, rope::ChunkSummary>>::zero(cx))
    }

    fn add_summary(&mut self, summary: &'a rope::ChunkSummary, cx: ()) {
        usize::add_summary(&mut self.0, summary, cx);
    }
}

impl Sub for BufferOffset {
    type Output = usize;

    fn sub(self, other: BufferOffset) -> Self::Output {
        self.0 - other.0
    }
}

impl AddAssign<DimensionPair<usize, Point>> for BufferOffset {
    fn add_assign(&mut self, other: DimensionPair<usize, Point>) {
        self.0 += other.key;
    }
}

impl language::ToPoint for BufferOffset {
    fn to_point(&self, snapshot: &text::BufferSnapshot) -> Point {
        self.0.to_point(snapshot)
    }
}

impl language::ToPointUtf16 for BufferOffset {
    fn to_point_utf16(&self, snapshot: &text::BufferSnapshot) -> PointUtf16 {
        self.0.to_point_utf16(snapshot)
    }
}

impl language::ToOffset for BufferOffset {
    fn to_offset(&self, snapshot: &text::BufferSnapshot) -> usize {
        self.0.to_offset(snapshot)
    }
}

impl language::ToOffsetUtf16 for BufferOffset {
    fn to_offset_utf16(&self, snapshot: &text::BufferSnapshot) -> OffsetUtf16 {
        self.0.to_offset_utf16(snapshot)
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct MultiBufferOffsetUtf16(pub OffsetUtf16);

impl ops::Add<usize> for MultiBufferOffsetUtf16 {
    type Output = MultiBufferOffsetUtf16;

    fn add(self, rhs: usize) -> Self::Output {
        MultiBufferOffsetUtf16(OffsetUtf16(self.0.0 + rhs))
    }
}

impl ops::Add<OffsetUtf16> for MultiBufferOffsetUtf16 {
    type Output = Self;

    fn add(self, rhs: OffsetUtf16) -> Self::Output {
        MultiBufferOffsetUtf16(self.0 + rhs)
    }
}

impl AddAssign<OffsetUtf16> for MultiBufferOffsetUtf16 {
    fn add_assign(&mut self, rhs: OffsetUtf16) {
        self.0 += rhs;
    }
}

impl AddAssign<usize> for MultiBufferOffsetUtf16 {
    fn add_assign(&mut self, rhs: usize) {
        self.0.0 += rhs;
    }
}

impl Sub for MultiBufferOffsetUtf16 {
    type Output = OffsetUtf16;

    fn sub(self, other: MultiBufferOffsetUtf16) -> Self::Output {
        self.0 - other.0
    }
}

impl Sub<OffsetUtf16> for MultiBufferOffsetUtf16 {
    type Output = MultiBufferOffsetUtf16;

    fn sub(self, other: OffsetUtf16) -> Self::Output {
        MultiBufferOffsetUtf16(self.0 - other)
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct BufferOffsetUtf16(pub OffsetUtf16);

impl MultiBufferOffset {
    pub(super) const ZERO: Self = Self(0);
    pub fn saturating_sub(self, other: MultiBufferOffset) -> usize {
        self.0.saturating_sub(other.0)
    }
    pub fn saturating_sub_usize(self, other: usize) -> MultiBufferOffset {
        MultiBufferOffset(self.0.saturating_sub(other))
    }
}

impl ops::Sub for MultiBufferOffset {
    type Output = usize;

    fn sub(self, other: MultiBufferOffset) -> Self::Output {
        self.0 - other.0
    }
}

impl ops::Sub<usize> for MultiBufferOffset {
    type Output = Self;

    fn sub(self, other: usize) -> Self::Output {
        MultiBufferOffset(self.0 - other)
    }
}

impl ops::SubAssign<usize> for MultiBufferOffset {
    fn sub_assign(&mut self, other: usize) {
        self.0 -= other;
    }
}

impl ops::Add<usize> for BufferOffset {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        BufferOffset(self.0 + rhs)
    }
}

impl ops::AddAssign<usize> for BufferOffset {
    fn add_assign(&mut self, other: usize) {
        self.0 += other;
    }
}

impl ops::Add<usize> for MultiBufferOffset {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        MultiBufferOffset(self.0 + rhs)
    }
}

impl ops::AddAssign<usize> for MultiBufferOffset {
    fn add_assign(&mut self, other: usize) {
        self.0 += other;
    }
}

impl ops::Add<isize> for MultiBufferOffset {
    type Output = Self;

    fn add(self, rhs: isize) -> Self::Output {
        MultiBufferOffset((self.0 as isize + rhs) as usize)
    }
}

impl ops::Add for MultiBufferOffset {
    type Output = Self;

    fn add(self, rhs: MultiBufferOffset) -> Self::Output {
        MultiBufferOffset(self.0 + rhs.0)
    }
}

impl ops::AddAssign<MultiBufferOffset> for MultiBufferOffset {
    fn add_assign(&mut self, other: MultiBufferOffset) {
        self.0 += other.0;
    }
}

pub trait ToOffset: 'static + fmt::Debug {
    fn to_offset(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset;
    fn to_offset_utf16(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16;
}

pub trait ToPoint: 'static + fmt::Debug {
    fn to_point(&self, snapshot: &MultiBufferSnapshot) -> Point;
    fn to_point_utf16(&self, snapshot: &MultiBufferSnapshot) -> PointUtf16;
}

impl ToOffset for Point {
    fn to_offset<'a>(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset {
        snapshot.point_to_offset(*self)
    }
    fn to_offset_utf16(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16 {
        snapshot.point_to_offset_utf16(*self)
    }
}

impl ToOffset for MultiBufferOffset {
    #[track_caller]
    fn to_offset<'a>(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset {
        assert!(
            *self <= snapshot.len(),
            "offset {} is greater than the snapshot.len() {}",
            self.0,
            snapshot.len().0,
        );
        *self
    }
    fn to_offset_utf16(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16 {
        snapshot.offset_to_offset_utf16(*self)
    }
}

impl ToOffset for MultiBufferOffsetUtf16 {
    fn to_offset<'a>(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset {
        snapshot.offset_utf16_to_offset(*self)
    }

    fn to_offset_utf16(&self, _snapshot: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16 {
        *self
    }
}

impl ToOffset for PointUtf16 {
    fn to_offset<'a>(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset {
        snapshot.point_utf16_to_offset(*self)
    }
    fn to_offset_utf16(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16 {
        snapshot.point_utf16_to_offset_utf16(*self)
    }
}

impl ToPoint for MultiBufferOffset {
    fn to_point<'a>(&self, snapshot: &MultiBufferSnapshot) -> Point {
        snapshot.offset_to_point(*self)
    }
    fn to_point_utf16<'a>(&self, snapshot: &MultiBufferSnapshot) -> PointUtf16 {
        snapshot.offset_to_point_utf16(*self)
    }
}

impl ToPoint for Point {
    fn to_point<'a>(&self, _: &MultiBufferSnapshot) -> Point {
        *self
    }
    fn to_point_utf16<'a>(&self, snapshot: &MultiBufferSnapshot) -> PointUtf16 {
        snapshot.point_to_point_utf16(*self)
    }
}

impl ToPoint for PointUtf16 {
    fn to_point<'a>(&self, snapshot: &MultiBufferSnapshot) -> Point {
        snapshot.point_utf16_to_point(*self)
    }
    fn to_point_utf16<'a>(&self, _: &MultiBufferSnapshot) -> PointUtf16 {
        *self
    }
}

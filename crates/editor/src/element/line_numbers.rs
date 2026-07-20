use super::*;

#[derive(Debug)]
pub struct LineNumberSegment {
    pub(super) shaped_line: ShapedLine,
    pub(super) hitbox: Option<Hitbox>,
}

#[derive(Debug)]
pub struct LineNumberLayout {
    pub(super) segments: SmallVec<[LineNumberSegment; 1]>,
}

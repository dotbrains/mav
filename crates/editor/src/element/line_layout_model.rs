use super::*;

#[derive(Debug)]
pub(crate) struct LineWithInvisibles {
    pub(super) fragments: SmallVec<[LineFragment; 1]>,
    pub(super) invisibles: Vec<Invisible>,
    pub(super) len: usize,
    pub(crate) width: Pixels,
    pub(super) font_size: Pixels,
}

pub(crate) enum LineFragment {
    Text(ShapedLine),
    Element {
        id: ChunkRendererId,
        element: Option<AnyElement>,
        size: Size<Pixels>,
        len: usize,
    },
}

impl fmt::Debug for LineFragment {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LineFragment::Text(shaped_line) => f.debug_tuple("Text").field(shaped_line).finish(),
            LineFragment::Element { size, len, .. } => f
                .debug_struct("Element")
                .field("size", size)
                .field("len", len)
                .finish(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Invisible {
    /// A tab character
    ///
    /// A tab character is internally represented by spaces (configured by the user's tab width)
    /// aligned to the nearest column, so it's necessary to store the start and end offset for
    /// adjacency checks.
    Tab {
        line_start_offset: usize,
        line_end_offset: usize,
    },
    /// A whitespace character (ASCII space or any other Unicode whitespace).
    ///
    /// Storing both offsets correctly accounts for multi-byte whitespace characters
    /// such as U+00A0 NO-BREAK SPACE, keeping adjacency checks correct.
    Whitespace {
        line_start_offset: usize,
        line_end_offset: usize,
    },
}

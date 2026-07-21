use super::*;

#[derive(Debug)]
pub(super) struct BufferEdit {
    pub(super) range: Range<BufferOffset>,
    pub(super) new_text: Arc<str>,
    pub(super) is_insertion: bool,
    pub(super) original_indent_column: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum DiffChangeKind {
    BufferEdited,
    DiffUpdated { base_changed: bool },
    ExpandOrCollapseHunks { expand: bool },
}

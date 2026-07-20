use super::*;

#[derive(Clone, Copy)]
pub struct RowHighlightOptions {
    pub autoscroll: bool,
    pub include_gutter: bool,
}

impl Default for RowHighlightOptions {
    fn default() -> Self {
        Self {
            autoscroll: Default::default(),
            include_gutter: true,
        }
    }
}

pub(crate) struct RowHighlight {
    pub(crate) index: usize,
    pub(crate) range: Range<Anchor>,
    pub(crate) color: fn(&App) -> Hsla,
    pub(crate) options: RowHighlightOptions,
    pub(crate) type_id: TypeId,
}

#[derive(Clone, Debug)]
pub(crate) struct AddSelectionsState {
    pub(crate) groups: Vec<AddSelectionsGroup>,
}

#[derive(Clone, Debug)]
pub(crate) struct AddSelectionsGroup {
    pub(crate) above: bool,
    pub(crate) stack: Vec<usize>,
}

#[derive(Clone)]
pub(crate) struct SelectNextState {
    pub(crate) query: AhoCorasick,
    pub(crate) wordwise: bool,
    pub(crate) done: bool,
}

impl std::fmt::Debug for SelectNextState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(std::any::type_name::<Self>())
            .field("wordwise", &self.wordwise)
            .field("done", &self.done)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct AutocloseRegion {
    pub(crate) selection_id: usize,
    pub(crate) range: Range<Anchor>,
    pub(crate) pair: BracketPair,
}

#[derive(Debug)]
pub(crate) struct SnippetState {
    pub(crate) ranges: Vec<Vec<Range<Anchor>>>,
    pub(crate) active_index: usize,
    pub(crate) choices: Vec<Option<Vec<String>>>,
}

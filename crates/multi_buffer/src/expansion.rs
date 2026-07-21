use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExpandExcerptDirection {
    Up,
    Down,
    UpAndDown,
}

impl ExpandExcerptDirection {
    pub fn should_expand_up(&self) -> bool {
        match self {
            ExpandExcerptDirection::Up => true,
            ExpandExcerptDirection::Down => false,
            ExpandExcerptDirection::UpAndDown => true,
        }
    }

    pub fn should_expand_down(&self) -> bool {
        match self {
            ExpandExcerptDirection::Up => false,
            ExpandExcerptDirection::Down => true,
            ExpandExcerptDirection::UpAndDown => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IndentGuide {
    pub buffer_id: BufferId,
    pub start_row: MultiBufferRow,
    pub end_row: MultiBufferRow,
    pub depth: u32,
    pub tab_size: u32,
    pub settings: IndentGuideSettings,
}

impl IndentGuide {
    pub fn indent_level(&self) -> u32 {
        self.depth * self.tab_size
    }
}

use super::*;

#[derive(Clone, Debug)]
pub(super) struct CodeLensLine {
    pub(super) position: Anchor,
    pub(super) indent_column: u32,
    pub(super) items: Vec<CodeLensItem>,
}

#[derive(Clone, Debug)]
pub(super) struct CodeLensItem {
    pub(super) title: Option<SharedString>,
    pub(super) action: CodeAction,
}

pub(super) struct CodeLensBlock {
    pub(super) block_id: CustomBlockId,
    pub(super) anchor: Anchor,
    pub(super) line: CodeLensLine,
}

pub(crate) struct CodeLensState {
    pub(super) blocks: HashMap<BufferId, Vec<CodeLensBlock>>,
    pub(super) actions: HashMap<BufferId, CodeLensActions>,
    pub(super) resolve_task: Task<()>,
}

impl Default for CodeLensState {
    fn default() -> Self {
        Self {
            blocks: HashMap::default(),
            actions: HashMap::default(),
            resolve_task: Task::ready(()),
        }
    }
}

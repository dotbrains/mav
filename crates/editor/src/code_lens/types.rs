use super::*;

#[derive(Clone, Debug)]
pub(crate) struct CodeLensLine {
    pub(crate) position: Anchor,
    pub(crate) indent_column: u32,
    pub(crate) items: Vec<CodeLensItem>,
}

#[derive(Clone, Debug)]
pub(crate) struct CodeLensItem {
    pub(crate) title: Option<SharedString>,
    pub(crate) action: CodeAction,
}

pub(crate) struct CodeLensBlock {
    pub(crate) block_id: CustomBlockId,
    pub(crate) anchor: Anchor,
    pub(crate) line: CodeLensLine,
}

pub(crate) struct CodeLensState {
    pub(crate) blocks: HashMap<BufferId, Vec<CodeLensBlock>>,
    pub(crate) actions: HashMap<BufferId, CodeLensActions>,
    pub(crate) resolve_task: Task<()>,
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

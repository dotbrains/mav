use super::*;

pub(super) struct UnopenedWorktree {
    pub(super) path: String,
    pub(super) main_workspace_path: String,
}

pub(super) struct TestState {
    pub(super) fs: Arc<FakeFs>,
    pub(super) thread_counter: u32,
    pub(super) workspace_counter: u32,
    pub(super) worktree_counter: u32,
    pub(super) saved_thread_ids: Vec<acp::SessionId>,
    pub(super) unopened_worktrees: Vec<UnopenedWorktree>,
}

impl TestState {
    pub(super) fn new(fs: Arc<FakeFs>) -> Self {
        Self {
            fs,
            thread_counter: 0,
            workspace_counter: 1,
            worktree_counter: 0,
            saved_thread_ids: Vec::new(),
            unopened_worktrees: Vec::new(),
        }
    }

    pub(super) fn next_metadata_only_thread_id(&mut self) -> acp::SessionId {
        let id = self.thread_counter;
        self.thread_counter += 1;
        acp::SessionId::new(Arc::from(format!("prop-thread-{id}")))
    }

    pub(super) fn next_workspace_path(&mut self) -> String {
        let id = self.workspace_counter;
        self.workspace_counter += 1;
        format!("/prop-project-{id}")
    }

    pub(super) fn next_worktree_name(&mut self) -> String {
        let id = self.worktree_counter;
        self.worktree_counter += 1;
        format!("wt-{id}")
    }
}

#[derive(Debug)]
pub(super) enum Operation {
    SaveThread { project_group_index: usize },
    SaveWorktreeThread { worktree_index: usize },
    ToggleAgentPanel,
    CreateDraftThread,
    AddProject { use_worktree: bool },
    ArchiveThread { index: usize },
    SwitchToThread { index: usize },
    SwitchToProjectGroup { index: usize },
    AddLinkedWorktree { project_group_index: usize },
    AddWorktreeToProject { project_group_index: usize },
    RemoveWorktreeFromProject { project_group_index: usize },
}

// Distribution (out of 24 slots):
//   SaveThread:                5 slots (~21%)
//   SaveWorktreeThread:        2 slots (~8%)
//   ToggleAgentPanel:          1 slot  (~4%)
//   CreateDraftThread:         1 slot  (~4%)
//   AddProject:                1 slot  (~4%)
//   ArchiveThread:             2 slots (~8%)
//   SwitchToThread:            2 slots (~8%)
//   SwitchToProjectGroup:      2 slots (~8%)
//   AddLinkedWorktree:         4 slots (~17%)
//   AddWorktreeToProject:      2 slots (~8%)
//   RemoveWorktreeFromProject: 2 slots (~8%)
pub(super) const DISTRIBUTION_SLOTS: u32 = 24;

impl TestState {
    pub(super) fn generate_operation(&self, raw: u32, project_group_count: usize) -> Operation {
        let extra = (raw / DISTRIBUTION_SLOTS) as usize;

        match raw % DISTRIBUTION_SLOTS {
            0..=4 => Operation::SaveThread {
                project_group_index: extra % project_group_count,
            },
            5..=6 if !self.unopened_worktrees.is_empty() => Operation::SaveWorktreeThread {
                worktree_index: extra % self.unopened_worktrees.len(),
            },
            5..=6 => Operation::SaveThread {
                project_group_index: extra % project_group_count,
            },
            7 => Operation::ToggleAgentPanel,
            8 => Operation::CreateDraftThread,
            9 => Operation::AddProject {
                use_worktree: !self.unopened_worktrees.is_empty(),
            },
            10..=11 if !self.saved_thread_ids.is_empty() => Operation::ArchiveThread {
                index: extra % self.saved_thread_ids.len(),
            },
            10..=11 => Operation::AddProject {
                use_worktree: !self.unopened_worktrees.is_empty(),
            },
            12..=13 if !self.saved_thread_ids.is_empty() => Operation::SwitchToThread {
                index: extra % self.saved_thread_ids.len(),
            },
            12..=13 => Operation::SwitchToProjectGroup {
                index: extra % project_group_count,
            },
            14..=15 => Operation::SwitchToProjectGroup {
                index: extra % project_group_count,
            },
            16..=19 if project_group_count > 0 => Operation::AddLinkedWorktree {
                project_group_index: extra % project_group_count,
            },
            16..=19 => Operation::SaveThread {
                project_group_index: extra % project_group_count,
            },
            20..=21 if project_group_count > 0 => Operation::AddWorktreeToProject {
                project_group_index: extra % project_group_count,
            },
            20..=21 => Operation::SaveThread {
                project_group_index: extra % project_group_count,
            },
            22..=23 if project_group_count > 0 => Operation::RemoveWorktreeFromProject {
                project_group_index: extra % project_group_count,
            },
            22..=23 => Operation::SaveThread {
                project_group_index: extra % project_group_count,
            },
            _ => unreachable!(),
        }
    }
}

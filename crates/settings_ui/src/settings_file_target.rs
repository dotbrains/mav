use mav_actions::OpenSettingsAtTarget;
use project::WorktreeId;

#[derive(Clone, Copy)]
pub(super) enum SettingsFileTarget {
    User,
    Project(WorktreeId),
}

impl From<&OpenSettingsAtTarget> for SettingsFileTarget {
    fn from(target: &OpenSettingsAtTarget) -> Self {
        match target {
            OpenSettingsAtTarget::User => Self::User,
            OpenSettingsAtTarget::Project { worktree_id } => {
                Self::Project(WorktreeId::from_usize(*worktree_id))
            }
        }
    }
}

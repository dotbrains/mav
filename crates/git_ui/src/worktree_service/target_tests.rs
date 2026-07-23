#[cfg(test)]
mod target_tests {
    use super::super::*;
    use super::test_support::*;
    use fs::Fs;
    use gpui::TestAppContext;
    use mav_actions::NewWorktreeBranchTarget;
    use project::{FakeFs, Project};
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use util::path;

    #[test]
    fn test_remote_branch_name_parse() {
        assert_eq!(
            RemoteBranchName::parse("refs/remotes/origin/main"),
            Some(RemoteBranchName {
                remote_name: "origin".to_string(),
                branch_name: "main".to_string(),
            })
        );
        assert_eq!(
            RemoteBranchName::parse("upstream/feature/foo"),
            Some(RemoteBranchName {
                remote_name: "upstream".to_string(),
                branch_name: "feature/foo".to_string(),
            })
        );
        assert_eq!(RemoteBranchName::parse("main"), None);
        assert_eq!(RemoteBranchName::parse("origin/"), None);
    }

    #[test]
    fn test_worktree_create_targets() {
        let origin_main = RemoteBranchName {
            remote_name: "origin".to_string(),
            branch_name: "main".to_string(),
        };

        // Multiple repositories: only the current branch, regardless of default.
        assert_eq!(
            worktree_create_targets(true, Some(origin_main.clone()), Some("feature")),
            vec![WorktreeCreateTarget::CurrentBranch]
        );

        // Default branch differs from current: offer both, default first.
        assert_eq!(
            worktree_create_targets(false, Some(origin_main.clone()), Some("feature")),
            vec![
                WorktreeCreateTarget::DefaultBranch(origin_main.clone()),
                WorktreeCreateTarget::CurrentBranch,
            ]
        );

        // Current branch matches the default: only the default branch entry.
        assert_eq!(
            worktree_create_targets(false, Some(origin_main.clone()), Some("main")),
            vec![WorktreeCreateTarget::DefaultBranch(origin_main)]
        );

        // No default branch resolved: fall back to the current branch.
        assert_eq!(
            worktree_create_targets(false, None, Some("feature")),
            vec![WorktreeCreateTarget::CurrentBranch]
        );
    }

    #[test]
    fn test_worktree_create_target_branch_label() {
        let origin_main = RemoteBranchName {
            remote_name: "origin".to_string(),
            branch_name: "main".to_string(),
        };
        assert_eq!(
            WorktreeCreateTarget::DefaultBranch(origin_main).branch_label(false, Some("feature")),
            "origin/main"
        );
        assert_eq!(
            WorktreeCreateTarget::CurrentBranch.branch_label(false, Some("feature")),
            "feature"
        );
        // Detached HEAD falls back to "HEAD".
        assert_eq!(
            WorktreeCreateTarget::CurrentBranch.branch_label(false, None),
            "HEAD"
        );
        // Multiple repositories pluralize the current branch.
        assert_eq!(
            WorktreeCreateTarget::CurrentBranch.branch_label(true, Some("feature")),
            "current branches"
        );
    }
}

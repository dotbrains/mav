#[cfg(test)]
mod trust_tests {
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

    #[gpui::test]
    async fn test_linked_worktree_inherits_trust_from_main_worktree(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            project::trusted_worktrees::init(collections::HashMap::default(), cx);
        });

        let fs = FakeFs::new(cx.background_executor.clone());
        cx.update(|cx| <dyn Fs>::set_global(fs.clone(), cx));
        fs.insert_tree(
            "/root",
            json!({
                "project": {
                    ".git": {},
                    "src": {
                        "main.rs": "fn main() {}",
                    },
                },
            }),
        )
        .await;

        let main_project_root = PathBuf::from(path!("/root/project"));
        let project =
            Project::test_with_worktree_trust(fs.clone(), [main_project_root.as_path()], cx).await;
        project
            .update(cx, |project, cx| project.git_scans_complete(cx))
            .await;

        // The main worktree starts restricted; trust it explicitly
        let worktree_store = project.read_with(cx, |project, _| project.worktree_store());
        let main_worktree_id = worktree_store.read_with(cx, |store, cx| {
            store
                .worktrees()
                .next()
                .map(|wt| wt.read(cx).id())
                .expect("should have a worktree")
        });
        let trusted_store = cx
            .read(|cx| project::trusted_worktrees::TrustedWorktrees::try_get_global(cx))
            .expect("trust store should exist");
        trusted_store.update(cx, |store, cx| {
            store.trust(
                &worktree_store,
                collections::HashSet::from_iter([project::trusted_worktrees::PathTrust::Worktree(
                    main_worktree_id,
                )]),
                cx,
            );
        });

        // Verify main worktree is now trusted
        let has_restricted = cx.read(|cx| {
            project::trusted_worktrees::TrustedWorktrees::has_restricted_worktrees(
                &worktree_store,
                cx,
            )
        });
        assert!(
            !has_restricted,
            "main worktree should be trusted after explicit trust"
        );

        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        multi_workspace.update(cx, |multi_workspace, cx| {
            multi_workspace.retain_active_workspace(cx);
        });

        // Create a linked worktree from the trusted main worktree
        let main_workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
        main_workspace.update_in(cx, |workspace, window, cx| {
            handle_create_worktree(
                workspace,
                &mav_actions::CreateWorktree {
                    worktree_name: Some("feature".to_string()),
                    branch_target: NewWorktreeBranchTarget::CurrentBranch,
                },
                window,
                None,
                cx,
            );
        });
        cx.run_until_parked();

        // The new workspace (linked worktree) should inherit trust
        let new_workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
        let new_worktree_store =
            new_workspace.read_with(cx, |ws, cx| ws.project().read(cx).worktree_store());
        let new_has_restricted = cx.read(|cx| {
            project::trusted_worktrees::TrustedWorktrees::has_restricted_worktrees(
                &new_worktree_store,
                cx,
            )
        });
        assert!(
            !new_has_restricted,
            "linked worktree should inherit trust from the main worktree"
        );

        // The security modal should not be showing
        let has_modal = new_workspace.read_with(cx, |ws, cx| {
            ws.active_modal::<workspace::security_modal::SecurityModal>(cx)
                .is_some()
        });
        assert!(
            !has_modal,
            "security modal should not show for a linked worktree created from a trusted main worktree"
        );
    }
}

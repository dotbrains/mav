use super::*;

#[gpui::test]
async fn test_discard_mixed_workspace_draft_closes_only_archived_worktree_items(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/main-repo",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-b": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-b",
                    },
                },
            },
            "src": {
                "lib.rs": "pub fn hello() {}",
            },
        }),
    )
    .await;

    fs.insert_tree(
        "/worktrees/main-repo/feature-b/main-repo",
        serde_json::json!({
            ".git": "gitdir: /main-repo/.git/worktrees/feature-b",
            "src": {
                "main.rs": "fn main() { hello(); }",
            },
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/main-repo/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/worktrees/main-repo/feature-b/main-repo"),
            ref_name: Some("refs/heads/feature-b".into()),
            sha: "def".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    agent_ui::test_support::record_mav_created_worktree(
        fs.as_ref(),
        Path::new("/worktrees/main-repo/feature-b/main-repo"),
        None,
        cx,
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let mixed_project = project::Project::test(
        fs.clone(),
        [
            "/main-repo".as_ref(),
            "/worktrees/main-repo/feature-b/main-repo".as_ref(),
        ],
        cx,
    )
    .await;

    mixed_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) = cx
        .add_window_view(|window, cx| MultiWorkspace::test_new(mixed_project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);
    let workspace =
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone());

    let worktree_ids: Vec<(WorktreeId, Arc<Path>)> = workspace.read_with(cx, |workspace, cx| {
        workspace
            .project()
            .read(cx)
            .visible_worktrees(cx)
            .map(|worktree| (worktree.read(cx).id(), worktree.read(cx).abs_path()))
            .collect()
    });

    let main_repo_worktree_id = worktree_ids
        .iter()
        .find(|(_, path)| path.as_ref() == Path::new("/main-repo"))
        .map(|(id, _)| *id)
        .expect("should find main-repo worktree");

    let feature_b_worktree_id = worktree_ids
        .iter()
        .find(|(_, path)| path.as_ref() == Path::new("/worktrees/main-repo/feature-b/main-repo"))
        .map(|(id, _)| *id)
        .expect("should find feature-b worktree");

    let main_repo_path = project::ProjectPath {
        worktree_id: main_repo_worktree_id,
        path: Arc::from(rel_path("src/lib.rs")),
    };
    let feature_b_path = project::ProjectPath {
        worktree_id: feature_b_worktree_id,
        path: Arc::from(rel_path("src/main.rs")),
    };

    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(main_repo_path.clone(), None, true, window, cx)
        })
        .await
        .expect("should open main-repo file");
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(feature_b_path.clone(), None, true, window, cx)
        })
        .await
        .expect("should open feature-b file");

    let folder_paths = PathList::new(&[
        PathBuf::from("/main-repo"),
        PathBuf::from("/worktrees/main-repo/feature-b/main-repo"),
    ]);
    let main_worktree_paths =
        PathList::new(&[PathBuf::from("/main-repo"), PathBuf::from("/main-repo")]);
    let draft_id = save_draft_metadata_with_main_paths(
        Some("Mixed Workspace Draft".into()),
        folder_paths,
        main_worktree_paths,
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        cx,
    );
    cx.update(|_, cx| {
        agent_ui::draft_prompt_store::write(
            draft_id,
            &[acp::ContentBlock::Text(acp::TextContent::new(
                "mixed workspace draft",
            ))],
            cx,
        )
    })
    .await
    .expect("draft prompt should persist");

    sidebar.update(cx, |sidebar, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    let draft_index = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .position(|entry| {
                matches!(
                    entry,
                    ListEntry::Thread(thread) if thread.metadata.thread_id == draft_id
                )
            })
            .expect("mixed workspace draft should be visible")
    });

    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(draft_index);
    });
    cx.dispatch_action(ArchiveSelectedThread);
    for _ in 0..8 {
        cx.run_until_parked();
    }

    assert_eq!(
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace
            .workspaces()
            .count()),
        1,
        "mixed workspace should be preserved"
    );

    let open_paths_after: Vec<project::ProjectPath> = workspace.read_with(cx, |workspace, cx| {
        workspace
            .panes()
            .iter()
            .flat_map(|pane| {
                pane.read(cx)
                    .items()
                    .filter_map(|item| item.project_path(cx))
            })
            .collect()
    });
    assert!(
        open_paths_after
            .iter()
            .any(|project_path| project_path.worktree_id == main_repo_worktree_id),
        "main-repo file should still be open"
    );
    assert!(
        !open_paths_after
            .iter()
            .any(|project_path| project_path.worktree_id == feature_b_worktree_id),
        "feature-b file should have been closed"
    );

    let draft_metadata_deleted = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(draft_id)
            .is_none()
    });
    assert!(
        draft_metadata_deleted,
        "discarded draft metadata should be deleted"
    );
}

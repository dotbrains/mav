use super::*;

#[gpui::test]
async fn test_terminal_close_event_on_archived_linked_worktree_removes_workspace(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a",
                    },
                },
            },
            "src": {},
        }),
    )
    .await;
    fs.insert_tree(
        "/worktrees/project/feature-a/project",
        serde_json::json!({
            ".git": "gitdir: /project/.git/worktrees/feature-a",
            "src": {},
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/worktrees/project/feature-a/project"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    agent_ui::test_support::record_mav_created_worktree(
        fs.as_ref(),
        Path::new("/worktrees/project/feature-a/project"),
        None,
        cx,
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project = project::Project::test(
        fs.clone(),
        ["/worktrees/project/feature-a/project".as_ref()],
        cx,
    )
    .await;

    main_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);
    let worktree_workspace = multi_workspace.update_in(cx, |multi_workspace, window, cx| {
        multi_workspace.test_add_workspace(worktree_project.clone(), window, cx)
    });
    let worktree_panel = add_agent_panel(&worktree_workspace, cx);
    let worktree_folder_paths =
        PathList::new(&[PathBuf::from("/worktrees/project/feature-a/project")]);

    let archived_session_id = acp::SessionId::new(Arc::from("archived-wt-thread"));
    save_thread_metadata(
        archived_session_id.clone(),
        Some("Archived Worktree Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &worktree_project,
        cx,
    );
    let archived_thread_id = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&archived_session_id)
            .expect("archived thread metadata should exist")
            .thread_id
    });
    cx.update(|_, cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.archive(archived_thread_id, None, cx);
        });
    });
    save_thread_metadata(
        acp::SessionId::new(Arc::from("main-thread")),
        Some("Main Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &main_project,
        cx,
    );
    let empty_draft_id = save_draft_metadata_with_main_paths(
        None,
        worktree_folder_paths.clone(),
        PathList::new(&[PathBuf::from("/project")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 3, 0, 0, 0).unwrap(),
        cx,
    );
    cx.update(|_, cx| {
        assert!(
            agent_ui::draft_prompt_store::read(empty_draft_id, cx).is_none(),
            "empty draft should not have persisted prompt content"
        );
    });

    let terminal_id = worktree_panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace
            .workspaces()
            .count()),
        2,
        "should start with main and linked worktree workspaces"
    );
    let entries_before = visible_entries_as_strings(&sidebar, cx);
    assert!(
        entries_before
            .iter()
            .any(|entry| entry.contains("Dev Server") && entry.contains('{')),
        "expected linked worktree terminal before closing, got: {entries_before:?}"
    );

    worktree_panel.update(cx, |panel, cx| {
        panel.emit_test_terminal_close(terminal_id, cx);
    });
    for _ in 0..4 {
        cx.run_until_parked();
    }

    let terminal_metadata_deleted = cx.update(|_, cx| {
        TerminalThreadMetadataStore::global(cx)
            .read(cx)
            .entry(terminal_id)
            .is_none()
    });
    assert!(
        terminal_metadata_deleted,
        "terminal metadata should be deleted after close"
    );
    let empty_draft_metadata_deleted = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(empty_draft_id)
            .is_none()
    });
    assert!(
        empty_draft_metadata_deleted,
        "empty draft metadata should be deleted before archiving the linked worktree"
    );
    let unarchived_worktree_threads = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries_for_path(&worktree_folder_paths, None)
            .count()
    });
    assert_eq!(
        unarchived_worktree_threads, 0,
        "closing the terminal must not create a fallback draft for the removed worktree"
    );
    assert_eq!(
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace
            .workspaces()
            .count()),
        1,
        "linked worktree workspace should be removed after closing its last terminal"
    );
    let entries_after = visible_entries_as_strings(&sidebar, cx);
    assert!(
        !entries_after.iter().any(|entry| entry.contains('{')),
        "no sidebar entry should reference the archived worktree, got: {entries_after:?}"
    );
    assert!(
        !fs.is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should be removed from disk after closing its last terminal"
    );
}

#[gpui::test]
async fn test_terminal_close_event_deletes_empty_draft_when_linked_worktree_has_no_archive_root(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    fs.set_branch_name(Path::new("/project/.git"), Some("main"));
    fs.insert_branches(Path::new("/project/.git"), &["main", "feature-a"]);
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/external-worktree"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project =
        project::Project::test(fs.clone(), ["/external-worktree".as_ref()], cx).await;

    main_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    let _sidebar = setup_sidebar(&multi_workspace, cx);
    let worktree_workspace = multi_workspace.update_in(cx, |multi_workspace, window, cx| {
        multi_workspace.test_add_workspace(worktree_project.clone(), window, cx)
    });
    let worktree_panel = add_agent_panel(&worktree_workspace, cx);

    save_thread_metadata(
        acp::SessionId::new(Arc::from("main-thread")),
        Some("Main Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &main_project,
        cx,
    );

    let worktree_folder_paths = PathList::new(&[PathBuf::from("/external-worktree")]);
    let empty_draft_id = save_draft_metadata_with_main_paths(
        None,
        worktree_folder_paths.clone(),
        PathList::new(&[PathBuf::from("/project")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 3, 0, 0, 0).unwrap(),
        cx,
    );

    let terminal_id = worktree_panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    worktree_panel.update(cx, |panel, cx| {
        panel.emit_test_terminal_close(terminal_id, cx);
    });
    for _ in 0..4 {
        cx.run_until_parked();
    }

    let empty_draft_metadata_deleted = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(empty_draft_id)
            .is_none()
    });
    assert!(
        empty_draft_metadata_deleted,
        "empty draft metadata should be deleted when removing the linked worktree workspace"
    );
    assert!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace_for_paths(&worktree_folder_paths, None, cx)
            })
            .is_none(),
        "linked worktree workspace should be removed after closing its last terminal"
    );
    assert!(
        fs.is_dir(Path::new("/external-worktree")).await,
        "external linked worktree directory should remain on disk when no archive root is produced"
    );
}

#[gpui::test]
async fn test_terminal_close_event_keeps_linked_worktree_workspace_with_live_editor_draft(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a",
                    },
                },
            },
            "src": {},
        }),
    )
    .await;
    fs.insert_tree(
        "/worktrees/project/feature-a/project",
        serde_json::json!({
            ".git": "gitdir: /project/.git/worktrees/feature-a",
            "src": {},
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/worktrees/project/feature-a/project"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project = project::Project::test(
        fs.clone(),
        ["/worktrees/project/feature-a/project".as_ref()],
        cx,
    )
    .await;

    main_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    let _sidebar = setup_sidebar(&multi_workspace, cx);
    let worktree_workspace = multi_workspace.update_in(cx, |multi_workspace, window, cx| {
        multi_workspace.test_add_workspace(worktree_project.clone(), window, cx)
    });
    let worktree_panel = add_agent_panel(&worktree_workspace, cx);

    save_thread_metadata(
        acp::SessionId::new(Arc::from("main-thread")),
        Some("Main Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &main_project,
        cx,
    );

    let worktree_folder_paths =
        PathList::new(&[PathBuf::from("/worktrees/project/feature-a/project")]);
    let draft_id = save_draft_metadata_with_main_paths(
        Some("Worktree Draft".into()),
        worktree_folder_paths.clone(),
        PathList::new(&[PathBuf::from("/project")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 3, 0, 0, 0).unwrap(),
        cx,
    );

    worktree_panel.update_in(cx, |panel, window, cx| {
        panel.load_agent_thread(
            Agent::Stub,
            draft_id,
            Some(worktree_folder_paths.clone()),
            None,
            false,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
    });
    cx.run_until_parked();
    let editor_text =
        worktree_panel.read_with(cx, |panel, cx| panel.editor_text_if_in_memory(draft_id, cx));
    assert_eq!(
        editor_text,
        Some(None),
        "draft should be in memory with empty editor text before editing"
    );

    let message_editor = worktree_panel.read_with(cx, |panel, cx| {
        panel
            .active_thread_view(cx)
            .expect("draft should be loaded in the agent panel")
            .read(cx)
            .message_editor
            .clone()
    });
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("keep this draft", window, cx);
    });

    let terminal_id = worktree_panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();
    let live_blocks = worktree_panel.read_with(cx, |panel, cx| {
        panel.draft_prompt_blocks_if_in_memory(draft_id, cx)
    });
    assert!(
        matches!(
            live_blocks.as_deref(),
            Some([acp::ContentBlock::Text(text)]) if text.text == "keep this draft"
        ),
        "edited draft should still be readable from the panel after opening the terminal"
    );

    assert_eq!(
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace
            .workspaces()
            .count()),
        2,
        "should start with main and linked worktree workspaces"
    );

    worktree_panel.update(cx, |panel, cx| {
        panel.emit_test_terminal_close(terminal_id, cx);
    });
    for _ in 0..4 {
        cx.run_until_parked();
    }

    let terminal_metadata_deleted = cx.update(|_, cx| {
        TerminalThreadMetadataStore::global(cx)
            .read(cx)
            .entry(terminal_id)
            .is_none()
    });
    assert!(
        terminal_metadata_deleted,
        "terminal metadata should be deleted after close"
    );
    let unarchived_worktree_threads = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries_for_path(&worktree_folder_paths, None)
            .count()
    });
    assert_eq!(
        unarchived_worktree_threads, 1,
        "edited draft should remain as a worktree thread reference"
    );
    assert!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace_for_paths(&worktree_folder_paths, None, cx)
            })
            .is_some(),
        "linked worktree workspace should stay open while an edited draft references it"
    );
    assert!(
        fs.is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should remain on disk while an edited draft references it"
    );
}

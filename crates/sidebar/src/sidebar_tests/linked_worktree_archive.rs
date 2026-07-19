use super::*;

#[gpui::test]
async fn test_archive_selected_draft_archives_linked_worktree_after_last_draft(
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
    add_agent_panel(&worktree_workspace, cx);

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
    let first_draft_id = save_draft_metadata_with_main_paths(
        Some("First Draft".into()),
        worktree_folder_paths.clone(),
        PathList::new(&[PathBuf::from("/project")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 3, 0, 0, 0).unwrap(),
        cx,
    );
    let second_draft_id = save_draft_metadata_with_main_paths(
        Some("Second Draft".into()),
        worktree_folder_paths.clone(),
        PathList::new(&[PathBuf::from("/project")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 4, 0, 0, 0).unwrap(),
        cx,
    );
    cx.update(|_, cx| {
        agent_ui::draft_prompt_store::write(
            first_draft_id,
            &[acp::ContentBlock::Text(acp::TextContent::new(
                "first draft",
            ))],
            cx,
        )
    })
    .await
    .expect("first draft prompt should persist");
    cx.update(|_, cx| {
        agent_ui::draft_prompt_store::write(
            second_draft_id,
            &[acp::ContentBlock::Text(acp::TextContent::new(
                "second draft",
            ))],
            cx,
        )
    })
    .await
    .expect("second draft prompt should persist");
    sidebar.update(cx, |sidebar, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    let first_draft_index = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .position(|entry| {
                matches!(
                    entry,
                    ListEntry::Thread(thread) if thread.metadata.thread_id == first_draft_id
                )
            })
            .expect("first draft should be visible in sidebar")
    });
    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(first_draft_index);
    });
    cx.dispatch_action(ArchiveSelectedThread);
    for _ in 0..4 {
        cx.run_until_parked();
    }

    let first_draft_metadata_deleted = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(first_draft_id)
            .is_none()
    });
    assert!(
        first_draft_metadata_deleted,
        "first discarded draft metadata should be deleted"
    );
    let second_draft_metadata_kept = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(second_draft_id)
            .is_some()
    });
    assert!(
        second_draft_metadata_kept,
        "remaining contentful draft should still block worktree archival"
    );
    assert!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace_for_paths(&worktree_folder_paths, None, cx)
            })
            .is_some(),
        "linked worktree workspace should remain while another draft references it"
    );
    assert!(
        fs.is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should remain while another draft references it"
    );

    let second_draft_index = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .position(|entry| {
                matches!(
                    entry,
                    ListEntry::Thread(thread) if thread.metadata.thread_id == second_draft_id
                )
            })
            .expect("second draft should be visible in sidebar")
    });
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(second_draft_index);
    });
    cx.dispatch_action(ArchiveSelectedThread);
    for _ in 0..8 {
        cx.run_until_parked();
    }

    let second_draft_metadata_deleted = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(second_draft_id)
            .is_none()
    });
    assert!(
        second_draft_metadata_deleted,
        "last discarded draft metadata should be deleted"
    );
    assert!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace_for_paths(&worktree_folder_paths, None, cx)
            })
            .is_none(),
        "linked worktree workspace should be removed after closing its last draft"
    );
    assert!(
        !fs.is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should be removed from disk after closing its last draft"
    );
}

#[gpui::test]
async fn test_archive_selected_draft_archives_closed_linked_worktree(cx: &mut TestAppContext) {
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
    main_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

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
        Some("Closed Worktree Draft".into()),
        worktree_folder_paths.clone(),
        PathList::new(&[PathBuf::from("/project")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 3, 0, 0, 0).unwrap(),
        cx,
    );
    cx.update(|_, cx| {
        agent_ui::draft_prompt_store::write(
            draft_id,
            &[acp::ContentBlock::Text(acp::TextContent::new(
                "closed draft",
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
            .expect("closed worktree draft should be visible in sidebar")
    });
    sidebar.read_with(cx, |sidebar, _cx| {
        match &sidebar.contents.entries[draft_index] {
            ListEntry::Thread(thread) => match &thread.workspace {
                ThreadEntryWorkspace::Closed { folder_paths, .. } => {
                    assert_eq!(folder_paths, &worktree_folder_paths);
                }
                ThreadEntryWorkspace::Open(_) => {
                    panic!("linked worktree draft should start closed")
                }
            },
            _ => panic!("expected draft row"),
        }
    });

    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(draft_index);
    });
    cx.dispatch_action(ArchiveSelectedThread);
    for _ in 0..8 {
        cx.run_until_parked();
    }

    let draft_metadata_deleted = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(draft_id)
            .is_none()
    });
    assert!(
        draft_metadata_deleted,
        "discarded closed worktree draft metadata should be deleted"
    );
    assert!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace_for_paths(&worktree_folder_paths, None, cx)
            })
            .is_none(),
        "temporary linked worktree workspace should be removed after discarding its last draft"
    );
    assert_eq!(
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace
            .workspaces()
            .count()),
        1,
        "discarding a closed linked worktree draft should leave only the main workspace"
    );
    assert!(
        !fs.is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should be removed from disk after discarding its last draft"
    );
}

#[gpui::test]
async fn test_terminal_close_event_closes_sidebar_terminal(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [my-project]", "  Dev Server"]
    );

    panel.update(cx, |panel, cx| {
        panel.emit_test_terminal_close(terminal_id, cx);
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, _cx| {
        assert!(!panel.has_terminal(terminal_id));
    });
    sidebar.read_with(cx, |sidebar, _cx| {
        assert!(sidebar.contents.entries.iter().all(|entry| {
            !matches!(entry, ListEntry::Terminal(terminal) if terminal.metadata.terminal_id == terminal_id)
        }));
    });
    sidebar.read_with(cx, |_sidebar, cx| {
        assert!(
            TerminalThreadMetadataStore::global(cx)
                .read(cx)
                .entry(terminal_id)
                .is_none(),
            "terminal metadata should be deleted when the terminal requests close"
        );
    });
}

#[gpui::test]
async fn test_agent_panel_terminal_notifications_update_sidebar(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let build_terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Build", true, window, cx)
        })
        .expect("build test terminal should be inserted");
    let server_terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Server", true, window, cx)
        })
        .expect("server test terminal should be inserted");
    cx.run_until_parked();

    panel.read_with(cx, |panel, _cx| {
        assert_eq!(panel.active_terminal_id(), Some(server_terminal_id));
    });

    panel.update(cx, |panel, cx| {
        panel.emit_test_terminal_bell(build_terminal_id, cx);
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, cx| {
        assert!(sidebar.has_notifications(cx));
        assert!(sidebar.contents.notified_terminals.contains(&build_terminal_id));
        assert!(sidebar.contents.entries.iter().any(|entry| {
            matches!(entry, ListEntry::Terminal(terminal) if terminal.metadata.terminal_id == build_terminal_id && terminal.has_notification)
        }));
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.activate_terminal(build_terminal_id, true, window, cx);
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, cx| {
        assert!(!sidebar.has_notifications(cx));
        assert!(
            !sidebar
                .contents
                .notified_terminals
                .contains(&build_terminal_id)
        );
    });
}

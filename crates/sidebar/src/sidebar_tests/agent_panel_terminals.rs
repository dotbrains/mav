use super::*;

#[gpui::test]
async fn test_agent_panel_terminals_appear_in_sidebar_and_search(cx: &mut TestAppContext) {
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
    sidebar.read_with(cx, |sidebar, _cx| {
        assert!(
            matches!(&sidebar.active_entry, Some(ActiveEntry::Terminal { terminal_id: active_terminal_id, .. }) if *active_terminal_id == terminal_id),
            "expected active terminal entry, got {:?}",
            sidebar.active_entry,
        );
        assert!(
            sidebar.contents.entries.iter().any(|entry| {
                matches!(entry, ListEntry::Terminal(terminal) if terminal.metadata.terminal_id == terminal_id && terminal.metadata.display_title().as_ref() == "Dev Server")
            }),
            "expected the inserted terminal to appear in sidebar contents",
        );
    });
    sidebar.read_with(cx, |_sidebar, cx| {
        let store = TerminalThreadMetadataStore::global(cx).read(cx);
        let metadata = store
            .entry(terminal_id)
            .expect("terminal metadata should be persisted");
        assert_eq!(metadata.title.as_ref(), "");
        assert_eq!(
            metadata.custom_title.as_ref().map(|title| title.as_ref()),
            Some("Dev Server")
        );
        assert_eq!(metadata.display_title().as_ref(), "Dev Server");
        assert!(
            metadata
                .folder_paths()
                .paths()
                .iter()
                .any(|path| path.as_path() == Path::new("/my-project"))
        );
    });

    type_in_search(&sidebar, "server", cx);
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [my-project]", "  Dev Server  <== selected"]
    );

    type_in_search(&sidebar, "missing", cx);
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        Vec::<String>::new()
    );
}

#[gpui::test]
async fn test_closing_last_agent_panel_terminal_restores_empty_header(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    assert_project_header_has_threads(&sidebar, "my-project", false, cx);

    let terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    assert_project_header_has_threads(&sidebar, "my-project", true, cx);

    let (terminal_metadata, terminal_workspace) = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .find_map(|entry| match entry {
                ListEntry::Terminal(terminal) if terminal.metadata.terminal_id == terminal_id => {
                    Some((terminal.metadata.clone(), terminal.workspace.clone()))
                }
                _ => None,
            })
            .expect("terminal should be visible in sidebar")
    });
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.close_terminal(&terminal_metadata, &terminal_workspace, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, cx| {
        assert!(!panel.has_terminal(terminal_id));
        assert!(
            panel.active_view_is_new_draft(cx),
            "closing the active terminal should leave the panel on its empty draft"
        );
    });
    // Closing the terminal drops the user back onto the panel's empty
    // draft. The sidebar mirrors that with a "New {agent} Thread"
    // placeholder row, so the header reports having threads.
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [my-project]", "  New Mav Agent Thread"]
    );
    assert_project_header_has_threads(&sidebar, "my-project", true, cx);

    let project_group_key = multi_workspace.read_with(cx, |multi_workspace, cx| {
        multi_workspace.workspace().read(cx).project_group_key(cx)
    });
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.toggle_collapse(&project_group_key, window, cx);
    });
    cx.run_until_parked();

    // Collapsed: header hides children but still reports the placeholder
    // as a thread present in the group.
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["> [my-project]"]
    );
    assert_project_header_has_threads(&sidebar, "my-project", true, cx);
}

#[gpui::test]
async fn test_agent_panel_terminal_metadata_remains_visible_after_panel_is_removed(
    cx: &mut TestAppContext,
) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    let workspace = multi_workspace.read_with(cx, |multi_workspace, _cx| {
        multi_workspace.workspace().clone()
    });

    let terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.remove_panel(&panel, window, cx);
    });
    sidebar.update(cx, |sidebar, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.panel::<AgentPanel>(cx).is_none()
    }));
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [my-project]", "  Dev Server"]
    );

    sidebar.read_with(cx, |sidebar, _cx| {
        assert!(sidebar.contents.entries.iter().any(|entry| {
            matches!(entry, ListEntry::Terminal(terminal) if terminal.metadata.terminal_id == terminal_id)
        }));
    });
}

#[gpui::test]
async fn test_terminal_metadata_is_deduped_across_project_groups(cx: &mut TestAppContext) {
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        cx.set_global(agent_ui::MaxIdleRetainedThreads(1));
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let project_b = project::Project::test(fs.clone(), ["/project-b".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a, window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    let workspace_a = multi_workspace.read_with(cx, |multi_workspace, _cx| {
        multi_workspace.workspace().clone()
    });
    multi_workspace.update_in(cx, |multi_workspace, window, cx| {
        multi_workspace.test_add_workspace(project_b, window, cx);
    });
    let terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Original", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    workspace_a.update_in(cx, |workspace, window, cx| {
        workspace.remove_panel(&panel, window, cx);
    });
    let now = Utc::now();
    let metadata = TerminalThreadMetadata {
        terminal_id,
        title: "Dev Server".into(),
        custom_title: None,
        created_at: now,
        worktree_paths: WorktreePaths::from_path_lists(
            PathList::new(&[PathBuf::from("/project-a")]),
            PathList::new(&[PathBuf::from("/project-b")]),
        )
        .unwrap(),
        remote_connection: None,
        working_directory: None,
    };

    cx.update(|_, cx| {
        TerminalThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(metadata, cx);
        });
    });
    sidebar.update(cx, |sidebar, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _cx| {
        assert_eq!(
            sidebar
                .contents
                .entries
                .iter()
                .filter(|entry| {
                    matches!(
                        entry,
                        ListEntry::Terminal(terminal)
                            if terminal.metadata.terminal_id == terminal_id
                    )
                })
                .count(),
            1
        );
    });
}

#[gpui::test]
async fn test_agent_panel_terminal_shows_project_and_linked_worktree(cx: &mut TestAppContext) {
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        cx.set_global(agent_ui::MaxIdleRetainedThreads(1));
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", serde_json::json!({ ".git": {}, "src": {} }))
        .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/wt-feature-a"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project = project::Project::test(fs.clone(), ["/wt-feature-a".as_ref()], cx).await;

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
    let panel = add_agent_panel(&worktree_workspace, cx);

    panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [project]", "  Dev Server {wt-feature-a}"]
    );

    type_in_search(&sidebar, "wt-feature-a", cx);
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [project]", "  Dev Server {wt-feature-a}  <== selected"]
    );
}

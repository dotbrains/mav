use super::*;

#[gpui::test]
async fn test_initial_content_for_thread_summary_uses_own_session_id(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let source_session_id = acp::SessionId::new("source-thread-session");
    let source_title: SharedString = "Source Thread Title".into();
    let db_thread = agent::DbThread {
        title: source_title.clone(),
        messages: Vec::new(),
        updated_at: Utc::now(),
        detailed_summary: None,
        initial_project_snapshot: None,
        cumulative_token_usage: Default::default(),
        request_token_usage: HashMap::default(),
        model: None,
        profile: None,
        subagent_context: None,
        speed: None,
        thinking_enabled: false,
        thinking_effort: None,
        draft_prompt: None,
        ui_scroll_position: None,
        sandboxed_terminal_temp_dir: None,
        sandbox_grants: Default::default(),
    };

    let thread_store = cx.update(|cx| ThreadStore::global(cx));
    thread_store
        .update(cx, |store, cx| {
            store.save_thread(
                source_session_id.clone(),
                db_thread,
                PathList::default(),
                cx,
            )
        })
        .await
        .expect("saving source thread should succeed");
    cx.run_until_parked();

    thread_store.read_with(cx, |store, _cx| {
        let entry = store
            .thread_from_session_id(&source_session_id)
            .expect("saved thread should be listed in the store");
        assert!(
            entry.parent_session_id.is_none(),
            "saved thread is a root thread with no parent session"
        );
    });

    let content = cx
        .update(|cx| AgentPanel::initial_content_for_thread_summary(source_session_id.clone(), cx))
        .expect("initial content should be produced for a root thread");

    match content {
        AgentInitialContent::ThreadSummary { session_id, title } => {
            assert_eq!(
                session_id, source_session_id,
                "thread-summary mention should use the source thread's own session id"
            );
            assert_eq!(title, Some(source_title.clone()));
        }
        _ => panic!("expected AgentInitialContent::ThreadSummary"),
    }

    let missing = cx.update(|cx| {
        AgentPanel::initial_content_for_thread_summary(acp::SessionId::new("does-not-exist"), cx)
    });
    assert!(
        missing.is_none(),
        "unknown session ids should not produce initial content"
    );
}

#[test]
fn test_deserialize_agent_variants() {
    assert_eq!(
        serde_json::from_str::<Agent>(r#""NativeAgent""#).unwrap(),
        Agent::NativeAgent,
    );
    assert_eq!(
        serde_json::from_str::<Agent>(r#"{"Custom":{"name":"my-agent"}}"#).unwrap(),
        Agent::Custom {
            id: "my-agent".into(),
        },
    );

    assert_eq!(
        serde_json::from_str::<Agent>(r#""TextThread""#).unwrap(),
        Agent::NativeAgent,
    );

    assert_eq!(
        serde_json::from_str::<Agent>(r#""native_agent""#).unwrap(),
        Agent::NativeAgent,
    );
    assert_eq!(
        serde_json::from_str::<Agent>(r#"{"custom":{"name":"my-agent"}}"#).unwrap(),
        Agent::Custom {
            id: "my-agent".into(),
        },
    );

    assert_eq!(
        serde_json::to_string(&Agent::NativeAgent).unwrap(),
        r#""native_agent""#,
    );
    assert_eq!(
        serde_json::to_string(&Agent::Custom {
            id: "my-agent".into()
        })
        .unwrap(),
        r#"{"custom":{"name":"my-agent"}}"#,
    );
}

#[gpui::test]
fn test_resolve_worktree_branch_target() {
    let resolved = git_ui::worktree_service::resolve_worktree_branch_target(
        &NewWorktreeBranchTarget::ExistingBranch {
            name: "feature".to_string(),
        },
    );
    assert_eq!(resolved, Some("feature".to_string()));

    let resolved = git_ui::worktree_service::resolve_worktree_branch_target(
        &NewWorktreeBranchTarget::CurrentBranch,
    );
    assert_eq!(resolved, None);

    let resolved = git_ui::worktree_service::resolve_worktree_branch_target(
        &NewWorktreeBranchTarget::RemoteBranch {
            remote_name: "origin".to_string(),
            branch_name: "main".to_string(),
        },
    );
    assert_eq!(resolved, Some("refs/remotes/origin/main".to_string()));
}

#[gpui::test]
async fn test_selected_agent_syncs_when_navigating_between_threads(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;

    let stub_agent = Agent::Custom { id: "Test".into() };

    let connection_a = StubAgentConnection::new();
    connection_a.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("response a".into()),
    )]);
    open_thread_with_connection(&panel, connection_a, &mut cx);
    let _session_id_a = active_session_id(&panel, &cx);
    let thread_id_a = active_thread_id(&panel, &cx);
    send_message(&panel, &mut cx);
    cx.run_until_parked();

    panel.read_with(&cx, |panel, _cx| {
        assert_eq!(panel.selected_agent, stub_agent);
    });

    let custom_agent = Agent::Custom {
        id: "my-custom-agent".into(),
    };
    let connection_b = StubAgentConnection::new()
        .with_agent_id("my-custom-agent".into())
        .with_telemetry_id("my-custom-agent".into());
    connection_b.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("response b".into()),
    )]);
    open_thread_with_custom_connection(&panel, connection_b, &mut cx);
    send_message(&panel, &mut cx);
    cx.run_until_parked();

    panel.read_with(&cx, |panel, _cx| {
        assert_eq!(
            panel.selected_agent, custom_agent,
            "selected_agent should have changed to the custom agent"
        );
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.load_agent_thread(
            stub_agent.clone(),
            thread_id_a,
            None,
            None,
            true,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
    });

    panel.read_with(&cx, |panel, _cx| {
        assert_eq!(
            panel.selected_agent, stub_agent,
            "selected_agent should sync back to thread A's agent"
        );
    });
}

#[gpui::test]
async fn test_classify_worktrees_skips_non_git_root_with_nested_repo(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/repo_a", json!({ ".git": {}, "src": { "main.rs": "" } }))
        .await;
    fs.insert_tree("/repo_b", json!({ ".git": {}, "src": { "lib.rs": "" } }))
        .await;
    fs.insert_tree(
        "/plain_dir",
        json!({ "nested_repo": { ".git": {}, "src": { "lib.rs": "" } } }),
    )
    .await;

    let project = Project::test(
        fs.clone(),
        [
            Path::new("/repo_a"),
            Path::new("/repo_b"),
            Path::new("/plain_dir"),
        ],
        cx,
    )
    .await;

    cx.executor().run_until_parked();

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let workspace = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        cx.new(|cx| AgentPanel::new(workspace, window, cx))
    });

    cx.run_until_parked();

    panel.read_with(cx, |panel, cx| {
        let (git_repos, non_git_paths) =
            git_ui::worktree_service::classify_worktrees(panel.project.read(cx), cx);

        let git_work_dirs: Vec<PathBuf> = git_repos
            .iter()
            .map(|repo| repo.read(cx).work_directory_abs_path.to_path_buf())
            .collect();

        assert_eq!(
            git_repos.len(),
            2,
            "only repo_a and repo_b should be classified as git repos, but got: {git_work_dirs:?}"
        );
        assert!(
            git_work_dirs.contains(&PathBuf::from("/repo_a")),
            "repo_a should be in git_repos: {git_work_dirs:?}"
        );
        assert!(
            git_work_dirs.contains(&PathBuf::from("/repo_b")),
            "repo_b should be in git_repos: {git_work_dirs:?}"
        );

        assert_eq!(
            non_git_paths,
            vec![PathBuf::from("/plain_dir")],
            "plain_dir should be classified as a non-git path"
        );
    });
}

#[gpui::test]
async fn test_vim_search_does_not_steal_focus_from_agent_panel(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        vim::init(cx);
        search::init(cx);

        settings::SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |s| s.vim_mode = Some(true));
        });

        let mut vim_key_bindings =
            settings::KeymapFile::load_asset_allow_partial_failure("keymaps/vim.json", cx).unwrap();
        for key_binding in &mut vim_key_bindings {
            key_binding.set_meta(settings::KeybindSource::Vim.meta());
        }
        cx.bind_keys(vim_key_bindings);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({ "file.txt": "hello world" }))
        .await;
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();
    let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);

    workspace
        .update_in(&mut cx, |workspace, window, cx| {
            workspace.open_paths(
                vec![PathBuf::from("/project/file.txt")],
                workspace::OpenOptions::default(),
                None,
                window,
                cx,
            )
        })
        .await;
    cx.run_until_parked();

    workspace.update_in(&mut cx, |workspace, window, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            pane.toolbar().update(cx, |toolbar, cx| {
                let search_bar = cx.new(|cx| search::BufferSearchBar::new(None, window, cx));
                toolbar.add_item(search_bar, window, cx);
            });
        });
    });

    let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    open_thread_with_connection(&panel, StubAgentConnection::new(), &mut cx);

    workspace.update_in(&mut cx, |workspace, window, cx| {
        workspace.focus_panel::<AgentPanel>(window, cx);
    });
    cx.run_until_parked();

    workspace.update_in(&mut cx, |_, window, cx| {
        assert!(
            panel.read(cx).focus_handle(cx).contains_focused(window, cx),
            "Agent panel should be focused before pressing '/'"
        );
    });

    cx.simulate_keystrokes("/");

    workspace.update_in(&mut cx, |_, window, cx| {
        assert!(
            panel.read(cx).focus_handle(cx).contains_focused(window, cx),
            "Focus should remain on the agent panel after pressing '/'"
        );
    });
}

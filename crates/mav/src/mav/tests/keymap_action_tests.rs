use super::*;

#[gpui::test]
async fn test_base_keymap(cx: &mut gpui::TestAppContext) {
    let executor = cx.executor();
    let app_state = init_keymap_test(cx);
    let project = Project::test(app_state.fs.clone(), [], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    // From the Atom keymap
    use workspace::ActivatePreviousPane;
    // From the JetBrains keymap
    use workspace::ActivatePreviousItem;

    app_state
        .fs
        .save(
            paths::settings_file(),
            &r#"{"base_keymap": "Atom"}"#.into(),
            Default::default(),
        )
        .await
        .unwrap();

    app_state
        .fs
        .save(
            "/keymap.json".as_ref(),
            &r#"[{"bindings": {"backspace": "test_only::ActionA"}}]"#.into(),
            Default::default(),
        )
        .await
        .unwrap();
    executor.run_until_parked();
    cx.update(|cx| {
        let (keymap_rx, keymap_watcher) = watch_config_file(
            &executor,
            app_state.fs.clone(),
            PathBuf::from("/keymap.json"),
        );
        watch_settings_files(app_state.fs.clone(), cx);
        handle_keymap_file_changes(keymap_rx, keymap_watcher, cx);
    });
    window
        .update(cx, |_, _, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.register_action(|_, _: &ActionA, _window, _cx| {});
                workspace.register_action(|_, _: &ActionB, _window, _cx| {});
                workspace.register_action(|_, _: &ActivatePreviousPane, _window, _cx| {});
                workspace.register_action(|_, _: &ActivatePreviousItem, _window, _cx| {});
                cx.notify();
            });
        })
        .unwrap();
    executor.run_until_parked();
    // Test loading the keymap base at all
    assert_key_bindings_for(
        window.into(),
        cx,
        vec![("backspace", &ActionA), ("k", &ActivatePreviousPane)],
        line!(),
    );

    // Test modifying the users keymap, while retaining the base keymap
    app_state
        .fs
        .save(
            "/keymap.json".as_ref(),
            &r#"[{"bindings": {"backspace": "test_only::ActionB"}}]"#.into(),
            Default::default(),
        )
        .await
        .unwrap();

    executor.run_until_parked();

    assert_key_bindings_for(
        window.into(),
        cx,
        vec![("backspace", &ActionB), ("k", &ActivatePreviousPane)],
        line!(),
    );

    // Test modifying the base, while retaining the users keymap
    app_state
        .fs
        .save(
            paths::settings_file(),
            &r#"{"base_keymap": "JetBrains"}"#.into(),
            Default::default(),
        )
        .await
        .unwrap();

    executor.run_until_parked();

    assert_key_bindings_for(
        window.into(),
        cx,
        vec![
            ("backspace", &ActionB),
            ("{", &ActivatePreviousItem::default()),
        ],
        line!(),
    );
}

#[gpui::test]
async fn test_disabled_keymap_binding(cx: &mut gpui::TestAppContext) {
    let executor = cx.executor();
    let app_state = init_keymap_test(cx);
    let project = Project::test(app_state.fs.clone(), [], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    // From the Atom keymap
    use workspace::ActivatePreviousPane;
    // From the JetBrains keymap
    use diagnostics::Deploy;

    window
        .update(cx, |_, _, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.register_action(|_, _: &ActionA, _window, _cx| {});
                workspace.register_action(|_, _: &ActionB, _window, _cx| {});
                workspace.register_action(|_, _: &Deploy, _window, _cx| {});
                cx.notify();
            });
        })
        .unwrap();
    app_state
        .fs
        .save(
            paths::settings_file(),
            &r#"{"base_keymap": "Atom"}"#.into(),
            Default::default(),
        )
        .await
        .unwrap();
    app_state
        .fs
        .save(
            "/keymap.json".as_ref(),
            &r#"[{"bindings": {"backspace": "test_only::ActionA"}}]"#.into(),
            Default::default(),
        )
        .await
        .unwrap();

    cx.update(|cx| {
        let (keymap_rx, keymap_watcher) = watch_config_file(
            &executor,
            app_state.fs.clone(),
            PathBuf::from("/keymap.json"),
        );

        watch_settings_files(app_state.fs.clone(), cx);
        handle_keymap_file_changes(keymap_rx, keymap_watcher, cx);
    });

    cx.background_executor.run_until_parked();

    cx.background_executor.run_until_parked();
    // Test loading the keymap base at all
    assert_key_bindings_for(
        window.into(),
        cx,
        vec![("backspace", &ActionA), ("k", &ActivatePreviousPane)],
        line!(),
    );

    // Test disabling the key binding for the base keymap
    app_state
        .fs
        .save(
            "/keymap.json".as_ref(),
            &r#"[{"bindings": {"backspace": null}}]"#.into(),
            Default::default(),
        )
        .await
        .unwrap();

    cx.background_executor.run_until_parked();

    assert_key_bindings_for(
        window.into(),
        cx,
        vec![("k", &ActivatePreviousPane)],
        line!(),
    );

    // Test modifying the base, while retaining the users keymap
    app_state
        .fs
        .save(
            paths::settings_file(),
            &r#"{"base_keymap": "JetBrains"}"#.into(),
            Default::default(),
        )
        .await
        .unwrap();

    cx.background_executor.run_until_parked();

    assert_key_bindings_for(window.into(), cx, vec![("6", &Deploy)], line!());
}

#[gpui::test]
async fn test_generate_keymap_json_schema_for_registered_actions(cx: &mut gpui::TestAppContext) {
    init_keymap_test(cx);
    cx.update(|cx| {
        // Make sure it doesn't panic.
        KeymapFile::generate_json_schema_for_registered_actions(cx);
    });
}

/// Checks that action namespaces are the expected set. The purpose of this is to prevent typos
/// and let you know when introducing a new namespace.
#[gpui::test]
async fn test_action_namespaces(cx: &mut gpui::TestAppContext) {
    use itertools::Itertools;

    init_keymap_test(cx);
    cx.update(|cx| {
        let all_actions = cx.all_action_names();

        let mut actions_without_namespace = Vec::new();
        let all_namespaces = all_actions
            .iter()
            .filter_map(|action_name| {
                let namespace = action_name
                    .split("::")
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .skip(1)
                    .rev()
                    .join("::");
                if namespace.is_empty() {
                    actions_without_namespace.push(*action_name);
                }
                if &namespace == "test_only" || &namespace == "stories" {
                    None
                } else {
                    Some(namespace)
                }
            })
            .sorted()
            .dedup()
            .collect::<Vec<_>>();
        assert_eq!(actions_without_namespace, Vec::<&str>::new());

        let expected_namespaces = vec![
            "action",
            "activity_indicator",
            "agent",
            "sidebar",
            "app_menu",
            "assistant",
            "assistant2",
            "auto_update",
            "branch_picker",
            "bedrock",
            "branches",
            "buffer_search",
            "channel_modal",
            "cli",
            "client",
            "collab",
            "collab_panel",
            "command_palette",
            "console",
            "context_server",
            "copilot",
            "csv",
            "debug_panel",
            "debugger",
            "dev",
            "diagnostics",
            "edit_prediction",
            "editor",
            "encoding_selector",
            "feedback",
            "file_finder",
            "git",
            "git_graph",
            "git_onboarding",
            "git_panel",
            "git_picker",
            "go_to_line",
            "highlights_tree_view",
            "icon_theme_selector",
            "image_viewer",
            "inline_assistant",
            "journal",
            "keymap_editor",
            "keystroke_input",
            "language_selector",
            "welcome",
            "line_ending_selector",
            "lsp_tool",
            "markdown",
            "menu",
            "multi_workspace",
            "new_process_modal",
            "notebook",
            "onboarding",
            "outline",
            "outline_panel",
            "pane",
            "panel",
            "picker",
            "project_panel",
            "project_search",
            "project_symbols",
            "projects",
            "recent_projects",
            "remote_debug",
            "repl",
            "search",
            "settings_editor",
            "settings_profile_selector",
            "skill_creator",
            "snippets",
            "stash_picker",
            "svg",
            "syntax_tree_view",
            "tab_switcher",
            "task",
            "terminal",
            "terminal_panel",
            "text_finder",
            "theme",
            "theme_selector",
            "toast",
            "toolchain",
            "variable_list",
            "vim",
            "window",
            "workspace",
            "worktree_picker",
            "mav",
            "mav_actions",
            "mav_predict_onboarding",
            "zeta",
        ];
        assert_eq!(
            all_namespaces,
            expected_namespaces
                .into_iter()
                .map(|namespace| namespace.to_string())
                .sorted()
                .collect::<Vec<_>>()
        );
    });
}

use super::*;

#[gpui::test]
async fn test_lsp_tools_gated_by_feature_flag(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/test"), json!({})).await;
    let project = Project::test(fs, [path!("/test").as_ref()], cx).await;
    let project_context = cx.new(|_cx| ProjectContext::default());
    let context_server_store = project.read_with(cx, |project, _| project.context_server_store());
    let context_server_registry =
        cx.new(|cx| ContextServerRegistry::new(context_server_store.clone(), cx));
    let model = Arc::new(FakeLanguageModel::default());
    let environment = Rc::new(cx.update(|cx| {
        FakeThreadEnvironment::default().with_terminal(FakeTerminalHandle::new_never_exits(cx))
    }));

    let thread = cx.new(|cx| {
        let mut thread = Thread::new(
            project,
            project_context,
            context_server_registry,
            Templates::new(),
            Some(model.clone() as Arc<dyn LanguageModel>),
            cx,
        );
        thread.add_default_tools(environment, cx);
        thread
    });

    let lsp_tool_names = [
        FindReferencesTool::NAME,
        GetCodeActionsTool::NAME,
        ApplyCodeActionTool::NAME,
        GoToDefinitionTool::NAME,
    ];

    // All LSP tools and the rename tool should be registered on the thread
    // regardless of the flag, since the feature flags only control exposure
    // to the model rather than registration.
    thread.read_with(cx, |thread, _| {
        for name in &lsp_tool_names {
            assert!(
                thread.has_registered_tool(name),
                "expected LSP tool {name} to be registered"
            );
        }
        assert!(
            thread.has_registered_tool(RenameTool::NAME),
            "expected rename tool to be registered"
        );
    });

    // Without the `lsp-tool` flag, sending a message should produce a
    // completion request whose tool list excludes the LSP tools.
    // The rename tool is on its own `rename-tool` flag with
    // `enabled_for_staff`, so it is already visible in debug builds.
    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["hello"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let completion = model.pending_completions().pop().unwrap();
    let tool_names = tool_names_for_completion(&completion);
    for name in &lsp_tool_names {
        assert!(
            !tool_names.iter().any(|t| t == name),
            "expected LSP tool {name} to be hidden without the lsp-tool flag, \
             but completion tools were: {tool_names:?}"
        );
    }
    assert!(
        tool_names.iter().any(|t| t == RenameTool::NAME),
        "expected rename tool to be visible (enabled_for_staff in debug builds), \
         but completion tools were: {tool_names:?}"
    );
    // Sanity check: a non-LSP default tool should still be exposed.
    assert!(
        tool_names.iter().any(|t| t == ReadFileTool::NAME),
        "expected non-LSP tools to still be exposed, got: {tool_names:?}"
    );
    model.end_last_completion_stream();
    cx.run_until_parked();

    // Enable the `lsp-tool` flag and send another message; the LSP tools
    // should now appear in the completion request.
    cx.update(|cx| {
        cx.update_flags(false, vec!["lsp-tool".to_string()]);
    });

    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["hello again"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let completion = model.pending_completions().pop().unwrap();
    let tool_names = tool_names_for_completion(&completion);
    for name in &lsp_tool_names {
        assert!(
            tool_names.iter().any(|t| t == name),
            "expected LSP tool {name} to be exposed when lsp-tool flag is on, \
             but completion tools were: {tool_names:?}"
        );
    }
    assert!(
        tool_names.iter().any(|t| t == RenameTool::NAME),
        "expected rename tool to still be exposed, \
         but completion tools were: {tool_names:?}"
    );
}

#[gpui::test]
async fn test_sibling_thread_tools_gated_by_feature_flag(cx: &mut TestAppContext) {
    init_test(cx);

    // `CreateThreadToolFeatureFlag::enabled_for_staff()` returns true, which
    // means tests in debug builds resolve it to ON unless we explicitly
    // override it via `FeatureFlagsSettings`. Register the settings type and
    // install an (empty) `FeatureFlagStore` global so the `cx.has_flag` path
    // actually consults overrides instead of falling back to the
    // staff-debug-build default.
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, _| {
            store.register_setting::<feature_flags::FeatureFlagsSettings>();
        });
        cx.update_flags(false, vec![]);
    });

    fn set_flag_override(value: &str, cx: &mut TestAppContext) {
        cx.update(|cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |content| {
                    content
                        .feature_flags
                        .get_or_insert_default()
                        .insert("create-thread-tool".to_string(), value.to_string());
                });
            });
        });
    }

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/test"), json!({})).await;
    let project = Project::test(fs, [path!("/test").as_ref()], cx).await;
    let project_context = cx.new(|_cx| ProjectContext::default());
    let context_server_store = project.read_with(cx, |project, _| project.context_server_store());
    let context_server_registry =
        cx.new(|cx| ContextServerRegistry::new(context_server_store.clone(), cx));
    let model = Arc::new(FakeLanguageModel::default());
    let environment = Rc::new(cx.update(|cx| {
        FakeThreadEnvironment::default().with_terminal(FakeTerminalHandle::new_never_exits(cx))
    }));

    let thread = cx.new(|cx| {
        let mut thread = Thread::new(
            project,
            project_context,
            context_server_registry,
            Templates::new(),
            Some(model.clone() as Arc<dyn LanguageModel>),
            cx,
        );
        thread.add_default_tools(environment, cx);
        thread
    });

    let sibling_tool_names = [CreateThreadTool::NAME, ListAgentsAndModelsTool::NAME];

    // Like the LSP/rename tools, sibling-thread tools are registered
    // unconditionally and gated only at exposure time. The registration must
    // be visible regardless of the flag's current value.
    thread.read_with(cx, |thread, _| {
        for name in &sibling_tool_names {
            assert!(
                thread.has_registered_tool(name),
                "expected sibling-thread tool {name} to be registered"
            );
        }
    });

    // Flag explicitly off: a completion request must omit the tools.
    set_flag_override("off", cx);
    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["hello"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let completion = model.pending_completions().pop().unwrap();
    let tool_names = tool_names_for_completion(&completion);
    for name in &sibling_tool_names {
        assert!(
            !tool_names.iter().any(|t| t == name),
            "expected {name} to be hidden when create-thread-tool flag is off, \
             but completion tools were: {tool_names:?}"
        );
    }
    // Sanity check: an unrelated default tool should still be exposed.
    assert!(
        tool_names.iter().any(|t| t == ReadFileTool::NAME),
        "expected non-sibling-thread tools to still be exposed, got: {tool_names:?}"
    );
    model.end_last_completion_stream();
    cx.run_until_parked();

    // Flag explicitly on: the next completion request must include both tools.
    set_flag_override("on", cx);
    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["hello again"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let completion = model.pending_completions().pop().unwrap();
    let tool_names = tool_names_for_completion(&completion);
    for name in &sibling_tool_names {
        assert!(
            tool_names.iter().any(|t| t == name),
            "expected {name} to be exposed when create-thread-tool flag is on, \
             but completion tools were: {tool_names:?}"
        );
    }
}

#[gpui::test]
async fn test_parent_cancel_stops_subagent(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        cx.update_flags(true, vec!["subagents".to_string()]);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/test"), json!({})).await;
    let project = Project::test(fs, [path!("/test").as_ref()], cx).await;
    let project_context = cx.new(|_cx| ProjectContext::default());
    let context_server_store = project.read_with(cx, |project, _| project.context_server_store());
    let context_server_registry =
        cx.new(|cx| ContextServerRegistry::new(context_server_store.clone(), cx));
    let model = Arc::new(FakeLanguageModel::default());

    let parent = cx.new(|cx| {
        Thread::new(
            project.clone(),
            project_context.clone(),
            context_server_registry.clone(),
            Templates::new(),
            Some(model.clone()),
            cx,
        )
    });

    let subagent = cx.new(|cx| Thread::new_subagent(&parent, cx));

    parent.update(cx, |thread, _cx| {
        thread.register_running_subagent(subagent.downgrade());
    });

    subagent
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Do work".to_string()], cx)
        })
        .unwrap();
    cx.run_until_parked();

    subagent.read_with(cx, |thread, _| {
        assert!(!thread.is_turn_complete(), "subagent should be running");
    });

    parent.update(cx, |thread, cx| {
        thread.cancel(cx).detach();
    });

    subagent.read_with(cx, |thread, _| {
        assert!(
            thread.is_turn_complete(),
            "subagent should be cancelled when parent cancels"
        );
    });
}

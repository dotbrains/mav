use super::tests::*;
use super::*;

#[test]
fn test_data_retention_error_maps_from_provider_error() {
    let provider_error = LanguageModelCompletionError::DataRetentionConsentRequired {
        model_name: "Claude Fable 5".to_string(),
    };
    let error = ThreadError::from(anyhow!(provider_error));
    assert!(
        matches!(error, ThreadError::DataRetentionConsentRequired),
        "expected ThreadError::DataRetentionConsentRequired, got: {error:?}"
    );
}

#[gpui::test]
async fn test_drop(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, _cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;
    let weak_view = conversation_view.downgrade();
    drop(conversation_view);
    assert!(!weak_view.is_upgradable());
}

#[gpui::test]
async fn test_external_source_prompt_requires_manual_send(cx: &mut TestAppContext) {
    init_test(cx);

    let Some(prompt) = crate::ExternalSourcePrompt::new("Write me a script") else {
        panic!("expected prompt from external source to sanitize successfully");
    };
    let initial_content = AgentInitialContent::FromExternalSource(prompt);

    let (conversation_view, cx) = setup_conversation_view_with_initial_content(
        StubAgentServer::default_response(),
        initial_content,
        cx,
    )
    .await;

    active_thread(&conversation_view, cx).read_with(cx, |view, cx| {
        assert!(view.show_external_source_prompt_warning);
        assert_eq!(view.thread.read(cx).entries().len(), 0);
        assert_eq!(view.message_editor.read(cx).text(cx), "Write me a script");
    });
}

#[gpui::test]
async fn test_external_source_prompt_warning_clears_after_send(cx: &mut TestAppContext) {
    init_test(cx);

    let Some(prompt) = crate::ExternalSourcePrompt::new("Write me a script") else {
        panic!("expected prompt from external source to sanitize successfully");
    };
    let initial_content = AgentInitialContent::FromExternalSource(prompt);

    let (conversation_view, cx) = setup_conversation_view_with_initial_content(
        StubAgentServer::default_response(),
        initial_content,
        cx,
    )
    .await;

    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));
    cx.run_until_parked();

    active_thread(&conversation_view, cx).read_with(cx, |view, cx| {
        assert!(!view.show_external_source_prompt_warning);
        assert_eq!(view.message_editor.read(cx).text(cx), "");
        assert_eq!(view.thread.read(cx).entries().len(), 2);
    });
}

#[gpui::test]
async fn test_agent_code_span_resolver_resolves_worktree_paths(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        util::path!("/project"),
        json!({
            "src": {
                "main.rs": ""
            },
            "README.md": ""
        }),
    )
    .await;

    let project = Project::test(fs, [Path::new(util::path!("/project"))], cx).await;
    let resolver = cx.update(|cx| AgentCodeSpanResolver::new(&project.downgrade(), cx));

    let uri = cx
        .update(|cx| resolver.try_resolve("src/main.rs:10", cx))
        .expect("expected worktree-relative file path to resolve");
    assert_eq!(
        MentionUri::parse(&uri, PathStyle::local()).unwrap(),
        MentionUri::Selection {
            abs_path: Some(PathBuf::from(util::path!("/project/src/main.rs"))),
            line_range: 9..=9,
            column: None,
        }
    );

    let uri = cx
        .update(|cx| resolver.try_resolve("src/main.rs:10:5", cx))
        .expect("expected worktree-relative file path with row and column to resolve");
    assert_eq!(
        MentionUri::parse(&uri, PathStyle::local()).unwrap(),
        MentionUri::Selection {
            abs_path: Some(PathBuf::from(util::path!("/project/src/main.rs"))),
            line_range: 9..=9,
            column: Some(4),
        }
    );

    let uri = cx
        .update(|cx| resolver.try_resolve("src/main.rs:0", cx))
        .expect("`:0` should fall back to a file mention instead of returning None");
    assert_eq!(
        MentionUri::parse(&uri, PathStyle::local()).unwrap(),
        MentionUri::File {
            abs_path: PathBuf::from(util::path!("/project/src/main.rs")),
        }
    );

    assert!(cx.update(|cx| resolver.try_resolve("String", cx)).is_none());
    assert!(
        cx.update(|cx| resolver.try_resolve("does/not/exist.rs", cx))
            .is_none()
    );
    assert!(
        cx.update(|cx| resolver.try_resolve("src/main.rs.", cx))
            .is_some()
    );

    let uri = cx
        .update(|cx| resolver.try_resolve("project/src/main.rs:10", cx))
        .expect("expected root-prefixed worktree path to resolve");
    assert_eq!(
        MentionUri::parse(&uri, PathStyle::local()).unwrap(),
        MentionUri::Selection {
            abs_path: Some(PathBuf::from(util::path!("/project/src/main.rs"))),
            line_range: 9..=9,
            column: None,
        }
    );
}

use super::*;

#[gpui::test]
async fn test_range_format_respects_language_tab_size_override(cx: &mut TestAppContext) {
    let (project, editor, cx, fake_server) = setup_range_format_test(cx).await;

    // Set Rust language override and assert overridden tabsize is sent to language server
    update_test_language_settings(cx, &|settings| {
        settings.languages.0.insert(
            "Rust".into(),
            LanguageSettingsContent {
                tab_size: NonZeroU32::new(8),
                ..Default::default()
            },
        );
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("something_new\n", window, cx)
    });
    assert!(cx.read(|cx| editor.is_dirty(cx)));
    let save = editor
        .update_in(cx, |editor, window, cx| {
            editor.save(
                SaveOptions {
                    format: true,
                    force_format: false,
                    autosave: false,
                },
                project.clone(),
                window,
                cx,
            )
        })
        .unwrap();
    fake_server
        .set_request_handler::<lsp::request::RangeFormatting, _, _>(move |params, _| async move {
            assert_eq!(
                params.text_document.uri,
                lsp::Uri::from_file_path(path!("/file.rs")).unwrap()
            );
            assert_eq!(params.options.tab_size, 8);
            Ok(Some(Vec::new()))
        })
        .next()
        .await;
    save.await;
}

#[gpui::test]
async fn test_document_format_manual_trigger(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.formatter = Some(FormatterList::Single(Formatter::LanguageServer(
            settings::LanguageServerFormatterSpecifier::Current,
        )))
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.rs"), Default::default()).await;

    let project = Project::test(fs, [path!("/").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(Arc::new(Language::new(
        LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".to_string()],
                ..Default::default()
            },
            ..LanguageConfig::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    )));
    update_test_language_settings(cx, &|settings| {
        // Enable Prettier formatting for the same buffer, and ensure
        // LSP is called instead of Prettier.
        settings.defaults.prettier.get_or_insert_default().allowed = Some(true);
    });
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                document_formatting_provider: Some(lsp::OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/file.rs"), cx)
        })
        .await
        .unwrap();

    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        build_editor_with_project(project.clone(), buffer, window, cx)
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("one\ntwo\nthree\n", window, cx)
    });

    let fake_server = fake_servers.next().await.unwrap();

    let format = editor
        .update_in(cx, |editor, window, cx| {
            editor.perform_format(
                project.clone(),
                FormatTrigger::Manual,
                FormatTarget::Buffers(editor.buffer().read(cx).all_buffers()),
                window,
                cx,
            )
        })
        .unwrap();
    fake_server
        .set_request_handler::<lsp::request::Formatting, _, _>(move |params, _| async move {
            assert_eq!(
                params.text_document.uri,
                lsp::Uri::from_file_path(path!("/file.rs")).unwrap()
            );
            assert_eq!(params.options.tab_size, 4);
            Ok(Some(vec![lsp::TextEdit::new(
                lsp::Range::new(lsp::Position::new(0, 3), lsp::Position::new(1, 0)),
                ", ".to_string(),
            )]))
        })
        .next()
        .await;
    format.await;
    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        "one, two\nthree\n"
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("one\ntwo\nthree\n", window, cx)
    });
    // Ensure we don't lock if formatting hangs.
    fake_server.set_request_handler::<lsp::request::Formatting, _, _>(
        move |params, _| async move {
            assert_eq!(
                params.text_document.uri,
                lsp::Uri::from_file_path(path!("/file.rs")).unwrap()
            );
            futures::future::pending::<()>().await;
            unreachable!()
        },
    );
    let format = editor
        .update_in(cx, |editor, window, cx| {
            editor.perform_format(
                project,
                FormatTrigger::Manual,
                FormatTarget::Buffers(editor.buffer().read(cx).all_buffers()),
                window,
                cx,
            )
        })
        .unwrap();
    cx.executor().advance_clock(super::FORMAT_TIMEOUT);
    format.await;
    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        "one\ntwo\nthree\n"
    );
}

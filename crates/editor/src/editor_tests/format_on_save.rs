use super::*;

#[gpui::test]
async fn test_document_format_during_save(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.rs"), Default::default()).await;

    let project = Project::test(fs, [path!("/file.rs").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
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
    assert!(cx.read(|cx| editor.is_dirty(cx)));

    let fake_server = fake_servers.next().await.unwrap();

    {
        fake_server.set_request_handler::<lsp::request::Formatting, _, _>(
            move |params, _| async move {
                assert_eq!(
                    params.text_document.uri,
                    lsp::Uri::from_file_path(path!("/file.rs")).unwrap()
                );
                assert_eq!(params.options.tab_size, 4);
                Ok(Some(vec![lsp::TextEdit::new(
                    lsp::Range::new(lsp::Position::new(0, 3), lsp::Position::new(1, 0)),
                    ", ".to_string(),
                )]))
            },
        );
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
        save.await;

        assert_eq!(
            editor.update(cx, |editor, cx| editor.text(cx)),
            "one, two\nthree\n"
        );
        assert!(!cx.read(|cx| editor.is_dirty(cx)));
    }

    {
        editor.update_in(cx, |editor, window, cx| {
            editor.set_text("one\ntwo\nthree\n", window, cx)
        });
        assert!(cx.read(|cx| editor.is_dirty(cx)));

        // Ensure we can still save even if formatting hangs.
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
        cx.executor().advance_clock(super::FORMAT_TIMEOUT);
        save.await;
        assert_eq!(
            editor.update(cx, |editor, cx| editor.text(cx)),
            "one\ntwo\nthree\n"
        );
    }

    // Set rust language override and assert overridden tabsize is sent to language server
    update_test_language_settings(cx, &|settings| {
        settings.languages.0.insert(
            "Rust".into(),
            LanguageSettingsContent {
                tab_size: NonZeroU32::new(8),
                ..Default::default()
            },
        );
    });

    {
        editor.update_in(cx, |editor, window, cx| {
            editor.set_text("somehting_new\n", window, cx)
        });
        assert!(cx.read(|cx| editor.is_dirty(cx)));
        let _formatting_request_signal = fake_server
            .set_request_handler::<lsp::request::Formatting, _, _>(move |params, _| async move {
                assert_eq!(
                    params.text_document.uri,
                    lsp::Uri::from_file_path(path!("/file.rs")).unwrap()
                );
                assert_eq!(params.options.tab_size, 8);
                Ok(Some(vec![]))
            });
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
        save.await;
    }
}

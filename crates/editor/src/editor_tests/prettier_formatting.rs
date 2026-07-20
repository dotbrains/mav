use super::*;

#[gpui::test]
async fn test_document_format_with_prettier(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.formatter = Some(FormatterList::Single(Formatter::Prettier))
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.ts"), Default::default()).await;

    let project = Project::test(fs, [path!("/file.ts").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    language_registry.add(Arc::new(Language::new(
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    )));
    update_test_language_settings(cx, &|settings| {
        settings.defaults.prettier.get_or_insert_default().allowed = Some(true);
    });

    let test_plugin = "test_plugin";
    let _ = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            prettier_plugins: vec![test_plugin],
            ..Default::default()
        },
    );

    let prettier_format_suffix = project::TEST_PRETTIER_FORMAT_SUFFIX;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/file.ts"), cx)
        })
        .await
        .unwrap();

    let buffer_text = "one\ntwo\nthree\n";
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text(buffer_text, window, cx)
    });

    editor
        .update_in(cx, |editor, window, cx| {
            editor.perform_format(
                project.clone(),
                FormatTrigger::Manual,
                FormatTarget::Buffers(editor.buffer().read(cx).all_buffers()),
                window,
                cx,
            )
        })
        .unwrap()
        .await;
    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        buffer_text.to_string() + prettier_format_suffix,
        "Test prettier formatting was not applied to the original buffer text",
    );

    update_test_language_settings(cx, &|settings| {
        settings.defaults.formatter = Some(FormatterList::default())
    });
    let format = editor.update_in(cx, |editor, window, cx| {
        editor.perform_format(
            project.clone(),
            FormatTrigger::Manual,
            FormatTarget::Buffers(editor.buffer().read(cx).all_buffers()),
            window,
            cx,
        )
    });
    format.await.unwrap();
    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        buffer_text.to_string() + prettier_format_suffix + "\n" + prettier_format_suffix,
        "Autoformatting (via test prettier) was not applied to the original buffer text",
    );
}

#[gpui::test]
async fn test_document_format_with_prettier_explicit_language(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.formatter = Some(FormatterList::Single(Formatter::Prettier))
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.settings"), Default::default())
        .await;

    let project = Project::test(fs, [path!("/file.settings").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    let ts_lang = Arc::new(Language::new(
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..LanguageMatcher::default()
            },
            prettier_parser_name: Some("typescript".to_string()),
            ..LanguageConfig::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    ));

    language_registry.add(ts_lang.clone());

    update_test_language_settings(cx, &|settings| {
        settings.defaults.prettier.get_or_insert_default().allowed = Some(true);
    });

    let test_plugin = "test_plugin";
    let _ = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            prettier_plugins: vec![test_plugin],
            ..Default::default()
        },
    );

    let prettier_format_suffix = project::TEST_PRETTIER_FORMAT_SUFFIX;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/file.settings"), cx)
        })
        .await
        .unwrap();

    project.update(cx, |project, cx| {
        project.set_language_for_buffer(&buffer, ts_lang, cx)
    });

    let buffer_text = "one\ntwo\nthree\n";
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text(buffer_text, window, cx)
    });

    editor
        .update_in(cx, |editor, window, cx| {
            editor.perform_format(
                project.clone(),
                FormatTrigger::Manual,
                FormatTarget::Buffers(editor.buffer().read(cx).all_buffers()),
                window,
                cx,
            )
        })
        .unwrap()
        .await;
    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        buffer_text.to_string() + prettier_format_suffix + "\ntypescript",
        "Test prettier formatting was not applied to the original buffer text",
    );

    update_test_language_settings(cx, &|settings| {
        settings.defaults.formatter = Some(FormatterList::default())
    });
    let format = editor.update_in(cx, |editor, window, cx| {
        editor.perform_format(
            project.clone(),
            FormatTrigger::Manual,
            FormatTarget::Buffers(editor.buffer().read(cx).all_buffers()),
            window,
            cx,
        )
    });
    format.await.unwrap();

    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        buffer_text.to_string()
            + prettier_format_suffix
            + "\ntypescript\n"
            + prettier_format_suffix
            + "\ntypescript",
        "Autoformatting (via test prettier) was not applied to the original buffer text",
    );
}

#[gpui::test]
async fn test_range_format_with_prettier(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.formatter = Some(FormatterList::Single(Formatter::Prettier))
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.ts"), Default::default()).await;

    let project = Project::test(fs, [path!("/file.ts").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    language_registry.add(Arc::new(Language::new(
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    )));
    update_test_language_settings(cx, &|settings| {
        settings.defaults.prettier.get_or_insert_default().allowed = Some(true);
    });

    let test_plugin = "test_plugin";
    let _ = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            prettier_plugins: vec![test_plugin],
            ..Default::default()
        },
    );

    let prettier_range_format_suffix = project::TEST_PRETTIER_RANGE_FORMAT_SUFFIX;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/file.ts"), cx)
        })
        .await
        .unwrap();

    let buffer_text = "one\ntwo\nthree\nfour\nfive\n";
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        build_editor_with_project(project.clone(), buffer, window, cx)
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text(buffer_text, window, cx)
    });

    cx.executor().run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges([Point::new(1, 0)..Point::new(3, 0)])
        });
    });

    let format = editor
        .update_in(cx, |editor, window, cx| {
            editor.format_selections(&FormatSelections, window, cx)
        })
        .unwrap();
    format.await.unwrap();

    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        format!("one\ntwo{prettier_range_format_suffix}\nthree\nfour\nfive\n"),
        "Range formatting (via test prettier) was not applied to the buffer text",
    );
}

#[gpui::test]
async fn test_range_format_with_prettier_explicit_language(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.formatter = Some(FormatterList::Single(Formatter::Prettier))
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.settings"), Default::default())
        .await;

    let project = Project::test(fs, [path!("/file.settings").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    let ts_lang = Arc::new(Language::new(
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..LanguageMatcher::default()
            },
            prettier_parser_name: Some("typescript".to_string()),
            ..LanguageConfig::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    ));

    language_registry.add(ts_lang.clone());

    update_test_language_settings(cx, &|settings| {
        settings.defaults.prettier.get_or_insert_default().allowed = Some(true);
    });

    let test_plugin = "test_plugin";
    let _ = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            prettier_plugins: vec![test_plugin],
            ..Default::default()
        },
    );

    let prettier_range_format_suffix = project::TEST_PRETTIER_RANGE_FORMAT_SUFFIX;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/file.settings"), cx)
        })
        .await
        .unwrap();

    project.update(cx, |project, cx| {
        project.set_language_for_buffer(&buffer, ts_lang, cx)
    });

    let buffer_text = "one\ntwo\nthree\nfour\nfive\n";
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        build_editor_with_project(project.clone(), buffer, window, cx)
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text(buffer_text, window, cx)
    });

    cx.executor().run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges([Point::new(1, 0)..Point::new(3, 0)])
        });
    });

    let format = editor
        .update_in(cx, |editor, window, cx| {
            editor.format_selections(&FormatSelections, window, cx)
        })
        .unwrap();
    format.await.unwrap();

    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        format!("one\ntwo{prettier_range_format_suffix}\ntypescript\nthree\nfour\nfive\n"),
        "Range formatting (via test prettier) was not applied with explicit language",
    );
}

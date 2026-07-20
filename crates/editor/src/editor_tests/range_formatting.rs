use super::*;

#[gpui::test]
async fn test_autosave_with_dirty_buffers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file1.rs": "fn main() { println!(\"hello\"); }",
            "file2.rs": "fn test() { println!(\"test\"); }",
            "file3.rs": "fn other() { println!(\"other\"); }\n",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    let worktree = project.update(cx, |project, cx| project.worktrees(cx).next().unwrap());
    let worktree_id = worktree.update(cx, |worktree, _| worktree.id());

    // Open three buffers
    let buffer_1 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("file1.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_2 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("file2.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_3 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("file3.rs")), cx)
        })
        .await
        .unwrap();

    // Create a multi-buffer with all three buffers
    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(ReadWrite);
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(1, 0)],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(1, 0)],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [Point::new(0, 0)..Point::new(1, 0)],
            0,
            cx,
        );
        multi_buffer
    });

    let editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });

    // Edit only the first buffer
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(
            SelectionEffects::scroll(Autoscroll::Next),
            window,
            cx,
            |s| s.select_ranges(Some(MultiBufferOffset(10)..MultiBufferOffset(10))),
        );
        editor.insert("// edited", window, cx);
    });

    // Verify that only buffer 1 is dirty
    buffer_1.update(cx, |buffer, _| assert!(buffer.is_dirty()));
    buffer_2.update(cx, |buffer, _| assert!(!buffer.is_dirty()));
    buffer_3.update(cx, |buffer, _| assert!(!buffer.is_dirty()));

    // Get write counts after file creation (files were created with initial content)
    // We expect each file to have been written once during creation
    let write_count_after_creation_1 = fs.write_count_for_path(path!("/dir/file1.rs"));
    let write_count_after_creation_2 = fs.write_count_for_path(path!("/dir/file2.rs"));
    let write_count_after_creation_3 = fs.write_count_for_path(path!("/dir/file3.rs"));

    // Perform autosave
    let save_task = editor.update_in(cx, |editor, window, cx| {
        editor.save(
            SaveOptions {
                format: true,
                force_format: false,
                autosave: true,
            },
            project.clone(),
            window,
            cx,
        )
    });
    save_task.await.unwrap();

    // Only the dirty buffer should have been saved
    assert_eq!(
        fs.write_count_for_path(path!("/dir/file1.rs")) - write_count_after_creation_1,
        1,
        "Buffer 1 was dirty, so it should have been written once during autosave"
    );
    assert_eq!(
        fs.write_count_for_path(path!("/dir/file2.rs")) - write_count_after_creation_2,
        0,
        "Buffer 2 was clean, so it should not have been written during autosave"
    );
    assert_eq!(
        fs.write_count_for_path(path!("/dir/file3.rs")) - write_count_after_creation_3,
        0,
        "Buffer 3 was clean, so it should not have been written during autosave"
    );

    // Verify buffer states after autosave
    buffer_1.update(cx, |buffer, _| assert!(!buffer.is_dirty()));
    buffer_2.update(cx, |buffer, _| assert!(!buffer.is_dirty()));
    buffer_3.update(cx, |buffer, _| assert!(!buffer.is_dirty()));

    // Now perform a manual save (format = true)
    let save_task = editor.update_in(cx, |editor, window, cx| {
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
    });
    save_task.await.unwrap();

    // During manual save, clean buffers don't get written to disk
    // They just get did_save called for language server notifications
    assert_eq!(
        fs.write_count_for_path(path!("/dir/file1.rs")) - write_count_after_creation_1,
        1,
        "Buffer 1 should only have been written once total (during autosave, not manual save)"
    );
    assert_eq!(
        fs.write_count_for_path(path!("/dir/file2.rs")) - write_count_after_creation_2,
        0,
        "Buffer 2 should not have been written at all"
    );
    assert_eq!(
        fs.write_count_for_path(path!("/dir/file3.rs")) - write_count_after_creation_3,
        0,
        "Buffer 3 should not have been written at all"
    );
}

async fn setup_range_format_test_with_capabilities(
    cx: &mut TestAppContext,
    capabilities: lsp::ServerCapabilities,
) -> (
    Entity<Project>,
    Entity<Editor>,
    &mut gpui::VisualTestContext,
    lsp::FakeLanguageServer,
) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.rs"), Default::default()).await;

    let project = Project::test(fs, [path!("/").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities,
            ..FakeLspAdapter::default()
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
        window.focus(&editor.focus_handle(cx), cx);
    });

    let fake_server = fake_servers.next().await.unwrap();

    (project, editor, cx, fake_server)
}

async fn setup_range_format_test(
    cx: &mut TestAppContext,
) -> (
    Entity<Project>,
    Entity<Editor>,
    &mut gpui::VisualTestContext,
    lsp::FakeLanguageServer,
) {
    setup_range_format_test_with_capabilities(
        cx,
        lsp::ServerCapabilities {
            document_range_formatting_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
    )
    .await
}

fn refresh_editor_actions(cx: &mut VisualTestContext) {
    cx.executor().run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
}

#[gpui::test]
async fn test_format_selections_action_available_when_range_formatting_is_supported(
    cx: &mut TestAppContext,
) {
    let (_, editor, cx, _) = setup_range_format_test(cx).await;

    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("one\ntwo\nthree\n", window, cx);
        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(1, 0)]);
        });
    });

    refresh_editor_actions(cx);

    assert!(cx.update(|window, cx| { window.is_action_available(&FormatSelections, cx) }));
}

#[gpui::test]
async fn test_format_selections_action_available_for_cursor_when_range_formatting_is_supported(
    cx: &mut TestAppContext,
) {
    let (_, editor, cx, _) = setup_range_format_test(cx).await;

    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("foo\nbar\n", window, cx);
        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges([Point::new(1, 1)..Point::new(1, 1)]);
        });
    });

    refresh_editor_actions(cx);

    assert!(cx.update(|window, cx| { window.is_action_available(&FormatSelections, cx) }));
}

#[gpui::test]
async fn test_format_selections_action_hidden_without_range_formatting_support(
    cx: &mut TestAppContext,
) {
    let (_, editor, cx, _) = setup_range_format_test_with_capabilities(
        cx,
        lsp::ServerCapabilities {
            document_formatting_provider: Some(lsp::OneOf::Left(true)),
            document_range_formatting_provider: Some(lsp::OneOf::Left(false)),
            ..lsp::ServerCapabilities::default()
        },
    )
    .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("one\ntwo\nthree\n", window, cx);
        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(1, 0)]);
        });
    });

    refresh_editor_actions(cx);

    assert!(!cx.update(|window, cx| { window.is_action_available(&FormatSelections, cx) }));
}

#[gpui::test]
async fn test_format_selections_action_hidden_without_range_capable_formatter(
    cx: &mut TestAppContext,
) {
    init_test(cx, |settings| {
        settings.defaults.formatter = Some(FormatterList::Single(Formatter::External {
            command: "awk".into(),
            arguments: Some(vec!["{ print }".to_string()]),
        }));
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.rs"), Default::default()).await;

    let project = Project::test(fs, [path!("/").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let _ = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                document_range_formatting_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            ..FakeLspAdapter::default()
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
        window.focus(&editor.focus_handle(cx), cx);
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("one\ntwo\nthree\n", window, cx);
        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(1, 0)]);
        });
    });

    refresh_editor_actions(cx);

    assert!(!cx.update(|window, cx| { window.is_action_available(&FormatSelections, cx) }));
}

#[gpui::test]
async fn test_range_format_on_save_success(cx: &mut TestAppContext) {
    let (project, editor, cx, fake_server) = setup_range_format_test(cx).await;

    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("one\ntwo\nthree\n", window, cx)
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
            assert_eq!(params.options.tab_size, 4);
            Ok(Some(vec![lsp::TextEdit::new(
                lsp::Range::new(lsp::Position::new(0, 3), lsp::Position::new(1, 0)),
                ", ".to_string(),
            )]))
        })
        .next()
        .await;
    save.await;
    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        "one, two\nthree\n"
    );
    assert!(!cx.read(|cx| editor.is_dirty(cx)));
}

#[gpui::test]
async fn test_range_format_on_save_timeout(cx: &mut TestAppContext) {
    let (project, editor, cx, fake_server) = setup_range_format_test(cx).await;

    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("one\ntwo\nthree\n", window, cx)
    });
    assert!(cx.read(|cx| editor.is_dirty(cx)));

    // Test that save still works when formatting hangs
    fake_server.set_request_handler::<lsp::request::RangeFormatting, _, _>(
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
    assert!(!cx.read(|cx| editor.is_dirty(cx)));
}

#[gpui::test]
async fn test_range_format_not_called_for_clean_buffer(cx: &mut TestAppContext) {
    let (project, editor, cx, fake_server) = setup_range_format_test(cx).await;

    // Buffer starts clean, no formatting should be requested
    let save = editor
        .update_in(cx, |editor, window, cx| {
            editor.save(
                SaveOptions {
                    format: false,
                    force_format: false,
                    autosave: false,
                },
                project.clone(),
                window,
                cx,
            )
        })
        .unwrap();
    let _pending_format_request = fake_server
        .set_request_handler::<lsp::request::RangeFormatting, _, _>(move |_, _| async move {
            panic!("Should not be invoked");
        })
        .next();
    save.await;
    cx.run_until_parked();
}

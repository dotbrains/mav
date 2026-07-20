use super::*;

#[gpui::test]
async fn test_auto_formatter_skips_server_without_formatting(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.rs"), Default::default()).await;

    let project = Project::test(fs, [path!("/file.rs").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    // First server: no formatting capability
    let mut no_format_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "no-format-server",
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions::default()),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    // Second server: has formatting capability
    let mut format_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "format-server",
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

    let _no_format_server = no_format_servers.next().await.unwrap();
    let format_server = format_servers.next().await.unwrap();

    format_server.set_request_handler::<lsp::request::Formatting, _, _>(
        move |params, _| async move {
            assert_eq!(
                params.text_document.uri,
                lsp::Uri::from_file_path(path!("/file.rs")).unwrap()
            );
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
}

#[gpui::test]
async fn test_redo_after_noop_format(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.ensure_final_newline_on_save = Some(false);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.txt"), "foo".into()).await;

    let project = Project::test(fs, [path!("/file.txt").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/file.txt"), cx)
        })
        .await
        .unwrap();

    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        build_editor_with_project(project.clone(), buffer, window, cx)
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(0)..MultiBufferOffset(0)])
        });
    });
    assert!(!cx.read(|cx| editor.is_dirty(cx)));

    editor.update_in(cx, |editor, window, cx| {
        editor.handle_input("\n", window, cx)
    });
    cx.run_until_parked();
    save(&editor, &project, cx).await;
    assert_eq!("\nfoo", editor.read_with(cx, |editor, cx| editor.text(cx)));

    editor.update_in(cx, |editor, window, cx| {
        editor.undo(&Default::default(), window, cx);
    });
    save(&editor, &project, cx).await;
    assert_eq!("foo", editor.read_with(cx, |editor, cx| editor.text(cx)));

    editor.update_in(cx, |editor, window, cx| {
        editor.redo(&Default::default(), window, cx);
    });
    cx.run_until_parked();
    assert_eq!("\nfoo", editor.read_with(cx, |editor, cx| editor.text(cx)));

    async fn save(editor: &Entity<Editor>, project: &Entity<Project>, cx: &mut VisualTestContext) {
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
        assert!(!cx.read(|cx| editor.is_dirty(cx)));
    }
}

#[gpui::test]
async fn test_multibuffer_format_during_save(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let cols = 4;
    let rows = 10;
    let sample_text_1 = sample_text(rows, cols, 'a');
    assert_eq!(
        sample_text_1,
        "aaaa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj"
    );
    let sample_text_2 = sample_text(rows, cols, 'l');
    assert_eq!(
        sample_text_2,
        "llll\nmmmm\nnnnn\noooo\npppp\nqqqq\nrrrr\nssss\ntttt\nuuuu"
    );
    let sample_text_3 = sample_text(rows, cols, 'v').replace('\u{7f}', ".");
    assert_eq!(
        sample_text_3,
        "vvvv\nwwww\nxxxx\nyyyy\nzzzz\n{{{{\n||||\n}}}}\n~~~~\n...."
    );

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": sample_text_1,
            "other.rs": sample_text_2,
            "lib.rs": sample_text_3,
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);

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

    let worktree = project.update(cx, |project, cx| {
        let mut worktrees = project.worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1);
        worktrees.pop().unwrap()
    });
    let worktree_id = worktree.update(cx, |worktree, _| worktree.id());

    let buffer_1 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_2 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("other.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_3 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("lib.rs")), cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(ReadWrite);
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [
                Point::new(0, 0)..Point::new(2, 4),
                Point::new(5, 0)..Point::new(6, 4),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [
                Point::new(0, 0)..Point::new(2, 4),
                Point::new(5, 0)..Point::new(6, 4),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [
                Point::new(0, 0)..Point::new(2, 4),
                Point::new(5, 0)..Point::new(6, 4),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        assert_eq!(multi_buffer.read(cx).excerpts().count(), 9);
        multi_buffer
    });
    let multi_buffer_editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });

    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        let a = editor.text(cx).find("aaaa").unwrap();
        editor.change_selections(
            SelectionEffects::scroll(Autoscroll::Next),
            window,
            cx,
            |s| s.select_ranges(Some(MultiBufferOffset(a + 1)..MultiBufferOffset(a + 2))),
        );
        editor.insert("|one|two|three|", window, cx);
    });
    assert!(cx.read(|cx| multi_buffer_editor.is_dirty(cx)));
    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        let n = editor.text(cx).find("nnnn").unwrap();
        editor.change_selections(
            SelectionEffects::scroll(Autoscroll::Next),
            window,
            cx,
            |s| s.select_ranges(Some(MultiBufferOffset(n + 4)..MultiBufferOffset(n + 14))),
        );
        editor.insert("|four|five|six|", window, cx);
    });
    assert!(cx.read(|cx| multi_buffer_editor.is_dirty(cx)));

    // First two buffers should be edited, but not the third one.
    pretty_assertions::assert_eq!(
        editor_content_with_blocks(&multi_buffer_editor, cx),
        indoc! {"
            § main.rs
            § -----
            a|one|two|three|aa
            bbbb
            cccc
            § -----
            ffff
            gggg
            § -----
            jjjj
            § other.rs
            § -----
            llll
            mmmm
            nnnn|four|five|six|
            § -----

            § -----
            uuuu
            § lib.rs
            § -----
            vvvv
            wwww
            xxxx
            § -----
            {{{{
            ||||
            § -----
            ...."}
    );
    buffer_1.update(cx, |buffer, _| {
        assert!(buffer.is_dirty());
        assert_eq!(
            buffer.text(),
            "a|one|two|three|aa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj",
        )
    });
    buffer_2.update(cx, |buffer, _| {
        assert!(buffer.is_dirty());
        assert_eq!(
            buffer.text(),
            "llll\nmmmm\nnnnn|four|five|six|\noooo\npppp\n\nssss\ntttt\nuuuu",
        )
    });
    buffer_3.update(cx, |buffer, _| {
        assert!(!buffer.is_dirty());
        assert_eq!(buffer.text(), sample_text_3,)
    });
    cx.executor().run_until_parked();

    let save = multi_buffer_editor
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

    let fake_server = fake_servers.next().await.unwrap();
    fake_server
        .server
        .on_request::<lsp::request::Formatting, _, _>(move |_params, _| async move {
            Ok(Some(vec![lsp::TextEdit::new(
                lsp::Range::new(lsp::Position::new(0, 3), lsp::Position::new(1, 0)),
                "[formatted]".to_string(),
            )]))
        })
        .detach();
    save.await;

    // After multibuffer saving, only first two buffers should be reformatted, but not the third one (as it was not dirty).
    assert!(cx.read(|cx| !multi_buffer_editor.is_dirty(cx)));
    assert_eq!(
        editor_content_with_blocks(&multi_buffer_editor, cx),
        indoc! {"
            § main.rs
            § -----
            a|o[formatted]bbbb
            cccc
            § -----
            ffff
            gggg
            § -----
            jjjj

            § other.rs
            § -----
            lll[formatted]mmmm
            nnnn|four|five|six|
            § -----

            § -----
            uuuu

            § lib.rs
            § -----
            vvvv
            wwww
            xxxx
            § -----
            {{{{
            ||||
            § -----
            ...."}
    );
    buffer_1.update(cx, |buffer, _| {
        assert!(!buffer.is_dirty());
        assert_eq!(
            buffer.text(),
            "a|o[formatted]bbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj\n",
        )
    });
    // Diff < left / right > :
    //  lll[formatted]mmmm
    // <nnnn|four|five|six|
    // <oooo
    // >nnnn|four|five|six|oooo
    //  pppp
    // <
    //  ssss
    //  tttt
    //  uuuu

    buffer_2.update(cx, |buffer, _| {
        assert!(!buffer.is_dirty());
        assert_eq!(
            buffer.text(),
            "lll[formatted]mmmm\nnnnn|four|five|six|\noooo\npppp\n\nssss\ntttt\nuuuu\n",
        )
    });
    buffer_3.update(cx, |buffer, _| {
        assert!(!buffer.is_dirty());
        assert_eq!(buffer.text(), sample_text_3,)
    });
}

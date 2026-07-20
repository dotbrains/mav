use super::*;

#[gpui::test]
async fn test_completion_replacing_surrounding_text_with_multicursors(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    // scenario: surrounding text matches completion text
    let completion_text = "to_offset";
    let initial_state = indoc! {"
        1. buf.to_offˇsuffix
        2. buf.to_offˇsuf
        3. buf.to_offˇfix
        4. buf.to_offˇ
        5. into_offˇensive
        6. ˇsuffix
        7. let ˇ //
        8. aaˇzz
        9. buf.to_off«zzzzzˇ»suffix
        10. buf.«ˇzzzzz»suffix
        11. to_off«ˇzzzzz»

        buf.to_offˇsuffix  // newest cursor
    "};
    let completion_marked_buffer = indoc! {"
        1. buf.to_offsuffix
        2. buf.to_offsuf
        3. buf.to_offfix
        4. buf.to_off
        5. into_offensive
        6. suffix
        7. let  //
        8. aazz
        9. buf.to_offzzzzzsuffix
        10. buf.zzzzzsuffix
        11. to_offzzzzz

        buf.<to_off|suffix>  // newest cursor
    "};
    let expected = indoc! {"
        1. buf.to_offsetˇ
        2. buf.to_offsetˇsuf
        3. buf.to_offsetˇfix
        4. buf.to_offsetˇ
        5. into_offsetˇensive
        6. to_offsetˇsuffix
        7. let to_offsetˇ //
        8. aato_offsetˇzz
        9. buf.to_offsetˇ
        10. buf.to_offsetˇsuffix
        11. to_offsetˇ

        buf.to_offsetˇ  // newest cursor
    "};
    cx.set_state(initial_state);
    cx.update_editor(|editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    handle_completion_request_with_insert_and_replace(
        &mut cx,
        completion_marked_buffer,
        vec![(completion_text, completion_text)],
        Arc::new(AtomicUsize::new(0)),
    )
    .await;
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    let apply_additional_edits = cx.update_editor(|editor, window, cx| {
        editor
            .confirm_completion_replace(&ConfirmCompletionReplace, window, cx)
            .unwrap()
    });
    cx.assert_editor_state(expected);
    handle_resolve_completion_request(&mut cx, None).await;
    apply_additional_edits.await.unwrap();

    // scenario: surrounding text matches surroundings of newest cursor, inserting at the end
    let completion_text = "foo_and_bar";
    let initial_state = indoc! {"
        1. ooanbˇ
        2. zooanbˇ
        3. ooanbˇz
        4. zooanbˇz
        5. ooanˇ
        6. oanbˇ

        ooanbˇ
    "};
    let completion_marked_buffer = indoc! {"
        1. ooanb
        2. zooanb
        3. ooanbz
        4. zooanbz
        5. ooan
        6. oanb

        <ooanb|>
    "};
    let expected = indoc! {"
        1. foo_and_barˇ
        2. zfoo_and_barˇ
        3. foo_and_barˇz
        4. zfoo_and_barˇz
        5. ooanfoo_and_barˇ
        6. oanbfoo_and_barˇ

        foo_and_barˇ
    "};
    cx.set_state(initial_state);
    cx.update_editor(|editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    handle_completion_request_with_insert_and_replace(
        &mut cx,
        completion_marked_buffer,
        vec![(completion_text, completion_text)],
        Arc::new(AtomicUsize::new(0)),
    )
    .await;
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    let apply_additional_edits = cx.update_editor(|editor, window, cx| {
        editor
            .confirm_completion_replace(&ConfirmCompletionReplace, window, cx)
            .unwrap()
    });
    cx.assert_editor_state(expected);
    handle_resolve_completion_request(&mut cx, None).await;
    apply_additional_edits.await.unwrap();

    // scenario: surrounding text matches surroundings of newest cursor, inserted at the middle
    // (expects the same as if it was inserted at the end)
    let completion_text = "foo_and_bar";
    let initial_state = indoc! {"
        1. ooˇanb
        2. zooˇanb
        3. ooˇanbz
        4. zooˇanbz

        ooˇanb
    "};
    let completion_marked_buffer = indoc! {"
        1. ooanb
        2. zooanb
        3. ooanbz
        4. zooanbz

        <oo|anb>
    "};
    let expected = indoc! {"
        1. foo_and_barˇ
        2. zfoo_and_barˇ
        3. foo_and_barˇz
        4. zfoo_and_barˇz

        foo_and_barˇ
    "};
    cx.set_state(initial_state);
    cx.update_editor(|editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    handle_completion_request_with_insert_and_replace(
        &mut cx,
        completion_marked_buffer,
        vec![(completion_text, completion_text)],
        Arc::new(AtomicUsize::new(0)),
    )
    .await;
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    let apply_additional_edits = cx.update_editor(|editor, window, cx| {
        editor
            .confirm_completion_replace(&ConfirmCompletionReplace, window, cx)
            .unwrap()
    });
    cx.assert_editor_state(expected);
    handle_resolve_completion_request(&mut cx, None).await;
    apply_additional_edits.await.unwrap();
}

// This used to crash
#[gpui::test]
async fn test_completion_in_multibuffer_with_replace_range(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let buffer_text = indoc! {"
        fn main() {
            10.satu;

            //
            // separate1
            // separate2
            // separate3
            //

            10.satu20;
        }
    "};
    let multibuffer_text_with_selections = indoc! {"
        fn main() {
            10.satuˇ;

            //

            10.satuˇ20;
        }
    "};
    let expected_multibuffer = indoc! {"
        fn main() {
            10.saturating_sub()ˇ;

            //

            10.saturating_sub()ˇ;
        }
    "};

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": buffer_text,
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    resolve_provider: None,
                    ..lsp::CompletionOptions::default()
                }),
                ..lsp::ServerCapabilities::default()
            },
            ..FakeLspAdapter::default()
        },
    );
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(Capability::ReadWrite);
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer.clone(),
            [
                Point::zero()..Point::new(2, 0),
                Point::new(7, 0)..buffer.read(cx).max_point(),
            ],
            0,
            cx,
        );
        multi_buffer
    });

    let editor = workspace.update_in(cx, |_, window, cx| {
        cx.new(|cx| {
            Editor::new(
                EditorMode::Full {
                    scale_ui_elements_with_buffer_font_size: false,
                    show_active_line_background: false,
                    sizing_behavior: SizingBehavior::Default,
                },
                multi_buffer.clone(),
                Some(project.clone()),
                window,
                cx,
            )
        })
    });

    let pane = workspace.update_in(cx, |workspace, _, _| workspace.active_pane().clone());
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(Box::new(editor.clone()), true, true, None, window, cx);
    });

    let fake_server = fake_servers.next().await.unwrap();
    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                Point::new(1, 11)..Point::new(1, 11),
                Point::new(5, 11)..Point::new(5, 11),
            ])
        });

        assert_text_with_selections(editor, multibuffer_text_with_selections, cx);
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });

    fake_server
        .set_request_handler::<lsp::request::Completion, _, _>(move |_, _| async move {
            let completion_item = lsp::CompletionItem {
                label: "saturating_sub()".into(),
                text_edit: Some(lsp::CompletionTextEdit::InsertAndReplace(
                    lsp::InsertReplaceEdit {
                        new_text: "saturating_sub()".to_owned(),
                        insert: lsp::Range::new(
                            lsp::Position::new(9, 7),
                            lsp::Position::new(9, 11),
                        ),
                        replace: lsp::Range::new(
                            lsp::Position::new(9, 7),
                            lsp::Position::new(9, 13),
                        ),
                    },
                )),
                ..lsp::CompletionItem::default()
            };

            Ok(Some(lsp::CompletionResponse::Array(vec![completion_item])))
        })
        .next()
        .await
        .unwrap();

    cx.condition(&editor, |editor, _| editor.context_menu_visible())
        .await;

    editor
        .update_in(cx, |editor, window, cx| {
            editor
                .confirm_completion_replace(&ConfirmCompletionReplace, window, cx)
                .unwrap()
        })
        .await
        .unwrap();

    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, expected_multibuffer, cx);
    })
}

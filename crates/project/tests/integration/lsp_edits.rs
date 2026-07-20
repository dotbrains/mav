use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_edits_from_lsp2_with_past_version(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let text = "
        fn a() {
            f1();
        }
        fn b() {
            f2();
        }
        fn c() {
            f3();
        }
    "
    .unindent();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.rs": text.clone(),
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp("Rust", FakeLspAdapter::default());

    let (buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();

    let mut fake_server = fake_servers.next().await.unwrap();
    let lsp_document_version = fake_server
        .receive_notification::<lsp::notification::DidOpenTextDocument>()
        .await
        .text_document
        .version;

    // Simulate editing the buffer after the language server computes some edits.
    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            [(
                Point::new(0, 0)..Point::new(0, 0),
                "// above first function\n",
            )],
            None,
            cx,
        );
        buffer.edit(
            [(
                Point::new(2, 0)..Point::new(2, 0),
                "    // inside first function\n",
            )],
            None,
            cx,
        );
        buffer.edit(
            [(
                Point::new(6, 4)..Point::new(6, 4),
                "// inside second function ",
            )],
            None,
            cx,
        );

        assert_eq!(
            buffer.text(),
            "
                // above first function
                fn a() {
                    // inside first function
                    f1();
                }
                fn b() {
                    // inside second function f2();
                }
                fn c() {
                    f3();
                }
            "
            .unindent()
        );
    });

    let edits = lsp_store
        .update(cx, |lsp_store, cx| {
            lsp_store.as_local_mut().unwrap().edits_from_lsp(
                &buffer,
                vec![
                    // replace body of first function
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(3, 0)),
                        new_text: "
                            fn a() {
                                f10();
                            }
                            "
                        .unindent(),
                    },
                    // edit inside second function
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(4, 6), lsp::Position::new(4, 6)),
                        new_text: "00".into(),
                    },
                    // edit inside third function via two distinct edits
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(7, 5), lsp::Position::new(7, 5)),
                        new_text: "4000".into(),
                    },
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(7, 5), lsp::Position::new(7, 6)),
                        new_text: "".into(),
                    },
                ],
                LanguageServerId(0),
                Some(lsp_document_version),
                cx,
            )
        })
        .await
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        for (range, new_text) in edits {
            buffer.edit([(range, new_text)], None, cx);
        }
        assert_eq!(
            buffer.text(),
            "
                // above first function
                fn a() {
                    // inside first function
                    f10();
                }
                fn b() {
                    // inside second function f200();
                }
                fn c() {
                    f4000();
                }
                "
            .unindent()
        );
    });
}

#[gpui::test]
async fn test_edits_from_lsp2_with_edits_on_adjacent_lines(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let text = "
        use a::b;
        use a::c;

        fn f() {
            b();
            c();
        }
    "
    .unindent();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.rs": text.clone(),
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();

    // Simulate the language server sending us a small edit in the form of a very large diff.
    // Rust-analyzer does this when performing a merge-imports code action.
    let edits = lsp_store
        .update(cx, |lsp_store, cx| {
            lsp_store.as_local_mut().unwrap().edits_from_lsp(
                &buffer,
                [
                    // Replace the first use statement without editing the semicolon.
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(0, 4), lsp::Position::new(0, 8)),
                        new_text: "a::{b, c}".into(),
                    },
                    // Reinsert the remainder of the file between the semicolon and the final
                    // newline of the file.
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(0, 9), lsp::Position::new(0, 9)),
                        new_text: "\n\n".into(),
                    },
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(0, 9), lsp::Position::new(0, 9)),
                        new_text: "
                            fn f() {
                                b();
                                c();
                            }"
                        .unindent(),
                    },
                    // Delete everything after the first newline of the file.
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(1, 0), lsp::Position::new(7, 0)),
                        new_text: "".into(),
                    },
                ],
                LanguageServerId(0),
                None,
                cx,
            )
        })
        .await
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        let edits = edits
            .into_iter()
            .map(|(range, text)| {
                (
                    range.start.to_point(buffer)..range.end.to_point(buffer),
                    text,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            edits,
            [
                (Point::new(0, 4)..Point::new(0, 8), "a::{b, c}".into()),
                (Point::new(1, 0)..Point::new(2, 0), "".into())
            ]
        );

        for (range, new_text) in edits {
            buffer.edit([(range, new_text)], None, cx);
        }
        assert_eq!(
            buffer.text(),
            "
                use a::{b, c};

                fn f() {
                    b();
                    c();
                }
            "
            .unindent()
        );
    });
}

#[gpui::test]
async fn test_edits_from_lsp_with_replacement_followed_by_adjacent_insertion(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let text = "Path()";

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.rs": text
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();

    // Simulate the language server sending us a pair of edits at the same location,
    // with an insertion following a replacement (which violates the LSP spec).
    let edits = lsp_store
        .update(cx, |lsp_store, cx| {
            lsp_store.as_local_mut().unwrap().edits_from_lsp(
                &buffer,
                [
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 4)),
                        new_text: "Path".into(),
                    },
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 0)),
                        new_text: "from path import Path\n\n\n".into(),
                    },
                ],
                LanguageServerId(0),
                None,
                cx,
            )
        })
        .await
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        buffer.edit(edits, None, cx);
        assert_eq!(buffer.text(), "from path import Path\n\n\nPath()")
    });
}

#[gpui::test]
async fn test_invalid_edits_from_lsp2(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let text = "
        use a::b;
        use a::c;

        fn f() {
            b();
            c();
        }
    "
    .unindent();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.rs": text.clone(),
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();

    // Simulate the language server sending us edits in a non-ordered fashion,
    // with ranges sometimes being inverted or pointing to invalid locations.
    let edits = lsp_store
        .update(cx, |lsp_store, cx| {
            lsp_store.as_local_mut().unwrap().edits_from_lsp(
                &buffer,
                [
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(0, 9), lsp::Position::new(0, 9)),
                        new_text: "\n\n".into(),
                    },
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(0, 8), lsp::Position::new(0, 4)),
                        new_text: "a::{b, c}".into(),
                    },
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(1, 0), lsp::Position::new(99, 0)),
                        new_text: "".into(),
                    },
                    lsp::TextEdit {
                        range: lsp::Range::new(lsp::Position::new(0, 9), lsp::Position::new(0, 9)),
                        new_text: "
                            fn f() {
                                b();
                                c();
                            }"
                        .unindent(),
                    },
                ],
                LanguageServerId(0),
                None,
                cx,
            )
        })
        .await
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        let edits = edits
            .into_iter()
            .map(|(range, text)| {
                (
                    range.start.to_point(buffer)..range.end.to_point(buffer),
                    text,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            edits,
            [
                (Point::new(0, 4)..Point::new(0, 8), "a::{b, c}".into()),
                (Point::new(1, 0)..Point::new(2, 0), "".into())
            ]
        );

        for (range, new_text) in edits {
            buffer.edit([(range, new_text)], None, cx);
        }
        assert_eq!(
            buffer.text(),
            "
                use a::{b, c};

                fn f() {
                    b();
                    c();
                }
            "
            .unindent()
        );
    });
}

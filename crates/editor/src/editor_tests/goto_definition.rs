use super::*;

#[gpui::test]
async fn test_goto_definition_with_find_all_references_fallback(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            definition_provider: Some(lsp::OneOf::Left(true)),
            references_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let set_up_lsp_handlers = |empty_go_to_definition: bool, cx: &mut EditorLspTestContext| {
        let go_to_definition = cx
            .lsp
            .set_request_handler::<lsp::request::GotoDefinition, _, _>(
                move |params, _| async move {
                    if empty_go_to_definition {
                        Ok(None)
                    } else {
                        Ok(Some(lsp::GotoDefinitionResponse::Scalar(lsp::Location {
                            uri: params.text_document_position_params.text_document.uri,
                            range: lsp::Range::new(
                                lsp::Position::new(4, 3),
                                lsp::Position::new(4, 6),
                            ),
                        })))
                    }
                },
            );
        let references = cx
            .lsp
            .set_request_handler::<lsp::request::References, _, _>(move |params, _| async move {
                Ok(Some(vec![lsp::Location {
                    uri: params.text_document_position.text_document.uri,
                    range: lsp::Range::new(lsp::Position::new(0, 8), lsp::Position::new(0, 11)),
                }]))
            });
        (go_to_definition, references)
    };

    cx.set_state(
        &r#"fn one() {
            let mut a = ˇtwo();
        }

        fn two() {}"#
            .unindent(),
    );
    set_up_lsp_handlers(false, &mut cx);
    let navigated = cx
        .update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to definition");
    assert_eq!(
        navigated,
        Navigated::Yes,
        "Should have navigated to definition from the GetDefinition response"
    );
    cx.assert_editor_state(
        &r#"fn one() {
            let mut a = two();
        }

        fn «twoˇ»() {}"#
            .unindent(),
    );

    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, test_editor_cx| {
        assert_eq!(
            editors.len(),
            1,
            "Initially, only one, test, editor should be open in the workspace"
        );
        assert_eq!(
            test_editor_cx.entity(),
            editors.last().expect("Asserted len is 1").clone()
        );
    });

    set_up_lsp_handlers(true, &mut cx);
    let navigated = cx
        .update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to lookup references");
    assert_eq!(
        navigated,
        Navigated::Yes,
        "Should have navigated to references as a fallback after empty GoToDefinition response"
    );
    // We should not change the selections in the existing file,
    // if opening another milti buffer with the references
    cx.assert_editor_state(
        &r#"fn one() {
            let mut a = two();
        }

        fn «twoˇ»() {}"#
            .unindent(),
    );
    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, test_editor_cx| {
        assert_eq!(
            editors.len(),
            2,
            "After falling back to references search, we open a new editor with the results"
        );
        let references_fallback_text = editors
            .into_iter()
            .find(|new_editor| *new_editor != test_editor_cx.entity())
            .expect("Should have one non-test editor now")
            .read(test_editor_cx)
            .text(test_editor_cx);
        assert_eq!(
            references_fallback_text, "fn one() {\n    let mut a = two();\n}",
            "Should use the range from the references response and not the GoToDefinition one"
        );
    });
}

#[gpui::test]
async fn test_goto_definition_no_fallback(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    cx.update(|cx| {
        let mut editor_settings = EditorSettings::get_global(cx).clone();
        editor_settings.go_to_definition_fallback = GoToDefinitionFallback::None;
        EditorSettings::override_global(editor_settings, cx);
    });
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            definition_provider: Some(lsp::OneOf::Left(true)),
            references_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;
    let original_state = r#"fn one() {
        let mut a = ˇtwo();
    }

    fn two() {}"#
        .unindent();
    cx.set_state(&original_state);

    let mut go_to_definition = cx
        .lsp
        .set_request_handler::<lsp::request::GotoDefinition, _, _>(
            move |_, _| async move { Ok(None) },
        );
    let _references = cx
        .lsp
        .set_request_handler::<lsp::request::References, _, _>(move |_, _| async move {
            panic!("Should not call for references with no go to definition fallback")
        });

    let navigated = cx
        .update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to lookup references");
    go_to_definition
        .next()
        .await
        .expect("Should have called the go_to_definition handler");

    assert_eq!(
        navigated,
        Navigated::No,
        "Should have navigated to references as a fallback after empty GoToDefinition response"
    );
    cx.assert_editor_state(&original_state);
    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, _| {
        assert_eq!(
            editors.len(),
            1,
            "After unsuccessful fallback, no other editor should have been opened"
        );
    });
}

#[gpui::test]
async fn test_goto_definition_close_ranges_open_singleton(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            definition_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    // File content: 10 lines with functions defined on lines 3, 5, and 7 (0-indexed).
    // With the default excerpt_context_lines of 2, ranges that are within
    // 2 * 2 = 4 rows of each other should be grouped into one excerpt.
    cx.set_state(
        &r#"fn caller() {
            let _ = ˇtarget();
        }
        fn target_a() {}

        fn target_b() {}

        fn target_c() {}
        "#
        .unindent(),
    );

    // Return two definitions that are close together (lines 3 and 5, gap of 2 rows)
    cx.set_request_handler::<lsp::request::GotoDefinition, _, _>(move |url, _, _| async move {
        Ok(Some(lsp::GotoDefinitionResponse::Array(vec![
            lsp::Location {
                uri: url.clone(),
                range: lsp::Range::new(lsp::Position::new(3, 3), lsp::Position::new(3, 11)),
            },
            lsp::Location {
                uri: url,
                range: lsp::Range::new(lsp::Position::new(5, 3), lsp::Position::new(5, 11)),
            },
        ])))
    });

    let navigated = cx
        .update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to definitions");
    assert_eq!(navigated, Navigated::Yes);

    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, _| {
        assert_eq!(
            editors.len(),
            1,
            "Close ranges should navigate in-place without opening a new editor"
        );
    });

    // Both target ranges should be selected
    cx.assert_editor_state(
        &r#"fn caller() {
            let _ = target();
        }
        fn «target_aˇ»() {}

        fn «target_bˇ»() {}

        fn target_c() {}
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_goto_definition_far_ranges_open_multibuffer(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            definition_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    // Create a file with definitions far apart (more than 2 * excerpt_context_lines rows).
    cx.set_state(
        &r#"fn caller() {
            let _ = ˇtarget();
        }
        fn target_a() {}















        fn target_b() {}
        "#
        .unindent(),
    );

    // Return two definitions that are far apart (lines 3 and 19, gap of 16 rows)
    cx.set_request_handler::<lsp::request::GotoDefinition, _, _>(move |url, _, _| async move {
        Ok(Some(lsp::GotoDefinitionResponse::Array(vec![
            lsp::Location {
                uri: url.clone(),
                range: lsp::Range::new(lsp::Position::new(3, 3), lsp::Position::new(3, 11)),
            },
            lsp::Location {
                uri: url,
                range: lsp::Range::new(lsp::Position::new(19, 3), lsp::Position::new(19, 11)),
            },
        ])))
    });

    let navigated = cx
        .update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to definitions");
    assert_eq!(navigated, Navigated::Yes);

    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, test_editor_cx| {
        assert_eq!(
            editors.len(),
            2,
            "Far apart ranges should open a new multibuffer editor"
        );
        let multibuffer_editor = editors
            .into_iter()
            .find(|editor| *editor != test_editor_cx.entity())
            .expect("Should have a multibuffer editor");
        let multibuffer_text = multibuffer_editor.read(test_editor_cx).text(test_editor_cx);
        assert!(
            multibuffer_text.contains("target_a"),
            "Multibuffer should contain the first definition"
        );
        assert!(
            multibuffer_text.contains("target_b"),
            "Multibuffer should contain the second definition"
        );
    });
}

#[gpui::test]
async fn test_goto_definition_contained_ranges(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            definition_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    // The LSP returns two single-line definitions on the same row where one
    // range contains the other. Both are on the same line so the
    // `fits_in_one_excerpt` check won't underflow, and the code reaches
    // `change_selections`.
    cx.set_state(
        &r#"fn caller() {
            let _ = ˇtarget();
        }
        fn target_outer() { fn target_inner() {} }
        "#
        .unindent(),
    );

    // Return two definitions on the same line: an outer range covering the
    // whole line and an inner range for just the inner function name.
    cx.set_request_handler::<lsp::request::GotoDefinition, _, _>(move |url, _, _| async move {
        Ok(Some(lsp::GotoDefinitionResponse::Array(vec![
            // Inner range: just "target_inner" (cols 23..35)
            lsp::Location {
                uri: url.clone(),
                range: lsp::Range::new(lsp::Position::new(3, 23), lsp::Position::new(3, 35)),
            },
            // Outer range: the whole line (cols 0..48)
            lsp::Location {
                uri: url,
                range: lsp::Range::new(lsp::Position::new(3, 0), lsp::Position::new(3, 48)),
            },
        ])))
    });

    let navigated = cx
        .update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to definitions");
    assert_eq!(navigated, Navigated::Yes);
}

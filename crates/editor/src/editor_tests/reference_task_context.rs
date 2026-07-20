use super::*;

#[gpui::test]
async fn test_goto_definition_preserve_scroll_strategy(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.go_to_definition_scroll_strategy = Some(GoToDefinitionScrollStrategy::Preserve);
        settings.vertical_scroll_margin = Some(0.0);
    });

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            definition_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let window = cx.window;
    let line_height = cx.update_editor(|editor, window, cx| {
        editor
            .style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });
    cx.simulate_window_resize(window, size(px(1000.), 8. * line_height));

    // Build a buffer where `target` is defined on row 10 and called from
    // row 20, with the cursor placed on the call site.
    let buffer = indoc! { "
            // 0
            // 1
            // 2
            // 3
            // 4
            // 5
            // 6
            // 7
            // 8
            // 9
            fn target() // 10
            // 11
            // 12
            // 13
            // 14
            // 15
            // 16
            // 17
            // 18
            // 19
            fn caller() { ˇtarget(); } // 20
            // 21
            // 22
            // 23
            // 24
            // 25
            // 26
            // 27
            // 28
            // 29
            // 30
        "};

    // Mock the response from the LSP server when requesting to go to a
    // definition so as to always jump to the `target` function.
    cx.set_request_handler::<lsp::request::GotoDefinition, _, _>(|url, _, _| async move {
        Ok(Some(lsp::GotoDefinitionResponse::Scalar(lsp::Location {
            uri: url.clone(),
            range: lsp::Range::new(lsp::Position::new(10, 3), lsp::Position::new(10, 9)),
        })))
    });

    let caller_row = 20.0;
    let target_row = 10.0;
    let offset = 1.5;
    let center_offset = cx.update_editor(|editor, _, _| {
        editor
            .visible_line_count()
            .map(|count| ((count - 1.0) / 2.0).floor())
            .expect("Visible line count should be available")
    });

    // When the cursor is visible inside the viewport, going to a definition
    // should preserve that same offset value.
    // In this case, with the cursor set at row 20 and the scroll position set
    // to 18.5 (20 - 1.5), when going to the definition of `target` in row 10,
    // the scroll position should end up at 8.5 (10 - 1.5), so as to preserve
    // that same offset of 1.5.
    cx.set_state(&buffer);
    cx.update_editor(|editor, window, cx| {
        editor.set_scroll_position(gpui::Point::new(0.0, caller_row - offset), window, cx);
    });
    cx.update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to definition");
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0.0, target_row - offset),
        );
    });

    // In the case where the cursor ends up outside of the visible viewport, the
    // scroll position's offset should be ignored and the center of the viewport
    // should be used instead.
    // Since the cursor is jumping to row 10, the scroll position's y coordinate
    // should end up at 10 minus the offset from the center of the viewport.
    cx.set_state(&buffer);
    cx.update_editor(|editor, window, cx| {
        editor.set_scroll_position(gpui::Point::new(0.0, 0.0), window, cx);
        let snapshot = editor.display_snapshot(cx);
        let cursor_row = editor
            .selections
            .newest_display(&snapshot)
            .start
            .row()
            .as_f64();
        let visible_lines = editor
            .visible_line_count()
            .expect("Visible line count should be available");

        assert!(cursor_row >= visible_lines, "Cursor should be offscreen");
    });

    cx.update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to definition");
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0.0, (target_row - center_offset).max(0.0)),
        );
    });
}

#[gpui::test]
async fn test_find_all_references_editor_reuse(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            references_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    cx.set_state(
        &r#"
        fn one() {
            let mut a = two();
        }

        fn ˇtwo() {}"#
            .unindent(),
    );
    cx.lsp
        .set_request_handler::<lsp::request::References, _, _>(move |params, _| async move {
            Ok(Some(vec![
                lsp::Location {
                    uri: params.text_document_position.text_document.uri.clone(),
                    range: lsp::Range::new(lsp::Position::new(0, 16), lsp::Position::new(0, 19)),
                },
                lsp::Location {
                    uri: params.text_document_position.text_document.uri,
                    range: lsp::Range::new(lsp::Position::new(4, 4), lsp::Position::new(4, 7)),
                },
            ]))
        });
    let navigated = cx
        .update_editor(|editor, window, cx| {
            editor.find_all_references(&FindAllReferences::default(), window, cx)
        })
        .unwrap()
        .await
        .expect("Failed to navigate to references");
    assert_eq!(
        navigated,
        Navigated::Yes,
        "Should have navigated to references from the FindAllReferences response"
    );
    cx.assert_editor_state(
        &r#"fn one() {
            let mut a = two();
        }

        fn ˇtwo() {}"#
            .unindent(),
    );

    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, _| {
        assert_eq!(editors.len(), 2, "We should have opened a new multibuffer");
    });

    cx.set_state(
        &r#"fn one() {
            let mut a = ˇtwo();
        }

        fn two() {}"#
            .unindent(),
    );
    let navigated = cx
        .update_editor(|editor, window, cx| {
            editor.find_all_references(&FindAllReferences::default(), window, cx)
        })
        .unwrap()
        .await
        .expect("Failed to navigate to references");
    assert_eq!(
        navigated,
        Navigated::Yes,
        "Should have navigated to references from the FindAllReferences response"
    );
    cx.assert_editor_state(
        &r#"fn one() {
            let mut a = ˇtwo();
        }

        fn two() {}"#
            .unindent(),
    );
    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, _| {
        assert_eq!(
            editors.len(),
            2,
            "should have re-used the previous multibuffer"
        );
    });

    cx.set_state(
        &r#"fn one() {
            let mut a = ˇtwo();
        }
        fn three() {}
        fn two() {}"#
            .unindent(),
    );
    cx.lsp
        .set_request_handler::<lsp::request::References, _, _>(move |params, _| async move {
            Ok(Some(vec![
                lsp::Location {
                    uri: params.text_document_position.text_document.uri.clone(),
                    range: lsp::Range::new(lsp::Position::new(0, 16), lsp::Position::new(0, 19)),
                },
                lsp::Location {
                    uri: params.text_document_position.text_document.uri,
                    range: lsp::Range::new(lsp::Position::new(5, 4), lsp::Position::new(5, 7)),
                },
            ]))
        });
    let navigated = cx
        .update_editor(|editor, window, cx| {
            editor.find_all_references(&FindAllReferences::default(), window, cx)
        })
        .unwrap()
        .await
        .expect("Failed to navigate to references");
    assert_eq!(
        navigated,
        Navigated::Yes,
        "Should have navigated to references from the FindAllReferences response"
    );
    cx.assert_editor_state(
        &r#"fn one() {
                let mut a = ˇtwo();
            }
            fn three() {}
            fn two() {}"#
            .unindent(),
    );
    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, _| {
        assert_eq!(
            editors.len(),
            3,
            "should have used a new multibuffer as offsets changed"
        );
    });
}
#[gpui::test]
async fn test_find_enclosing_node_with_task(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let text = r#"
        #[cfg(test)]
        mod tests() {
            #[test]
            fn runnable_1() {
                let a = 1;
            }

            #[test]
            fn runnable_2() {
                let a = 1;
                let b = 2;
            }
        }
    "#
    .unindent();

    let fs = FakeFs::new(cx.executor());
    fs.insert_file("/file.rs", Default::default()).await;

    let project = Project::test(fs, ["/a".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    let editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });

    editor.update_in(cx, |editor, window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        editor.runnables.insert(
            buffer.read(cx).remote_id(),
            3,
            buffer.read(cx).version(),
            RunnableTasks {
                templates: Vec::new(),
                offset: snapshot.anchor_before(MultiBufferOffset(43)),
                column: 0,
                extra_variables: HashMap::default(),
                context_range: BufferOffset(43)..BufferOffset(85),
            },
        );
        editor.runnables.insert(
            buffer.read(cx).remote_id(),
            8,
            buffer.read(cx).version(),
            RunnableTasks {
                templates: Vec::new(),
                offset: snapshot.anchor_before(MultiBufferOffset(86)),
                column: 0,
                extra_variables: HashMap::default(),
                context_range: BufferOffset(86)..BufferOffset(191),
            },
        );

        // Test finding task when cursor is inside function body
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(4, 5)..Point::new(4, 5)])
        });
        let (_, row, _) = editor.find_enclosing_node_task(cx).unwrap();
        assert_eq!(row, 3, "Should find task for cursor inside runnable_1");

        // Test finding task when cursor is on function name
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(8, 4)..Point::new(8, 4)])
        });
        let (_, row, _) = editor.find_enclosing_node_task(cx).unwrap();
        assert_eq!(row, 8, "Should find task when cursor is on function name");
    });
}

#[gpui::test]
async fn test_toggle_code_actions_build_tasks_context_error_notifies(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    struct FailingContextProvider;
    impl ContextProvider for FailingContextProvider {
        fn build_context(
            &self,
            _: &TaskVariables,
            _: ContextLocation<'_>,
            _: Option<HashMap<String, String>>,
            _: Arc<dyn LanguageToolchainStore>,
            _: &mut gpui::App,
        ) -> Task<anyhow::Result<TaskVariables>> {
            Task::ready(Err(anyhow::anyhow!("Task context provider failed")))
        }
    }

    let language = Arc::new(
        Arc::try_unwrap(rust_lang())
            .unwrap()
            .with_context_provider(Some(Arc::new(FailingContextProvider))),
    );

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/a"), json!({ "main.rs": "fn main() {}" }))
        .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(language.clone());

    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let mut cx = VisualTestContext::from_window(*window, cx);
    let workspace = window
        .read_with(&mut cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let worktree_id = workspace.update_in(&mut cx, |workspace, _, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let editor = workspace
        .update_in(&mut cx, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    editor.update_in(&mut cx, |editor, window, cx| {
        let buffer = editor.buffer().read(cx).as_singleton().unwrap();
        buffer.update(cx, |buffer, cx| {
            buffer.set_language(Some(language.clone()), cx)
        });

        let snapshot = editor.buffer().read(cx).snapshot(cx);
        editor.runnables.insert(
            buffer.read(cx).remote_id(),
            0,
            buffer.read(cx).version(),
            RunnableTasks {
                templates: Vec::new(),
                offset: snapshot.anchor_before(MultiBufferOffset(0)),
                column: 0,
                extra_variables: HashMap::default(),
                context_range: BufferOffset(0)..BufferOffset(0),
            },
        );
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(0, 0)])
        });

        editor.toggle_code_actions(
            &ToggleCodeActions {
                deployed_from: None,
                quick_launch: false,
            },
            window,
            cx,
        );
    });

    cx.run_until_parked();

    workspace.update_in(&mut cx, |workspace, _, _| {
        assert!(!workspace.notification_ids().is_empty());
    });
}

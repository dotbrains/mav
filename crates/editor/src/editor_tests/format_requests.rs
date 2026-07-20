use super::*;

#[gpui::test]
async fn test_formatter_failure_does_not_abort_subsequent_formatters(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.formatter = Some(FormatterList::Vec(vec![
            Formatter::LanguageServer(settings::LanguageServerFormatterSpecifier::Current),
            Formatter::CodeAction("organize-imports".into()),
        ]))
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.rs"), "fn main() {}\n".into())
        .await;

    let project = Project::test(fs, [path!("/").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                document_formatting_provider: Some(lsp::OneOf::Left(true)),
                code_action_provider: Some(lsp::CodeActionProviderCapability::Simple(true)),
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

    let fake_server = fake_servers.next().await.unwrap();

    // Formatter #1 (LanguageServer) returns an error to simulate failure
    fake_server.set_request_handler::<lsp::request::Formatting, _, _>(
        move |_params, _| async move { Err(anyhow::anyhow!("Simulated formatter failure")) },
    );

    // Formatter #2 (CodeAction) returns a successful edit
    fake_server.set_request_handler::<lsp::request::CodeActionRequest, _, _>(
        move |_params, _| async move {
            let uri = lsp::Uri::from_file_path(path!("/file.rs")).unwrap();
            Ok(Some(vec![lsp::CodeActionOrCommand::CodeAction(
                lsp::CodeAction {
                    kind: Some("organize-imports".into()),
                    edit: Some(lsp::WorkspaceEdit::new(
                        [(
                            uri,
                            vec![lsp::TextEdit::new(
                                lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 0)),
                                "use std::io;\n".to_string(),
                            )],
                        )]
                        .into_iter()
                        .collect(),
                    )),
                    ..Default::default()
                },
            )]))
        },
    );

    fake_server.set_request_handler::<lsp::request::CodeActionResolveRequest, _, _>({
        move |params, _| async move { Ok(params) }
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

    // Formatter #1 (LanguageServer) failed, but formatter #2 (CodeAction) should have applied
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.text(cx), "use std::io;\nfn main() {}\n");
    });

    // The entire format operation should undo as one transaction
    editor.update_in(cx, |editor, window, cx| {
        editor.undo(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "fn main() {}\n");
    });
}

#[gpui::test]
async fn test_concurrent_format_requests(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            document_formatting_provider: Some(lsp::OneOf::Left(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"
        one.twoˇ
    "});

    // The format request takes a long time. When it completes, it inserts
    // a newline and an indent before the `.`
    cx.lsp
        .set_request_handler::<lsp::request::Formatting, _, _>(move |_, cx| {
            let executor = cx.background_executor().clone();
            async move {
                executor.timer(Duration::from_millis(100)).await;
                Ok(Some(vec![lsp::TextEdit {
                    range: lsp::Range::new(lsp::Position::new(0, 3), lsp::Position::new(0, 3)),
                    new_text: "\n    ".into(),
                }]))
            }
        });

    // Submit a format request.
    let format_1 = cx
        .update_editor(|editor, window, cx| editor.format(&Format, window, cx))
        .unwrap();
    cx.executor().run_until_parked();

    // Submit a second format request.
    let format_2 = cx
        .update_editor(|editor, window, cx| editor.format(&Format, window, cx))
        .unwrap();
    cx.executor().run_until_parked();

    // Wait for both format requests to complete
    cx.executor().advance_clock(Duration::from_millis(200));
    format_1.await.unwrap();
    format_2.await.unwrap();

    // The formatting edits only happens once.
    cx.assert_editor_state(indoc! {"
        one
            .twoˇ
    "});
}

#[gpui::test]
async fn test_strip_whitespace_and_format_via_lsp(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.formatter = Some(FormatterList::default())
    });

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            document_formatting_provider: Some(lsp::OneOf::Left(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    // Record which buffer changes have been sent to the language server
    let buffer_changes = Arc::new(Mutex::new(Vec::new()));
    cx.lsp
        .handle_notification::<lsp::notification::DidChangeTextDocument, _>({
            let buffer_changes = buffer_changes.clone();
            move |params, _| {
                buffer_changes.lock().extend(
                    params
                        .content_changes
                        .into_iter()
                        .map(|e| (e.range.unwrap(), e.text)),
                );
            }
        });
    // Handle formatting requests to the language server.
    cx.lsp
        .set_request_handler::<lsp::request::Formatting, _, _>({
            move |_, _| {
                // Insert blank lines between each line of the buffer.
                async move {
                    // TODO: this assertion is not reliably true. Currently nothing guarantees that we deliver
                    // DidChangedTextDocument to the LSP before sending the formatting request.
                    // assert_eq!(
                    //     &buffer_changes.lock()[1..],
                    //     &[
                    //         (
                    //             lsp::Range::new(lsp::Position::new(0, 3), lsp::Position::new(0, 4)),
                    //             "".into()
                    //         ),
                    //         (
                    //             lsp::Range::new(lsp::Position::new(2, 5), lsp::Position::new(2, 6)),
                    //             "".into()
                    //         ),
                    //         (
                    //             lsp::Range::new(lsp::Position::new(3, 4), lsp::Position::new(3, 4)),
                    //             "\n".into()
                    //         ),
                    //     ]
                    // );

                    Ok(Some(vec![
                        lsp::TextEdit {
                            range: lsp::Range::new(
                                lsp::Position::new(1, 0),
                                lsp::Position::new(1, 0),
                            ),
                            new_text: "\n".into(),
                        },
                        lsp::TextEdit {
                            range: lsp::Range::new(
                                lsp::Position::new(2, 0),
                                lsp::Position::new(2, 0),
                            ),
                            new_text: "\n".into(),
                        },
                    ]))
                }
            }
        });

    // Set up a buffer white some trailing whitespace and no trailing newline.
    cx.set_state(
        &[
            "one ",   //
            "twoˇ",   //
            "three ", //
            "four",   //
        ]
        .join("\n"),
    );

    // Submit a format request.
    let format = cx
        .update_editor(|editor, window, cx| editor.format(&Format, window, cx))
        .unwrap();

    cx.run_until_parked();
    // After formatting the buffer, the trailing whitespace is stripped,
    // a newline is appended, and the edits provided by the language server
    // have been applied.
    format.await.unwrap();

    cx.assert_editor_state(
        &[
            "one",   //
            "",      //
            "twoˇ",  //
            "",      //
            "three", //
            "four",  //
            "",      //
        ]
        .join("\n"),
    );

    // Undoing the formatting undoes the trailing whitespace removal, the
    // trailing newline, and the LSP edits.
    cx.update_buffer(|buffer, cx| buffer.undo(cx));
    cx.assert_editor_state(
        &[
            "one ",   //
            "twoˇ",   //
            "three ", //
            "four",   //
        ]
        .join("\n"),
    );
}

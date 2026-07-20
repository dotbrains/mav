use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_lsp_rename_notifications(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "const ONE: usize = 1;",
            "two": {
                "two.rs": "const TWO: usize = one::ONE + one::ONE;"
            }

        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let watched_paths = lsp::FileOperationRegistrationOptions {
        filters: vec![
            FileOperationFilter {
                scheme: Some("file".to_owned()),
                pattern: lsp::FileOperationPattern {
                    glob: "**/*.rs".to_owned(),
                    matches: Some(lsp::FileOperationPatternKind::File),
                    options: None,
                },
            },
            FileOperationFilter {
                scheme: Some("file".to_owned()),
                pattern: lsp::FileOperationPattern {
                    glob: "**/**".to_owned(),
                    matches: Some(lsp::FileOperationPatternKind::Folder),
                    options: None,
                },
            },
        ],
    };
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                workspace: Some(lsp::WorkspaceServerCapabilities {
                    workspace_folders: None,
                    file_operations: Some(lsp::WorkspaceFileOperationsServerCapabilities {
                        did_rename: Some(watched_paths.clone()),
                        will_rename: Some(watched_paths),
                        ..Default::default()
                    }),
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let _ = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/one.rs"), cx)
        })
        .await
        .unwrap();

    let fake_server = fake_servers.next().await.unwrap();
    cx.executor().run_until_parked();
    let response = project.update(cx, |project, cx| {
        let worktree = project.worktrees(cx).next().unwrap();
        let entry = worktree
            .read(cx)
            .entry_for_path(rel_path("one.rs"))
            .unwrap();
        project.rename_entry(
            entry.id,
            (worktree.read(cx).id(), rel_path("three.rs")).into(),
            cx,
        )
    });
    let expected_edit = lsp::WorkspaceEdit {
        changes: None,
        document_changes: Some(DocumentChanges::Edits({
            vec![TextDocumentEdit {
                edits: vec![lsp::Edit::Plain(lsp::TextEdit {
                    range: lsp::Range {
                        start: lsp::Position {
                            line: 0,
                            character: 1,
                        },
                        end: lsp::Position {
                            line: 0,
                            character: 3,
                        },
                    },
                    new_text: "This is not a drill".to_owned(),
                })],
                text_document: lsp::OptionalVersionedTextDocumentIdentifier {
                    uri: Uri::from_str(uri!("file:///dir/two/two.rs")).unwrap(),
                    version: Some(1337),
                },
            }]
        })),
        change_annotations: None,
    };
    let resolved_workspace_edit = Arc::new(OnceLock::new());
    fake_server
        .set_request_handler::<WillRenameFiles, _, _>({
            let resolved_workspace_edit = resolved_workspace_edit.clone();
            let expected_edit = expected_edit.clone();
            move |params, _| {
                let resolved_workspace_edit = resolved_workspace_edit.clone();
                let expected_edit = expected_edit.clone();
                async move {
                    assert_eq!(params.files.len(), 1);
                    assert_eq!(params.files[0].old_uri, uri!("file:///dir/one.rs"));
                    assert_eq!(params.files[0].new_uri, uri!("file:///dir/three.rs"));
                    resolved_workspace_edit.set(expected_edit.clone()).unwrap();
                    Ok(Some(expected_edit))
                }
            }
        })
        .next()
        .await
        .unwrap();
    let _ = response.await.unwrap();
    fake_server
        .handle_notification::<DidRenameFiles, _>(|params, _| {
            assert_eq!(params.files.len(), 1);
            assert_eq!(params.files[0].old_uri, uri!("file:///dir/one.rs"));
            assert_eq!(params.files[0].new_uri, uri!("file:///dir/three.rs"));
        })
        .next()
        .await
        .unwrap();
    assert_eq!(resolved_workspace_edit.get(), Some(&expected_edit));
}

#[gpui::test]
async fn test_rename(cx: &mut gpui::TestAppContext) {
    // hi
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "const ONE: usize = 1;",
            "two.rs": "const TWO: usize = one::ONE + one::ONE;"
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                rename_provider: Some(lsp::OneOf::Right(lsp::RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let (buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/one.rs"), cx)
        })
        .await
        .unwrap();

    let fake_server = fake_servers.next().await.unwrap();
    cx.executor().run_until_parked();

    let response = project.update(cx, |project, cx| {
        project.prepare_rename(buffer.clone(), 7, cx)
    });
    fake_server
        .set_request_handler::<lsp::request::PrepareRenameRequest, _, _>(|params, _| async move {
            assert_eq!(
                params.text_document.uri.as_str(),
                uri!("file:///dir/one.rs")
            );
            assert_eq!(params.position, lsp::Position::new(0, 7));
            Ok(Some(lsp::PrepareRenameResponse::Range(lsp::Range::new(
                lsp::Position::new(0, 6),
                lsp::Position::new(0, 9),
            ))))
        })
        .next()
        .await
        .unwrap();
    let response = response.await.unwrap();
    let PrepareRenameResponse::Success(range) = response else {
        panic!("{:?}", response);
    };
    let range = buffer.update(cx, |buffer, _| range.to_offset(buffer));
    assert_eq!(range, 6..9);

    let response = project.update(cx, |project, cx| {
        project.perform_rename(buffer.clone(), 7, "THREE".to_string(), cx)
    });
    fake_server
        .set_request_handler::<lsp::request::Rename, _, _>(|params, _| async move {
            assert_eq!(
                params.text_document_position.text_document.uri.as_str(),
                uri!("file:///dir/one.rs")
            );
            assert_eq!(
                params.text_document_position.position,
                lsp::Position::new(0, 7)
            );
            assert_eq!(params.new_name, "THREE");
            Ok(Some(lsp::WorkspaceEdit {
                changes: Some(
                    [
                        (
                            lsp::Uri::from_file_path(path!("/dir/one.rs")).unwrap(),
                            vec![lsp::TextEdit::new(
                                lsp::Range::new(lsp::Position::new(0, 6), lsp::Position::new(0, 9)),
                                "THREE".to_string(),
                            )],
                        ),
                        (
                            lsp::Uri::from_file_path(path!("/dir/two.rs")).unwrap(),
                            vec![
                                lsp::TextEdit::new(
                                    lsp::Range::new(
                                        lsp::Position::new(0, 24),
                                        lsp::Position::new(0, 27),
                                    ),
                                    "THREE".to_string(),
                                ),
                                lsp::TextEdit::new(
                                    lsp::Range::new(
                                        lsp::Position::new(0, 35),
                                        lsp::Position::new(0, 38),
                                    ),
                                    "THREE".to_string(),
                                ),
                            ],
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..Default::default()
            }))
        })
        .next()
        .await
        .unwrap();
    let mut transaction = response.await.unwrap().0;
    assert_eq!(transaction.len(), 2);
    assert_eq!(
        transaction
            .remove_entry(&buffer)
            .unwrap()
            .0
            .update(cx, |buffer, _| buffer.text()),
        "const THREE: usize = 1;"
    );
    assert_eq!(
        transaction
            .into_keys()
            .next()
            .unwrap()
            .update(cx, |buffer, _| buffer.text()),
        "const TWO: usize = one::THREE + one::THREE;"
    );
}

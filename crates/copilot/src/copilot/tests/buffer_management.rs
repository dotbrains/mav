#[gpui::test(iterations = 10)]
async fn test_buffer_management(cx: &mut TestAppContext) {
    init_test(cx);
    let (copilot, mut lsp) = Copilot::fake(cx);

    let buffer_1 = cx.new(|cx| Buffer::local("Hello", cx));
    let buffer_1_uri: lsp::Uri = format!("buffer://{}", buffer_1.entity_id().as_u64())
        .parse()
        .unwrap();
    copilot.update(cx, |copilot, cx| copilot.register_buffer(&buffer_1, cx));
    assert_eq!(
        lsp.receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await,
        lsp::DidOpenTextDocumentParams {
            text_document: lsp::TextDocumentItem::new(
                buffer_1_uri.clone(),
                "plaintext".into(),
                0,
                "Hello".into()
            ),
        }
    );

    let buffer_2 = cx.new(|cx| Buffer::local("Goodbye", cx));
    let buffer_2_uri: lsp::Uri = format!("buffer://{}", buffer_2.entity_id().as_u64())
        .parse()
        .unwrap();
    copilot.update(cx, |copilot, cx| copilot.register_buffer(&buffer_2, cx));
    assert_eq!(
        lsp.receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await,
        lsp::DidOpenTextDocumentParams {
            text_document: lsp::TextDocumentItem::new(
                buffer_2_uri.clone(),
                "plaintext".into(),
                0,
                "Goodbye".into()
            ),
        }
    );

    buffer_1.update(cx, |buffer, cx| buffer.edit([(5..5, " world")], None, cx));
    assert_eq!(
        lsp.receive_notification::<lsp::notification::DidChangeTextDocument>()
            .await,
        lsp::DidChangeTextDocumentParams {
            text_document: lsp::VersionedTextDocumentIdentifier::new(buffer_1_uri.clone(), 1),
            content_changes: vec![lsp::TextDocumentContentChangeEvent {
                range: Some(lsp::Range::new(
                    lsp::Position::new(0, 5),
                    lsp::Position::new(0, 5)
                )),
                range_length: None,
                text: " world".into(),
            }],
        }
    );

    // Ensure updates to the file are reflected in the LSP.
    buffer_1.update(cx, |buffer, cx| {
        buffer.file_updated(
            Arc::new(File {
                abs_path: path!("/root/child/buffer-1").into(),
                path: rel_path("child/buffer-1").into(),
            }),
            cx,
        )
    });
    assert_eq!(
        lsp.receive_notification::<lsp::notification::DidCloseTextDocument>()
            .await,
        lsp::DidCloseTextDocumentParams {
            text_document: lsp::TextDocumentIdentifier::new(buffer_1_uri),
        }
    );
    let buffer_1_uri = lsp::Uri::from_file_path(path!("/root/child/buffer-1")).unwrap();
    assert_eq!(
        lsp.receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await,
        lsp::DidOpenTextDocumentParams {
            text_document: lsp::TextDocumentItem::new(
                buffer_1_uri.clone(),
                "plaintext".into(),
                1,
                "Hello world".into()
            ),
        }
    );

    // Ensure all previously-registered buffers are closed when signing out.
    lsp.set_request_handler::<request::SignOut, _, _>(|_, _| async {
        Ok(request::SignOutResult {})
    });
    copilot
        .update(cx, |copilot, cx| copilot.sign_out(cx))
        .await
        .unwrap();
    let mut received_close_notifications = vec![
        lsp.receive_notification::<lsp::notification::DidCloseTextDocument>()
            .await,
        lsp.receive_notification::<lsp::notification::DidCloseTextDocument>()
            .await,
    ];
    received_close_notifications.sort_by_key(|notification| notification.text_document.uri.clone());
    assert_eq!(
        received_close_notifications,
        vec![
            lsp::DidCloseTextDocumentParams {
                text_document: lsp::TextDocumentIdentifier::new(buffer_2_uri.clone()),
            },
            lsp::DidCloseTextDocumentParams {
                text_document: lsp::TextDocumentIdentifier::new(buffer_1_uri.clone()),
            },
        ],
    );

    // Ensure all previously-registered buffers are re-opened when signing in.
    lsp.set_request_handler::<request::SignIn, _, _>(|_, _| async {
        Ok(request::PromptUserDeviceFlow {
            user_code: "test-code".into(),
            command: lsp::Command {
                title: "Sign in".into(),
                command: "github.copilot.finishDeviceFlow".into(),
                arguments: None,
            },
        })
    });
    copilot
        .update(cx, |copilot, cx| copilot.sign_in(cx))
        .await
        .unwrap();

    // Simulate auth completion by directly updating sign-in status
    copilot.update(cx, |copilot, cx| {
        copilot.update_sign_in_status(
            request::SignInStatus::Ok {
                user: Some("user-1".into()),
            },
            cx,
        );
    });

    let mut received_open_notifications = vec![
        lsp.receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await,
        lsp.receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await,
    ];
    received_open_notifications.sort_by_key(|notification| notification.text_document.uri.clone());
    assert_eq!(
        received_open_notifications,
        vec![
            lsp::DidOpenTextDocumentParams {
                text_document: lsp::TextDocumentItem::new(
                    buffer_2_uri.clone(),
                    "plaintext".into(),
                    0,
                    "Goodbye".into()
                ),
            },
            lsp::DidOpenTextDocumentParams {
                text_document: lsp::TextDocumentItem::new(
                    buffer_1_uri.clone(),
                    "plaintext".into(),
                    0,
                    "Hello world".into()
                ),
            }
        ]
    );
    // Dropping a buffer causes it to be closed on the LSP side as well.
    cx.update(|_| drop(buffer_2));
    assert_eq!(
        lsp.receive_notification::<lsp::notification::DidCloseTextDocument>()
            .await,
        lsp::DidCloseTextDocumentParams {
            text_document: lsp::TextDocumentIdentifier::new(buffer_2_uri),
        }
    );
}

struct File {
    abs_path: PathBuf,
    path: Arc<RelPath>,
}

impl language::File for File {
    fn as_local(&self) -> Option<&dyn language::LocalFile> {
        Some(self)
    }

    fn disk_state(&self) -> language::DiskState {
        language::DiskState::Present {
            mtime: ::fs::MTime::from_seconds_and_nanos(100, 42),
            size: 0,
        }
    }

    fn path(&self) -> &Arc<RelPath> {
        &self.path
    }

    fn path_style(&self, _: &App) -> PathStyle {
        PathStyle::local()
    }

    fn full_path(&self, _: &App) -> PathBuf {
        unimplemented!()
    }

    fn file_name<'a>(&'a self, _: &'a App) -> &'a str {
        unimplemented!()
    }

    fn to_proto(&self, _: &App) -> rpc::proto::File {
        unimplemented!()
    }

    fn worktree_id(&self, _: &App) -> settings::WorktreeId {
        settings::WorktreeId::from_usize(0)
    }

    fn is_private(&self) -> bool {
        false
    }
}

impl language::LocalFile for File {
    fn abs_path(&self, _: &App) -> PathBuf {
        self.abs_path.clone()
    }

    fn load(&self, _: &App) -> Task<Result<String>> {
        unimplemented!()
    }

    fn load_bytes(&self, _cx: &App) -> Task<Result<Vec<u8>>> {
        unimplemented!()
    }
}

use super::*;

pub(super) async fn on_client_added(client: &Rc<TestClient>, _: &mut TestAppContext) {
    client.language_registry().add(Arc::new(Language::new(
        LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        None,
    )));
    client.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "the-fake-language-server",
            capabilities: lsp::LanguageServer::full_capabilities(),
            initializer: Some(Box::new({
                let fs = client.app_state.fs.clone();
                move |fake_server: &mut FakeLanguageServer| {
                    fake_server.set_request_handler::<lsp::request::Completion, _, _>(
                        |_, _| async move {
                            Ok(Some(lsp::CompletionResponse::Array(vec![
                                lsp::CompletionItem {
                                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                                        range: lsp::Range::new(
                                            lsp::Position::new(0, 0),
                                            lsp::Position::new(0, 0),
                                        ),
                                        new_text: "the-new-text".to_string(),
                                    })),
                                    ..Default::default()
                                },
                            ])))
                        },
                    );

                    fake_server.set_request_handler::<lsp::request::CodeActionRequest, _, _>(
                        |_, _| async move {
                            Ok(Some(vec![lsp::CodeActionOrCommand::CodeAction(
                                lsp::CodeAction {
                                    title: "the-code-action".to_string(),
                                    ..Default::default()
                                },
                            )]))
                        },
                    );

                    fake_server.set_request_handler::<lsp::request::PrepareRenameRequest, _, _>(
                        |params, _| async move {
                            Ok(Some(lsp::PrepareRenameResponse::Range(lsp::Range::new(
                                params.position,
                                params.position,
                            ))))
                        },
                    );

                    fake_server.set_request_handler::<lsp::request::GotoDefinition, _, _>({
                        let fs = fs.clone();
                        move |_, cx| {
                            let background = cx.background_executor();
                            let rng = background.rng();
                            let mut rng = rng.lock();
                            let count = rng.random_range::<usize, _>(1..3);
                            let files = fs.as_fake().files();
                            let files = (0..count)
                                .map(|_| files.choose(&mut rng).unwrap().clone())
                                .collect::<Vec<_>>();
                            async move {
                                log::info!("LSP: Returning definitions in files {:?}", &files);
                                Ok(Some(lsp::GotoDefinitionResponse::Array(
                                    files
                                        .into_iter()
                                        .map(|file| lsp::Location {
                                            uri: lsp::Uri::from_file_path(file).unwrap(),
                                            range: Default::default(),
                                        })
                                        .collect(),
                                )))
                            }
                        }
                    });

                    fake_server
                        .set_request_handler::<lsp::request::DocumentHighlightRequest, _, _>(
                            move |_, cx| {
                                let mut highlights = Vec::new();
                                let background = cx.background_executor();
                                let rng = background.rng();
                                let mut rng = rng.lock();

                                let highlight_count = rng.random_range(1..=5);
                                for _ in 0..highlight_count {
                                    let start_row = rng.random_range(0..100);
                                    let start_column = rng.random_range(0..100);
                                    let end_row = rng.random_range(0..100);
                                    let end_column = rng.random_range(0..100);
                                    let start = PointUtf16::new(start_row, start_column);
                                    let end = PointUtf16::new(end_row, end_column);
                                    let range = if start > end { end..start } else { start..end };
                                    highlights.push(lsp::DocumentHighlight {
                                        range: range_to_lsp(range.clone()).unwrap(),
                                        kind: Some(lsp::DocumentHighlightKind::READ),
                                    });
                                }
                                highlights.sort_unstable_by_key(|highlight| {
                                    (highlight.range.start, highlight.range.end)
                                });
                                async move { Ok(Some(highlights)) }
                            },
                        );
                }
            })),
            ..Default::default()
        },
    );
}

pub(super) async fn on_quiesce(
    _: &mut TestServer,
    clients: &mut [(Rc<TestClient>, TestAppContext)],
) {
    for (client, client_cx) in clients.iter() {
        for guest_project in client.dev_server_projects().iter() {
            guest_project.read_with(client_cx, |guest_project, cx| {
                    let host_project = clients.iter().find_map(|(client, cx)| {
                        let project = client
                            .local_projects()
                            .iter()
                            .find(|host_project| {
                                host_project.read_with(cx, |host_project, _| {
                                    host_project.remote_id() == guest_project.remote_id()
                                })
                            })?
                            .clone();
                        Some((project, cx))
                    });

                    if !guest_project.is_disconnected(cx)
                        && let Some((host_project, host_cx)) = host_project {
                            let host_worktree_snapshots =
                                host_project.read_with(host_cx, |host_project, cx| {
                                    host_project
                                        .worktrees(cx)
                                        .map(|worktree| {
                                            let worktree = worktree.read(cx);
                                            (worktree.id(), worktree.snapshot())
                                        })
                                        .collect::<BTreeMap<_, _>>()
                                });
                            let guest_worktree_snapshots = guest_project
                                .worktrees(cx)
                                .map(|worktree| {
                                    let worktree = worktree.read(cx);
                                    (worktree.id(), worktree.snapshot())
                                })
                                .collect::<BTreeMap<_, _>>();
                            let host_repository_snapshots = host_project.read_with(host_cx, |host_project, cx| {
                                host_project.git_store().read(cx).repo_snapshots(cx)
                            });
                            let guest_repository_snapshots = guest_project.git_store().read(cx).repo_snapshots(cx);

                            assert_eq!(
                                guest_worktree_snapshots.values().map(|w| w.abs_path()).collect::<Vec<_>>(),
                                host_worktree_snapshots.values().map(|w| w.abs_path()).collect::<Vec<_>>(),
                                "{} has different worktrees than the host for project {:?}",
                                client.username, guest_project.remote_id(),
                            );

                            assert_eq!(
                                guest_repository_snapshots.values().collect::<Vec<_>>(),
                                host_repository_snapshots.values().collect::<Vec<_>>(),
                                "{} has different repositories than the host for project {:?}",
                                client.username, guest_project.remote_id(),
                            );

                            for (id, host_snapshot) in &host_worktree_snapshots {
                                let guest_snapshot = &guest_worktree_snapshots[id];
                                assert_eq!(
                                    guest_snapshot.root_name(),
                                    host_snapshot.root_name(),
                                    "{} has different root name than the host for worktree {}, project {:?}",
                                    client.username,
                                    id,
                                    guest_project.remote_id(),
                                );
                                assert_eq!(
                                    guest_snapshot.abs_path(),
                                    host_snapshot.abs_path(),
                                    "{} has different abs path than the host for worktree {}, project: {:?}",
                                    client.username,
                                    id,
                                    guest_project.remote_id(),
                                );
                                assert_eq!(
                                    guest_snapshot.entries(false, 0).map(null_out_entry_size).collect::<Vec<_>>(),
                                    host_snapshot.entries(false, 0).map(null_out_entry_size).collect::<Vec<_>>(),
                                    "{} has different snapshot than the host for worktree {:?} ({:?}) and project {:?}",
                                    client.username,
                                    host_snapshot.abs_path(),
                                    id,
                                    guest_project.remote_id(),
                                );
                                assert_eq!(guest_snapshot.scan_id(), host_snapshot.scan_id(),
                                    "{} has different scan id than the host for worktree {:?} and project {:?}",
                                    client.username,
                                    host_snapshot.abs_path(),
                                    guest_project.remote_id(),
                                );
                            }
                        }

                    for buffer in guest_project.opened_buffers(cx) {
                        let buffer = buffer.read(cx);
                        assert_eq!(
                            buffer.deferred_ops_len(),
                            0,
                            "{} has deferred operations for buffer {:?} in project {:?}",
                            client.username,
                            buffer.file().unwrap().full_path(cx),
                            guest_project.remote_id(),
                        );
                    }
                });

            // A hack to work around a hack in
            // https://github.com/mav-industries/mav/pull/16696 that wasn't
            // detected until we upgraded the rng crate. This whole crate is
            // going away with DeltaDB soon, so we hold our nose and
            // continue.
            fn null_out_entry_size(entry: &project::Entry) -> project::Entry {
                project::Entry {
                    size: 0,
                    ..entry.clone()
                }
            }
        }

        let buffers = client.buffers().clone();
        for (guest_project, guest_buffers) in &buffers {
            let project_id = if guest_project.read_with(client_cx, |project, cx| {
                project.is_local() || project.is_disconnected(cx)
            }) {
                continue;
            } else {
                guest_project
                    .read_with(client_cx, |project, _| project.remote_id())
                    .unwrap()
            };
            let guest_user_id = client.user_id().unwrap();

            let host_project = clients.iter().find_map(|(client, cx)| {
                let project = client
                    .local_projects()
                    .iter()
                    .find(|host_project| {
                        host_project.read_with(cx, |host_project, _| {
                            host_project.remote_id() == Some(project_id)
                        })
                    })?
                    .clone();
                Some((client.user_id().unwrap(), project, cx))
            });

            let (host_user_id, host_project, host_cx) =
                if let Some((host_user_id, host_project, host_cx)) = host_project {
                    (host_user_id, host_project, host_cx)
                } else {
                    continue;
                };

            for guest_buffer in guest_buffers {
                let buffer_id = guest_buffer.read_with(client_cx, |buffer, _| buffer.remote_id());
                let host_buffer = host_project.read_with(host_cx, |project, cx| {
                    project.buffer_for_id(buffer_id, cx).unwrap_or_else(|| {
                        panic!(
                            "host does not have buffer for guest:{}, peer:{:?}, id:{}",
                            client.username,
                            client.peer_id(),
                            buffer_id
                        )
                    })
                });
                let path = host_buffer
                    .read_with(host_cx, |buffer, cx| buffer.file().unwrap().full_path(cx));

                assert_eq!(
                    guest_buffer.read_with(client_cx, |buffer, _| buffer.deferred_ops_len()),
                    0,
                    "{}, buffer {}, path {:?} has deferred operations",
                    client.username,
                    buffer_id,
                    path,
                );
                assert_eq!(
                    guest_buffer.read_with(client_cx, |buffer, _| buffer.text()),
                    host_buffer.read_with(host_cx, |buffer, _| buffer.text()),
                    "{}, buffer {}, path {:?}, differs from the host's buffer",
                    client.username,
                    buffer_id,
                    path
                );

                let host_file = host_buffer.read_with(host_cx, |b, _| b.file().cloned());
                let guest_file = guest_buffer.read_with(client_cx, |b, _| b.file().cloned());
                match (host_file, guest_file) {
                    (Some(host_file), Some(guest_file)) => {
                        assert_eq!(guest_file.path(), host_file.path());
                        assert_eq!(
                            guest_file.disk_state(),
                            host_file.disk_state(),
                            "guest {} disk_state does not match host {} for path {:?} in project {}",
                            guest_user_id,
                            host_user_id,
                            guest_file.path(),
                            project_id,
                        );
                    }
                    (None, None) => {}
                    (None, _) => panic!("host's file is None, guest's isn't"),
                    (_, None) => panic!("guest's file is None, hosts's isn't"),
                }

                let host_diff_base = host_project.read_with(host_cx, |project, cx| {
                    project
                        .git_store()
                        .read(cx)
                        .get_unstaged_diff(host_buffer.read(cx).remote_id(), cx)
                        .unwrap()
                        .read(cx)
                        .base_text_string(cx)
                });
                let guest_diff_base = guest_project.read_with(client_cx, |project, cx| {
                    project
                        .git_store()
                        .read(cx)
                        .get_unstaged_diff(guest_buffer.read(cx).remote_id(), cx)
                        .unwrap()
                        .read(cx)
                        .base_text_string(cx)
                });
                assert_eq!(
                    guest_diff_base, host_diff_base,
                    "guest {} diff base does not match host's for path {path:?} in project {project_id}",
                    client.username
                );

                let host_saved_version =
                    host_buffer.read_with(host_cx, |b, _| b.saved_version().clone());
                let guest_saved_version =
                    guest_buffer.read_with(client_cx, |b, _| b.saved_version().clone());
                assert_eq!(
                    guest_saved_version, host_saved_version,
                    "guest {} saved version does not match host's for path {path:?} in project {project_id}",
                    client.username
                );

                let host_is_dirty = host_buffer.read_with(host_cx, |b, _| b.is_dirty());
                let guest_is_dirty = guest_buffer.read_with(client_cx, |b, _| b.is_dirty());
                assert_eq!(
                    guest_is_dirty, host_is_dirty,
                    "guest {} dirty state does not match host's for path {path:?} in project {project_id}",
                    client.username
                );

                let host_saved_mtime = host_buffer.read_with(host_cx, |b, _| b.saved_mtime());
                let guest_saved_mtime = guest_buffer.read_with(client_cx, |b, _| b.saved_mtime());
                assert_eq!(
                    guest_saved_mtime, host_saved_mtime,
                    "guest {} saved mtime does not match host's for path {path:?} in project {project_id}",
                    client.username
                );

                let host_is_dirty = host_buffer.read_with(host_cx, |b, _| b.is_dirty());
                let guest_is_dirty = guest_buffer.read_with(client_cx, |b, _| b.is_dirty());
                assert_eq!(
                    guest_is_dirty, host_is_dirty,
                    "guest {} dirty status does not match host's for path {path:?} in project {project_id}",
                    client.username
                );

                let host_has_conflict = host_buffer.read_with(host_cx, |b, _| b.has_conflict());
                let guest_has_conflict = guest_buffer.read_with(client_cx, |b, _| b.has_conflict());
                assert_eq!(
                    guest_has_conflict, host_has_conflict,
                    "guest {} conflict status does not match host's for path {path:?} in project {project_id}",
                    client.username
                );
            }
        }
    }
}

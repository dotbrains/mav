use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_reporting_fs_changes_to_language_servers(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/the-root"),
        json!({
            ".gitignore": "target\n",
            "Cargo.lock": "",
            "src": {
                "a.rs": "",
                "b.rs": "",
            },
            "target": {
                "x": {
                    "out": {
                        "x.rs": ""
                    }
                },
                "y": {
                    "out": {
                        "y.rs": "",
                    }
                },
                "z": {
                    "out": {
                        "z.rs": ""
                    }
                }
            }
        }),
    )
    .await;
    fs.insert_tree(
        path!("/the-registry"),
        json!({
            "dep1": {
                "src": {
                    "dep1.rs": "",
                }
            },
            "dep2": {
                "src": {
                    "dep2.rs": "",
                }
            },
        }),
    )
    .await;
    fs.insert_tree(
        path!("/the/stdlib"),
        json!({
            "LICENSE": "",
            "src": {
                "string.rs": "",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/the-root").as_ref()], cx).await;
    let (language_registry, lsp_store) = project.read_with(cx, |project, _| {
        (project.languages().clone(), project.lsp_store())
    });
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "the-language-server",
            ..Default::default()
        },
    );

    cx.executor().run_until_parked();

    // Start the language server by opening a buffer with a compatible file extension.
    project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/the-root/src/a.rs"), cx)
        })
        .await
        .unwrap();

    // Initially, we don't load ignored files because the language server has not explicitly asked us to watch them.
    project.update(cx, |project, cx| {
        let worktree = project.worktrees(cx).next().unwrap();
        assert_eq!(
            worktree
                .read(cx)
                .snapshot()
                .entries(true, 0)
                .map(|entry| (entry.path.as_unix_str(), entry.is_ignored))
                .collect::<Vec<_>>(),
            &[
                ("", false),
                (".gitignore", false),
                ("Cargo.lock", false),
                ("src", false),
                ("src/a.rs", false),
                ("src/b.rs", false),
                ("target", true),
            ]
        );
    });

    let prev_read_dir_count = fs.read_dir_call_count();

    let fake_server = fake_servers.next().await.unwrap();
    cx.executor().run_until_parked();
    let server_id = lsp_store.read_with(cx, |lsp_store, _| {
        let (id, _) = lsp_store.language_server_statuses().next().unwrap();
        id
    });

    // Simulate jumping to a definition in a dependency outside of the worktree.
    let _out_of_worktree_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer_via_lsp(
                lsp::Uri::from_file_path(path!("/the-registry/dep1/src/dep1.rs")).unwrap(),
                server_id,
                cx,
            )
        })
        .await
        .unwrap();

    // Keep track of the FS events reported to the language server.
    let file_changes = Arc::new(Mutex::new(Vec::new()));
    fake_server
        .request::<lsp::request::RegisterCapability>(
            lsp::RegistrationParams {
                registrations: vec![lsp::Registration {
                    id: Default::default(),
                    method: "workspace/didChangeWatchedFiles".to_string(),
                    register_options: serde_json::to_value(
                        lsp::DidChangeWatchedFilesRegistrationOptions {
                            watchers: vec![
                                lsp::FileSystemWatcher {
                                    glob_pattern: lsp::GlobPattern::String(
                                        path!("/the-root/Cargo.toml").to_string(),
                                    ),
                                    kind: None,
                                },
                                lsp::FileSystemWatcher {
                                    glob_pattern: lsp::GlobPattern::String(
                                        path!("/the-root/src/*.{rs,c}").to_string(),
                                    ),
                                    kind: None,
                                },
                                lsp::FileSystemWatcher {
                                    glob_pattern: lsp::GlobPattern::String(
                                        path!("/the-root/target/y/**/*.rs").to_string(),
                                    ),
                                    kind: None,
                                },
                                lsp::FileSystemWatcher {
                                    glob_pattern: lsp::GlobPattern::String(
                                        path!("/the/stdlib/src/**/*.rs").to_string(),
                                    ),
                                    kind: None,
                                },
                                lsp::FileSystemWatcher {
                                    glob_pattern: lsp::GlobPattern::String(
                                        path!("**/Cargo.lock").to_string(),
                                    ),
                                    kind: None,
                                },
                            ],
                        },
                    )
                    .ok(),
                }],
            },
            DEFAULT_LSP_REQUEST_TIMEOUT,
        )
        .await
        .into_response()
        .unwrap();
    fake_server.handle_notification::<lsp::notification::DidChangeWatchedFiles, _>({
        let file_changes = file_changes.clone();
        move |params, _| {
            let mut file_changes = file_changes.lock();
            file_changes.extend(params.changes);
            file_changes.sort_by(|a, b| a.uri.cmp(&b.uri));
        }
    });

    cx.executor().run_until_parked();
    assert_eq!(mem::take(&mut *file_changes.lock()), &[]);
    assert_eq!(fs.read_dir_call_count() - prev_read_dir_count, 4);

    let mut new_watched_paths = fs.watched_paths();
    new_watched_paths.retain(|path| {
        !path.starts_with(config_dir()) && !path.starts_with(global_gitignore_path().unwrap())
    });
    assert_eq!(
        &new_watched_paths,
        &[
            Path::new(path!("/the-root")),
            Path::new(path!("/the-registry/dep1/src/dep1.rs")),
            Path::new(path!("/the/stdlib/src"))
        ]
    );

    // Now the language server has asked us to watch an ignored directory path,
    // so we recursively load it.
    project.update(cx, |project, cx| {
        let worktree = project.visible_worktrees(cx).next().unwrap();
        assert_eq!(
            worktree
                .read(cx)
                .snapshot()
                .entries(true, 0)
                .map(|entry| (entry.path.as_unix_str(), entry.is_ignored))
                .collect::<Vec<_>>(),
            &[
                ("", false),
                (".gitignore", false),
                ("Cargo.lock", false),
                ("src", false),
                ("src/a.rs", false),
                ("src/b.rs", false),
                ("target", true),
                ("target/x", true),
                ("target/y", true),
                ("target/y/out", true),
                ("target/y/out/y.rs", true),
                ("target/z", true),
            ]
        );
    });

    // Perform some file system mutations, two of which match the watched patterns,
    // and one of which does not.
    fs.create_file(path!("/the-root/src/c.rs").as_ref(), Default::default())
        .await
        .unwrap();
    fs.create_file(path!("/the-root/src/d.txt").as_ref(), Default::default())
        .await
        .unwrap();
    fs.remove_file(path!("/the-root/src/b.rs").as_ref(), Default::default())
        .await
        .unwrap();
    fs.create_file(
        path!("/the-root/target/x/out/x2.rs").as_ref(),
        Default::default(),
    )
    .await
    .unwrap();
    fs.create_file(
        path!("/the-root/target/y/out/y2.rs").as_ref(),
        Default::default(),
    )
    .await
    .unwrap();
    fs.save(
        path!("/the-root/Cargo.lock").as_ref(),
        &"".into(),
        Default::default(),
    )
    .await
    .unwrap();
    fs.save(
        path!("/the-stdlib/LICENSE").as_ref(),
        &"".into(),
        Default::default(),
    )
    .await
    .unwrap();
    fs.save(
        path!("/the/stdlib/src/string.rs").as_ref(),
        &"".into(),
        Default::default(),
    )
    .await
    .unwrap();

    // The language server receives events for the FS mutations that match its watch patterns.
    cx.executor().run_until_parked();
    assert_eq!(
        &*file_changes.lock(),
        &[
            lsp::FileEvent {
                uri: lsp::Uri::from_file_path(path!("/the-root/Cargo.lock")).unwrap(),
                typ: lsp::FileChangeType::CHANGED,
            },
            lsp::FileEvent {
                uri: lsp::Uri::from_file_path(path!("/the-root/src/b.rs")).unwrap(),
                typ: lsp::FileChangeType::DELETED,
            },
            lsp::FileEvent {
                uri: lsp::Uri::from_file_path(path!("/the-root/src/c.rs")).unwrap(),
                typ: lsp::FileChangeType::CREATED,
            },
            lsp::FileEvent {
                uri: lsp::Uri::from_file_path(path!("/the-root/target/y/out/y2.rs")).unwrap(),
                typ: lsp::FileChangeType::CREATED,
            },
            lsp::FileEvent {
                uri: lsp::Uri::from_file_path(path!("/the/stdlib/src/string.rs")).unwrap(),
                typ: lsp::FileChangeType::CHANGED,
            },
        ]
    );
}

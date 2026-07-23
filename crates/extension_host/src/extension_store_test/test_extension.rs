use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_extension_store_with_test_extension(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let executor = cx.executor();

    let root_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let cache_dir = root_dir.join("target");
    let test_extension_id = "test-extension";
    let test_extension_dir = root_dir.join("extensions").join(test_extension_id);

    let fs = Arc::new(RealFs::new(None, cx.executor()));
    let extensions_tree = TempTree::new(json!({
        "installed": {},
        "work": {}
    }));
    let project_dir = TempTree::new(json!({
        "test.gleam": ""
    }));

    let extensions_dir = extensions_tree.path().canonicalize().unwrap();
    let project_dir = project_dir.path().canonicalize().unwrap();

    let project = await_or_timeout(
        &executor,
        "awaiting Project::test",
        5,
        Project::test(fs.clone(), [project_dir.as_path()], cx),
    )
    .await;

    let proxy = Arc::new(ExtensionHostProxy::new());
    let theme_registry = Arc::new(ThemeRegistry::new(Box::new(())));
    theme_extension::init(proxy.clone(), theme_registry.clone(), cx.executor());
    let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
    language_extension::init(
        LspAccess::ViaLspStore(project.update(cx, |project, _| project.lsp_store())),
        proxy.clone(),
        language_registry.clone(),
    );
    let node_runtime = NodeRuntime::unavailable();

    let mut status_updates = language_registry.language_server_binary_statuses();

    let language_server_version = Arc::new(Mutex::new(FakeLanguageServerVersion {
        version: "v1.2.3".into(),
        binary_contents: "the-binary-contents".into(),
        http_request_count: 0,
    }));

    let extension_client = FakeHttpClient::create({
        let language_server_version = language_server_version.clone();
        move |request| {
            let language_server_version = language_server_version.clone();
            async move {
                let version = language_server_version.lock().version.clone();
                let binary_contents = language_server_version.lock().binary_contents.clone();

                let github_releases_uri = "https://api.github.com/repos/gleam-lang/gleam/releases";
                let asset_download_uri =
                    format!("https://fake-download.example.com/gleam-{version}");

                let uri = request.uri().to_string();
                if uri == github_releases_uri {
                    language_server_version.lock().http_request_count += 1;
                    Ok(Response::new(
                        json!([
                            {
                                "tag_name": version,
                                "prerelease": false,
                                "tarball_url": "",
                                "zipball_url": "",
                                "assets": [
                                    {
                                        "name": format!("gleam-{version}-aarch64-apple-darwin.tar.gz"),
                                        "browser_download_url": asset_download_uri
                                    },
                                    {
                                        "name": format!("gleam-{version}-x86_64-unknown-linux-musl.tar.gz"),
                                        "browser_download_url": asset_download_uri
                                    },
                                    {
                                        "name": format!("gleam-{version}-aarch64-unknown-linux-musl.tar.gz"),
                                        "browser_download_url": asset_download_uri
                                    },
                                    {
                                        "name": format!("gleam-{version}-x86_64-pc-windows-msvc.tar.gz"),
                                        "browser_download_url": asset_download_uri
                                    }
                                ]
                            }
                        ])
                        .to_string()
                        .into(),
                    ))
                } else if uri == asset_download_uri {
                    language_server_version.lock().http_request_count += 1;
                    let mut bytes = Vec::<u8>::new();
                    let mut archive = async_tar::Builder::new(&mut bytes);
                    let mut header = async_tar::Header::new_gnu();
                    header.set_size(binary_contents.len() as u64);
                    archive
                        .append_data(&mut header, "gleam", binary_contents.as_bytes())
                        .await
                        .unwrap();
                    archive.into_inner().await.unwrap();
                    let mut gzipped_bytes = Vec::new();
                    let mut encoder = GzipEncoder::new(BufReader::new(bytes.as_slice()));
                    encoder.read_to_end(&mut gzipped_bytes).await.unwrap();
                    Ok(Response::new(gzipped_bytes.into()))
                } else {
                    Ok(Response::builder().status(404).body("not found".into())?)
                }
            }
        }
    });
    let user_agent = cx.update(|cx| {
        format!(
            "Mav/{} ({}; {})",
            AppVersion::global(cx),
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    });
    let builder_client =
        Arc::new(ReqwestClient::user_agent(&user_agent).expect("Could not create HTTP client"));

    let extension_store = cx.new(|cx| {
        ExtensionStore::new(
            extensions_dir.clone(),
            Some(cache_dir),
            proxy,
            fs.clone(),
            extension_client.clone(),
            builder_client,
            None,
            node_runtime,
            cx,
        )
    });

    // Ensure that debounces fire.
    let mut events = cx.events(&extension_store);
    let executor = cx.executor();
    let _task = cx.executor().spawn(async move {
        while let Some(event) = events.next().await {
            if let Event::StartedReloading = event {
                executor.advance_clock(RELOAD_DEBOUNCE_DURATION);
            }
        }
    });

    extension_store.update(cx, |_, cx| {
        cx.subscribe(&extension_store, |_, _, event, _| {
            if matches!(event, Event::ExtensionFailedToLoad(_)) {
                panic!("extension failed to load");
            }
        })
        .detach();
    });

    let mut extension_events = cx.events(&cx.update(|cx| {
        extension::ExtensionEvents::try_global(cx)
            .expect("ExtensionEvents should be initialized in tests")
    }));

    let executor = cx.executor();
    await_or_timeout(
        &executor,
        "awaiting install_dev_extension",
        60,
        extension_store.update(cx, |store, cx| {
            store.install_dev_extension(test_extension_dir.clone(), cx)
        }),
    )
    .await
    .unwrap();

    await_or_timeout(
        &executor,
        "awaiting ExtensionsInstalledChanged",
        10,
        async {
            while let Some(event) = extension_events.next().await {
                if matches!(event, extension::Event::ExtensionsInstalledChanged) {
                    return;
                }
            }

            panic!(
                "[test_extension_store_with_test_extension] extension event stream ended before ExtensionsInstalledChanged"
            );
        },
    )
    .await;

    let mut fake_servers = language_registry.register_fake_lsp_server(
        LanguageServerName("gleam".into()),
        lsp::ServerCapabilities {
            completion_provider: Some(Default::default()),
            ..Default::default()
        },
        None,
    );
    cx.executor().run_until_parked();

    let mut project_events = cx.events(&project);
    let buffer_path = project_dir.join("test.gleam");
    let (buffer, _handle) = await_or_timeout(
        &executor,
        "awaiting open_local_buffer_with_lsp",
        5,
        project.update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(buffer_path.clone(), cx)
        }),
    )
    .await
    .unwrap();
    cx.executor().run_until_parked();

    let buffer_remote_id = buffer.read_with(cx, |buffer, _cx| buffer.remote_id());

    let fake_server = await_or_timeout(
        &executor,
        "awaiting first fake server spawn",
        10,
        fake_servers.next(),
    )
    .await
    .unwrap();

    let work_dir = extensions_dir.join(format!("work/{test_extension_id}"));
    let expected_server_path = work_dir.join("gleam-v1.2.3/gleam");
    let expected_binary_contents = language_server_version.lock().binary_contents.clone();

    // check that IO operations in extension work correctly
    assert!(work_dir.join("dir-created-with-rel-path").exists());
    assert!(work_dir.join("dir-created-with-abs-path").exists());
    assert!(work_dir.join("file-created-with-abs-path").exists());
    assert!(work_dir.join("file-created-with-rel-path").exists());

    assert_eq!(fake_server.binary.path, expected_server_path);
    assert_eq!(fake_server.binary.arguments, [OsString::from("lsp")]);
    assert_eq!(
        await_or_timeout(
            &executor,
            "awaiting fs.load(expected_server_path)",
            5,
            fs.load(&expected_server_path)
        )
        .await
        .unwrap(),
        expected_binary_contents
    );
    assert_eq!(language_server_version.lock().http_request_count, 2);
    assert_eq!(
        [
            await_or_timeout(
                &executor,
                "awaiting status_updates #1",
                5,
                status_updates.next()
            )
            .await
            .unwrap(),
            await_or_timeout(
                &executor,
                "awaiting status_updates #2",
                5,
                status_updates.next()
            )
            .await
            .unwrap(),
            await_or_timeout(
                &executor,
                "awaiting status_updates #3",
                5,
                status_updates.next()
            )
            .await
            .unwrap(),
            await_or_timeout(
                &executor,
                "awaiting status_updates #4",
                5,
                status_updates.next()
            )
            .await
            .unwrap(),
        ],
        [
            (
                LanguageServerName::new_static("gleam"),
                BinaryStatus::Starting
            ),
            (
                LanguageServerName::new_static("gleam"),
                BinaryStatus::CheckingForUpdate
            ),
            (
                LanguageServerName::new_static("gleam"),
                BinaryStatus::Downloading
            ),
            (LanguageServerName::new_static("gleam"), BinaryStatus::None)
        ]
    );

    // The extension creates custom labels for completion items.
    fake_server.set_request_handler::<lsp::request::Completion, _, _>(|_, _| async move {
        Ok(Some(lsp::CompletionResponse::Array(vec![
            lsp::CompletionItem {
                label: "foo".into(),
                kind: Some(lsp::CompletionItemKind::FUNCTION),
                detail: Some("fn() -> Result(Nil, Error)".into()),
                ..Default::default()
            },
            lsp::CompletionItem {
                label: "bar.baz".into(),
                kind: Some(lsp::CompletionItemKind::FUNCTION),
                detail: Some("fn(List(a)) -> a".into()),
                ..Default::default()
            },
            lsp::CompletionItem {
                label: "Quux".into(),
                kind: Some(lsp::CompletionItemKind::CONSTRUCTOR),
                detail: Some("fn(String) -> T".into()),
                ..Default::default()
            },
            lsp::CompletionItem {
                label: "my_string".into(),
                kind: Some(lsp::CompletionItemKind::CONSTANT),
                detail: Some("String".into()),
                ..Default::default()
            },
        ])))
    });

    // `register_fake_lsp_server` can yield a server instance before the client has fully registered
    // the buffer with the project LSP plumbing. Wait for the project to observe that registration
    // before issuing requests like completion.
    await_or_timeout(
        &executor,
        "awaiting LanguageServerBufferRegistered",
        5,
        async {
            while let Some(event) = project_events.next().await {
                if let project::Event::LanguageServerBufferRegistered { buffer_id, .. } = event {
                    if buffer_id == buffer_remote_id {
                        return;
                    }
                }
            }

            panic!(
                "[test_extension_store_with_test_extension] project event stream ended before buffer registration for {}",
                buffer_path.display()
            );
        },
    )
    .await;

    let completion_labels = await_or_timeout(
        &executor,
        "awaiting completions",
        5,
        project.update(cx, |project, cx| {
            project.completions(&buffer, 0, DEFAULT_COMPLETION_CONTEXT, cx)
        }),
    )
    .await
    .unwrap()
    .into_iter()
    .flat_map(|response| response.completions)
    .map(|c| c.label.text)
    .collect::<Vec<_>>();
    assert_eq!(
        completion_labels,
        [
            "foo: fn() -> Result(Nil, Error)".to_string(),
            "bar.baz: fn(List(a)) -> a".to_string(),
            "Quux: fn(String) -> T".to_string(),
            "my_string: String".to_string(),
        ]
    );

    // Simulate a new version of the language server being released
    language_server_version.lock().version = "v2.0.0".into();
    language_server_version.lock().binary_contents = "the-new-binary-contents".into();
    language_server_version.lock().http_request_count = 0;

    // Start a new instance of the language server.
    project.update(cx, |project, cx| {
        project.restart_language_servers_for_buffers(
            vec![buffer.clone()],
            HashSet::default(),
            true,
            cx,
        )
    });
    cx.executor().run_until_parked();

    // The extension has cached the binary path, and does not attempt
    // to reinstall it.
    let fake_server = await_or_timeout(
        &executor,
        "awaiting second fake server spawn",
        5,
        fake_servers.next(),
    )
    .await
    .unwrap();
    assert_eq!(fake_server.binary.path, expected_server_path);
    assert_eq!(
        await_or_timeout(
            &executor,
            "awaiting fs.load(expected_server_path) after restart",
            5,
            fs.load(&expected_server_path)
        )
        .await
        .unwrap(),
        expected_binary_contents
    );
    assert_eq!(language_server_version.lock().http_request_count, 0);

    // Reload the extension, clearing its cache.
    // Start a new instance of the language server.
    await_or_timeout(
        &executor,
        "awaiting extension_store.reload(test-extension)",
        5,
        extension_store.update(cx, |store, cx| {
            store.reload(Some("test-extension".into()), cx)
        }),
    )
    .await;
    cx.executor().run_until_parked();
    project.update(cx, |project, cx| {
        project.restart_language_servers_for_buffers(
            vec![buffer.clone()],
            HashSet::default(),
            true,
            cx,
        )
    });

    // The extension re-fetches the latest version of the language server.
    let fake_server = await_or_timeout(
        &executor,
        "awaiting third fake server spawn",
        5,
        fake_servers.next(),
    )
    .await
    .unwrap();
    let new_expected_server_path =
        extensions_dir.join(format!("work/{test_extension_id}/gleam-v2.0.0/gleam"));
    let expected_binary_contents = language_server_version.lock().binary_contents.clone();
    assert_eq!(fake_server.binary.path, new_expected_server_path);
    assert_eq!(fake_server.binary.arguments, [OsString::from("lsp")]);
    assert_eq!(
        await_or_timeout(
            &executor,
            "awaiting fs.load(new_expected_server_path)",
            5,
            fs.load(&new_expected_server_path)
        )
        .await
        .unwrap(),
        expected_binary_contents
    );

    // The old language server directory has been cleaned up.
    assert!(
        await_or_timeout(
            &executor,
            "awaiting fs.metadata(expected_server_path)",
            5,
            fs.metadata(&expected_server_path)
        )
        .await
        .unwrap()
        .is_none()
    );
}

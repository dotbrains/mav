use call::ActiveCall;
use collections::{BTreeMap, HashSet};
use fs::Fs as _;
use futures::StreamExt as _;
use gpui::{BackgroundExecutor, TestAppContext, UpdateGlobal};
use language::{
    FakeLspAdapter, Language, LanguageConfig, LanguageMatcher, LineEnding, Rope,
    language_settings::{Formatter, FormatterList},
    rust_lang, tree_sitter_typescript,
};
use lsp::OneOf;
use pretty_assertions::assert_eq;
use project::lsp_store::{FormatTrigger, LspFormatTarget};
use serde_json::json;
use settings::{LanguageServerFormatterSpecifier, PrettierSettingsContent, SettingsStore};
use std::{env, sync::Arc};
use util::{path, rel_path::rel_path};

use crate::TestServer;

#[gpui::test(iterations = 10)]
async fn test_reloading_buffer_manually(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(path!("/a"), json!({ "a.rs": "let one = 1;" }))
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/a"), cx_a).await;
    let buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.rs")), cx)
        })
        .await
        .unwrap();
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let open_buffer = project_b.update(cx_b, |p, cx| {
        p.open_buffer((worktree_id, rel_path("a.rs")), cx)
    });
    let buffer_b = cx_b.executor().spawn(open_buffer).await.unwrap();
    buffer_b.update(cx_b, |buffer, cx| {
        buffer.edit([(4..7, "six")], None, cx);
        buffer.edit([(10..11, "6")], None, cx);
        assert_eq!(buffer.text(), "let six = 6;");
        assert!(buffer.is_dirty());
        assert!(!buffer.has_conflict());
    });
    executor.run_until_parked();

    buffer_a.read_with(cx_a, |buffer, _| assert_eq!(buffer.text(), "let six = 6;"));

    client_a
        .fs()
        .save(
            path!("/a/a.rs").as_ref(),
            &Rope::from("let seven = 7;"),
            LineEnding::Unix,
        )
        .await
        .unwrap();
    executor.run_until_parked();

    buffer_a.read_with(cx_a, |buffer, _| assert!(buffer.has_conflict()));

    buffer_b.read_with(cx_b, |buffer, _| assert!(buffer.has_conflict()));

    project_b
        .update(cx_b, |project, cx| {
            project.reload_buffers(HashSet::from_iter([buffer_b.clone()]), true, cx)
        })
        .await
        .unwrap();

    buffer_a.read_with(cx_a, |buffer, _| {
        assert_eq!(buffer.text(), "let seven = 7;");
        assert!(!buffer.is_dirty());
        assert!(!buffer.has_conflict());
    });

    buffer_b.read_with(cx_b, |buffer, _| {
        assert_eq!(buffer.text(), "let seven = 7;");
        assert!(!buffer.is_dirty());
        assert!(!buffer.has_conflict());
    });

    buffer_a.update(cx_a, |buffer, cx| {
        // Undoing on the host is a no-op when the reload was initiated by the guest.
        buffer.undo(cx);
        assert_eq!(buffer.text(), "let seven = 7;");
        assert!(!buffer.is_dirty());
        assert!(!buffer.has_conflict());
    });
    buffer_b.update(cx_b, |buffer, cx| {
        // Undoing on the guest rolls back the buffer to before it was reloaded but the conflict gets cleared.
        buffer.undo(cx);
        assert_eq!(buffer.text(), "let six = 6;");
        assert!(buffer.is_dirty());
        assert!(!buffer.has_conflict());
    });
}

#[gpui::test(iterations = 10)]
async fn test_formatting_buffer(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a.language_registry().add(rust_lang());
    let mut fake_language_servers = client_a
        .language_registry()
        .register_fake_lsp("Rust", FakeLspAdapter::default());

    // Here we insert a fake tree with a directory that exists on disk. This is needed
    // because later we'll invoke a command, which requires passing a working directory
    // that points to a valid location on disk.
    let directory = env::current_dir().unwrap();
    client_a
        .fs()
        .insert_tree(&directory, json!({ "a.rs": "let one = \"two\"" }))
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(&directory, cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.rs")), cx)
        })
        .await
        .unwrap();

    let _handle = project_b.update(cx_b, |project, cx| {
        project.register_buffer_with_language_servers(&buffer_b, cx)
    });
    let fake_language_server = fake_language_servers.next().await.unwrap();
    executor.run_until_parked();
    fake_language_server.set_request_handler::<lsp::request::Formatting, _, _>(|_, _| async move {
        Ok(Some(vec![
            lsp::TextEdit {
                range: lsp::Range::new(lsp::Position::new(0, 4), lsp::Position::new(0, 4)),
                new_text: "h".to_string(),
            },
            lsp::TextEdit {
                range: lsp::Range::new(lsp::Position::new(0, 7), lsp::Position::new(0, 7)),
                new_text: "y".to_string(),
            },
        ]))
    });

    project_b
        .update(cx_b, |project, cx| {
            project.format(
                HashSet::from_iter([buffer_b.clone()]),
                LspFormatTarget::Buffers,
                true,
                FormatTrigger::Save,
                cx,
            )
        })
        .await
        .unwrap();

    // The edits from the LSP are applied, and a final newline is added.
    assert_eq!(
        buffer_b.read_with(cx_b, |buffer, _| buffer.text()),
        "let honey = \"two\"\n"
    );

    // There is no `awk` command on Windows.
    #[cfg(not(target_os = "windows"))]
    {
        // Ensure buffer can be formatted using an external command. Notice how the
        // host's configuration is honored as opposed to using the guest's settings.
        cx_a.update(|cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |file| {
                    file.project.all_languages.defaults.formatter =
                        Some(FormatterList::Single(Formatter::External {
                            command: "awk".into(),
                            arguments: Some(vec!["{sub(/two/,\"{buffer_path}\")}1".to_string()]),
                        }));
                });
            });
        });

        executor.allow_parking();
        project_b
            .update(cx_b, |project, cx| {
                project.format(
                    HashSet::from_iter([buffer_b.clone()]),
                    LspFormatTarget::Buffers,
                    true,
                    FormatTrigger::Save,
                    cx,
                )
            })
            .await
            .unwrap();
        assert_eq!(
            buffer_b.read_with(cx_b, |buffer, _| buffer.text()),
            format!("let honey = \"{}/a.rs\"\n", directory.to_str().unwrap())
        );
    }
}

#[gpui::test(iterations = 10)]
async fn test_range_formatting_buffer(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a.language_registry().add(rust_lang());
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                document_range_formatting_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let directory = env::current_dir().unwrap();
    client_a
        .fs()
        .insert_tree(&directory, json!({ "a.rs": "one\ntwo\nthree\n" }))
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(&directory, cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.rs")), cx)
        })
        .await
        .unwrap();

    let _handle = project_b.update(cx_b, |project, cx| {
        project.register_buffer_with_language_servers(&buffer_b, cx)
    });
    let fake_language_server = fake_language_servers.next().await.unwrap();
    executor.run_until_parked();

    fake_language_server.set_request_handler::<lsp::request::RangeFormatting, _, _>(
        |params, _| async move {
            assert_eq!(params.range.start, lsp::Position::new(0, 0));
            assert_eq!(params.range.end, lsp::Position::new(1, 3));
            Ok(Some(vec![lsp::TextEdit::new(
                lsp::Range::new(lsp::Position::new(0, 3), lsp::Position::new(1, 0)),
                ", ".to_string(),
            )]))
        },
    );

    let buffer_id = buffer_b.read_with(cx_b, |buffer, _| buffer.remote_id());
    let ranges = buffer_b.read_with(cx_b, |buffer, _| {
        let start = buffer.anchor_before(0);
        let end = buffer.anchor_after(7);
        vec![start..end]
    });

    let mut ranges_map = BTreeMap::new();
    ranges_map.insert(buffer_id, ranges);

    project_b
        .update(cx_b, |project, cx| {
            project.format(
                HashSet::from_iter([buffer_b.clone()]),
                LspFormatTarget::Ranges(ranges_map),
                true,
                FormatTrigger::Manual,
                cx,
            )
        })
        .await
        .unwrap();

    assert_eq!(
        buffer_b.read_with(cx_b, |buffer, _| buffer.text()),
        "one, two\nthree\n"
    );
}

#[gpui::test(iterations = 10)]
async fn test_prettier_formatting_buffer(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    let test_plugin = "test_plugin";

    client_a.language_registry().add(Arc::new(Language::new(
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    )));
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            prettier_plugins: vec![test_plugin],
            ..Default::default()
        },
    );

    // Here we insert a fake tree with a directory that exists on disk. This is needed
    // because later we'll invoke a command, which requires passing a working directory
    // that points to a valid location on disk.
    let directory = env::current_dir().unwrap();
    let buffer_text = "let one = \"two\"";
    client_a
        .fs()
        .insert_tree(&directory, json!({ "a.ts": buffer_text }))
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(&directory, cx_a).await;
    let prettier_format_suffix = project::TEST_PRETTIER_FORMAT_SUFFIX;
    let open_buffer = project_a.update(cx_a, |p, cx| {
        p.open_buffer((worktree_id, rel_path("a.ts")), cx)
    });
    let buffer_a = cx_a.executor().spawn(open_buffer).await.unwrap();

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    let (buffer_b, _) = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer_with_lsp((worktree_id, rel_path("a.ts")), cx)
        })
        .await
        .unwrap();

    cx_a.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |file| {
                file.project.all_languages.defaults.formatter = Some(FormatterList::default());
                file.project.all_languages.defaults.prettier = Some(PrettierSettingsContent {
                    allowed: Some(true),
                    ..Default::default()
                });
            });
        });
    });
    cx_b.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |file| {
                file.project.all_languages.defaults.formatter = Some(FormatterList::Single(
                    Formatter::LanguageServer(LanguageServerFormatterSpecifier::Current),
                ));
                file.project.all_languages.defaults.prettier = Some(PrettierSettingsContent {
                    allowed: Some(true),
                    ..Default::default()
                });
            });
        });
    });
    let fake_language_server = fake_language_servers.next().await.unwrap();
    fake_language_server.set_request_handler::<lsp::request::Formatting, _, _>(|_, _| async move {
        panic!(
            "Unexpected: prettier should be preferred since it's enabled and language supports it"
        )
    });

    project_b
        .update(cx_b, |project, cx| {
            project.format(
                HashSet::from_iter([buffer_b.clone()]),
                LspFormatTarget::Buffers,
                true,
                FormatTrigger::Save,
                cx,
            )
        })
        .await
        .unwrap();

    executor.run_until_parked();
    assert_eq!(
        buffer_b.read_with(cx_b, |buffer, _| buffer.text()),
        buffer_text.to_string() + "\n" + prettier_format_suffix,
        "Prettier formatting was not applied to client buffer after client's request"
    );

    project_a
        .update(cx_a, |project, cx| {
            project.format(
                HashSet::from_iter([buffer_a.clone()]),
                LspFormatTarget::Buffers,
                true,
                FormatTrigger::Manual,
                cx,
            )
        })
        .await
        .unwrap();

    executor.run_until_parked();
    assert_eq!(
        buffer_b.read_with(cx_b, |buffer, _| buffer.text()),
        buffer_text.to_string() + "\n" + prettier_format_suffix + "\n" + prettier_format_suffix,
        "Prettier formatting was not applied to client buffer after host's request"
    );
}

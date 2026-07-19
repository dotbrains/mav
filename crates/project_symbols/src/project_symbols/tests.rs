use super::*;
use futures::StreamExt;
use gpui::{TestAppContext, VisualContext};
use language::{FakeLspAdapter, Language, LanguageConfig, LanguageMatcher};
use lsp::OneOf;
use project::FakeFs;
use serde_json::json;
use settings::SettingsStore;
use std::{path::Path, sync::Arc};
use util::path;
use workspace::MultiWorkspace;

#[gpui::test]
async fn test_project_symbols(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "test.rs": "" }))
        .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(Arc::new(Language::new(
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
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                workspace_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let _buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/test.rs"), cx)
        })
        .await
        .unwrap();

    let fake_symbols = [
        symbol("one", path!("/external")),
        symbol("ton", path!("/dir/test.rs")),
        symbol("uno", path!("/dir/test.rs")),
    ];
    let fake_server = fake_servers.next().await.unwrap();
    fake_server.set_request_handler::<lsp::WorkspaceSymbolRequest, _, _>(
        move |params: lsp::WorkspaceSymbolParams, cx| {
            let executor = cx.background_executor().clone();
            let fake_symbols = fake_symbols.clone();
            async move {
                let (query, prefixed) = match params.query.strip_prefix("dir::") {
                    Some(query) => (query, true),
                    None => (&*params.query, false),
                };
                let candidates = fake_symbols
                    .iter()
                    .enumerate()
                    .filter(|(_, symbol)| !prefixed || symbol.location.uri.path().contains("dir"))
                    .map(|(id, symbol)| StringMatchCandidate::new(id, &symbol.name))
                    .collect::<Vec<_>>();
                let matches = if query.is_empty() {
                    Vec::new()
                } else {
                    fuzzy::match_strings(
                        &candidates,
                        &query,
                        true,
                        true,
                        100,
                        &Default::default(),
                        executor.clone(),
                    )
                    .await
                };

                Ok(Some(lsp::WorkspaceSymbolResponse::Flat(
                    matches
                        .into_iter()
                        .map(|mat| fake_symbols[mat.candidate_id].clone())
                        .collect(),
                )))
            }
        },
    );

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let symbols = cx.new_window_entity(|window, cx| {
        Picker::uniform_list(
            ProjectSymbolsDelegate::new(workspace.downgrade(), project.clone()),
            window,
            cx,
        )
    });

    symbols.update_in(cx, |p, window, cx| {
        p.update_matches("o".to_string(), window, cx);
        p.update_matches("on".to_string(), window, cx);
        p.update_matches("onex".to_string(), window, cx);
    });

    cx.run_until_parked();
    symbols.read_with(cx, |symbols, _| {
        assert_eq!(symbols.delegate.matches.len(), 0);
    });

    symbols.update_in(cx, |p, window, cx| {
        p.update_matches("one".to_string(), window, cx);
        p.update_matches("on".to_string(), window, cx);
    });

    cx.run_until_parked();
    symbols.read_with(cx, |symbols, _| {
        let delegate = &symbols.delegate;
        assert_eq!(delegate.matches.len(), 2);
        assert_eq!(delegate.matches[0].string, "ton");
        assert_eq!(delegate.matches[1].string, "one");
    });

    symbols.update_in(cx, |p, window, cx| {
        p.update_matches("o".to_string(), window, cx);
        p.update_matches("".to_string(), window, cx);
    });

    cx.run_until_parked();
    symbols.read_with(cx, |symbols, _| {
        assert_eq!(symbols.delegate.matches.len(), 0);
    });

    symbols.update_in(cx, |p, window, cx| {
        p.update_matches("dir::to".to_string(), window, cx);
    });

    cx.run_until_parked();
    symbols.read_with(cx, |symbols, _| {
        assert_eq!(symbols.delegate.matches.len(), 1);
    });
}

#[gpui::test]
async fn test_project_symbols_renders_utf8_match(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "test.rs": "" }))
        .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(Arc::new(Language::new(
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
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                workspace_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let _buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/test.rs"), cx)
        })
        .await
        .unwrap();

    let fake_symbols = [symbol("안녕", path!("/dir/test.rs"))];
    let fake_server = fake_servers.next().await.unwrap();
    fake_server.set_request_handler::<lsp::WorkspaceSymbolRequest, _, _>(
        move |params: lsp::WorkspaceSymbolParams, cx| {
            let executor = cx.background_executor().clone();
            let fake_symbols = fake_symbols.clone();
            async move {
                let candidates = fake_symbols
                    .iter()
                    .enumerate()
                    .map(|(id, symbol)| StringMatchCandidate::new(id, &symbol.name))
                    .collect::<Vec<_>>();
                let matches = fuzzy::match_strings(
                    &candidates,
                    &params.query,
                    true,
                    true,
                    100,
                    &Default::default(),
                    executor,
                )
                .await;

                Ok(Some(lsp::WorkspaceSymbolResponse::Flat(
                    matches
                        .into_iter()
                        .map(|mat| fake_symbols[mat.candidate_id].clone())
                        .collect(),
                )))
            }
        },
    );

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let symbols = cx.new_window_entity(|window, cx| {
        Picker::uniform_list(
            ProjectSymbolsDelegate::new(workspace.downgrade(), project.clone()),
            window,
            cx,
        )
    });

    symbols.update_in(cx, |p, window, cx| {
        p.update_matches("안".to_string(), window, cx);
    });

    cx.run_until_parked();
    symbols.read_with(cx, |symbols, _| {
        assert_eq!(symbols.delegate.matches.len(), 1);
        assert_eq!(symbols.delegate.matches[0].string, "안녕");
    });

    symbols.update_in(cx, |p, window, cx| {
        assert!(p.delegate.render_match(0, false, window, cx).is_some());
    });
}

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let store = SettingsStore::test(cx);
        cx.set_global(store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        editor::init(cx);
    });
}

fn symbol(name: &str, path: impl AsRef<Path>) -> lsp::SymbolInformation {
    #[allow(deprecated)]
    lsp::SymbolInformation {
        name: name.to_string(),
        kind: lsp::SymbolKind::FUNCTION,
        tags: None,
        deprecated: None,
        container_name: None,
        location: lsp::Location::new(
            lsp::Uri::from_file_path(path.as_ref()).unwrap(),
            lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 0)),
        ),
    }
}

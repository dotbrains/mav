use super::test_support::*;

#[gpui::test]
async fn test_code_lens_resolve_only_visible(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.code_lens = Some(CodeLens::On);
    });

    let line_count: u32 = 100;
    let lens_every: u32 = 10;
    let lines = (0..line_count)
        .map(|i| format!("function func_{i}() {{}}"))
        .collect::<Vec<_>>()
        .join("\n");

    let lens_lines = (0..line_count)
        .filter(|i| i % lens_every == 0)
        .collect::<Vec<_>>();

    let resolved_lines = Arc::new(Mutex::new(Vec::<u32>::new()));

    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), serde_json::json!({ "main.ts": lines }))
        .await;

    let project = project::Project::test(fs, [path!("/dir").as_ref()], cx).await;
    let (multi_workspace, cx) = cx.add_window_view(|window, cx| {
        workspace::MultiWorkspace::test_new(project.clone(), window, cx)
    });
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(Arc::new(language::Language::new(
        language::LanguageConfig {
            name: "TypeScript".into(),
            matcher: language::LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..language::LanguageMatcher::default()
            },
            ..language::LanguageConfig::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    )));

    let mut fake_servers = language_registry.register_fake_lsp(
        "TypeScript",
        language::FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                code_lens_provider: Some(lsp::CodeLensOptions {
                    resolve_provider: Some(true),
                }),
                ..lsp::ServerCapabilities::default()
            },
            ..language::FakeLspAdapter::default()
        },
    );

    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                std::path::PathBuf::from(path!("/dir/main.ts")),
                workspace::OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    let fake_server = fake_servers.next().await.unwrap();

    let lens_lines_for_handler = lens_lines.clone();
    fake_server.set_request_handler::<lsp::request::CodeLensRequest, _, _>(move |_, _| {
        let lens_lines = lens_lines_for_handler.clone();
        async move {
            Ok(Some(
                lens_lines
                    .iter()
                    .map(|&line| lsp::CodeLens {
                        range: lsp::Range::new(
                            lsp::Position::new(line, 0),
                            lsp::Position::new(line, 10),
                        ),
                        command: None,
                        data: Some(serde_json::json!({ "line": line })),
                    })
                    .collect(),
            ))
        }
    });

    {
        let resolved_lines = resolved_lines.clone();
        fake_server.set_request_handler::<lsp::request::CodeLensResolve, _, _>(move |lens, _| {
            let resolved_lines = resolved_lines.clone();
            async move {
                let line = lens
                    .data
                    .as_ref()
                    .and_then(|d| d.get("line"))
                    .and_then(|v| v.as_u64())
                    .unwrap() as u32;
                resolved_lines.lock().unwrap().push(line);
                Ok(lsp::CodeLens {
                    command: Some(lsp::Command {
                        title: format!("{line} references"),
                        command: format!("show_refs_{line}"),
                        arguments: None,
                    }),
                    ..lens
                })
            }
        });
    }

    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();

    let initial_resolved = resolved_lines
        .lock()
        .unwrap()
        .drain(..)
        .collect::<HashSet<_>>();
    assert_eq!(
        initial_resolved,
        HashSet::from_iter([0, 10, 20, 30, 40]),
        "Only lenses visible at the top should be resolved"
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end(&crate::actions::MoveToEnd, window, cx);
    });
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();

    let after_scroll_resolved = resolved_lines
        .lock()
        .unwrap()
        .drain(..)
        .collect::<HashSet<_>>();
    // Once the lenses are first applied we insert a placeholder block per
    // lens row so the line is reserved while the resolve is in flight.
    // Those placeholder blocks add display height, so after scrolling to
    // the end the visible buffer-row range is slightly smaller than it
    // would be without them, and lens row 60 is just outside it.
    assert_eq!(
        after_scroll_resolved,
        HashSet::from_iter([70, 80, 90]),
        "Only newly visible lenses at the bottom should be resolved, not middle ones"
    );
}

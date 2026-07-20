use super::*;
use pretty_assertions::assert_eq;

#[gpui::test(iterations = 10)]
async fn test_definition(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.rs": "const fn a() { A }",
            "b.rs": "const y: i32 = crate::a()",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir/b.rs").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp("Rust", FakeLspAdapter::default());

    let (buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/b.rs"), cx)
        })
        .await
        .unwrap();

    let fake_server = fake_servers.next().await.unwrap();
    cx.executor().run_until_parked();

    fake_server.set_request_handler::<lsp::request::GotoDefinition, _, _>(|params, _| async move {
        let params = params.text_document_position_params;
        assert_eq!(
            params.text_document.uri.to_file_path().unwrap(),
            Path::new(path!("/dir/b.rs")),
        );
        assert_eq!(params.position, lsp::Position::new(0, 22));

        Ok(Some(lsp::GotoDefinitionResponse::Scalar(
            lsp::Location::new(
                lsp::Uri::from_file_path(path!("/dir/a.rs")).unwrap(),
                lsp::Range::new(lsp::Position::new(0, 9), lsp::Position::new(0, 10)),
            ),
        )))
    });
    let mut definitions = project
        .update(cx, |project, cx| project.definitions(&buffer, 22, cx))
        .await
        .unwrap()
        .unwrap();

    // Assert no new language server started
    cx.executor().run_until_parked();
    assert!(fake_servers.try_recv().is_err());

    assert_eq!(definitions.len(), 1);
    let definition = definitions.pop().unwrap();
    cx.update(|cx| {
        let target_buffer = definition.target.buffer.read(cx);
        assert_eq!(
            target_buffer
                .file()
                .unwrap()
                .as_local()
                .unwrap()
                .abs_path(cx),
            Path::new(path!("/dir/a.rs")),
        );
        assert_eq!(definition.target.range.to_offset(target_buffer), 9..10);
        assert_eq!(
            list_worktrees(&project, cx),
            [
                (path!("/dir/b.rs").as_ref(), true),
                (path!("/dir/a.rs").as_ref(), false),
            ],
        );

        drop(definition);
    });
    cx.update(|cx| {
        assert_eq!(
            list_worktrees(&project, cx),
            [(path!("/dir/b.rs").as_ref(), true)]
        );
    });

    fn list_worktrees<'a>(project: &'a Entity<Project>, cx: &'a App) -> Vec<(&'a Path, bool)> {
        project
            .read(cx)
            .worktrees(cx)
            .map(|worktree| {
                let worktree = worktree.read(cx);
                (
                    worktree.as_local().unwrap().abs_path().as_ref(),
                    worktree.is_visible(),
                )
            })
            .collect::<Vec<_>>()
    }
}

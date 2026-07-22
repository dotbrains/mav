use super::*;

#[gpui::test]
async fn test_sort_by_name_tie_breaks_on_path(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                let git_panel = settings.git_panel.get_or_insert_default();
                git_panel.sort_by = Some(GitPanelSortBy::Name);
                git_panel.group_by = Some(GitPanelGroupBy::None);
            });
        });
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "lib": { "foo.rs": "LIB FOO\n" },
            "src": { "foo.rs": "SRC FOO\n" },
            "m.rs": "M\n",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let diff =
        cx.new_window_entity(|window, cx| ProjectDiff::new(project.clone(), workspace, window, cx));
    cx.run_until_parked();

    fs.set_head_and_index_for_repo(
        path!("/project/.git").as_ref(),
        &[
            ("lib/foo.rs", "lib foo\n".into()),
            ("src/foo.rs", "src foo\n".into()),
            ("m.rs", "m\n".into()),
        ],
    );
    cx.run_until_parked();

    // Sorted by file name, the two `foo.rs` files come before `m.rs`, and the
    // tie between them is broken by the full path (`lib/` before `src/`).
    // A plain path sort would instead order them `lib/foo.rs`, `m.rs`,
    // `src/foo.rs`.
    let paths = diff.read_with(cx, |diff, cx| diff.excerpt_file_paths(cx));
    assert_eq!(paths, vec!["lib/foo.rs", "src/foo.rs", "m.rs"]);
}

#[gpui::test]
async fn test_tree_view_orders_directories_before_files(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                let git_panel = settings.git_panel.get_or_insert_default();
                git_panel.tree_view = Some(true);
                git_panel.group_by = Some(GitPanelGroupBy::None);
            });
        });
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "src": {
                "a.rs": "A\n",
                "m.rs": "M\n",
                "sub": { "b.rs": "B\n" },
            },
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let diff =
        cx.new_window_entity(|window, cx| ProjectDiff::new(project.clone(), workspace, window, cx));
    cx.run_until_parked();

    fs.set_head_and_index_for_repo(
        path!("/project/.git").as_ref(),
        &[
            ("src/a.rs", "a\n".into()),
            ("src/m.rs", "m\n".into()),
            ("src/sub/b.rs", "b\n".into()),
        ],
    );
    cx.run_until_parked();

    // In tree view the `src/sub/` directory sorts before the files directly
    // in `src/`. A plain path sort would interleave them as `src/a.rs`,
    // `src/m.rs`, `src/sub/b.rs`.
    let paths = diff.read_with(cx, |diff, cx| diff.excerpt_file_paths(cx));
    assert_eq!(paths, vec!["src/sub/b.rs", "src/a.rs", "src/m.rs"]);
}

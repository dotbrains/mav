use super::*;

#[gpui::test]
async fn test_tree_view_without_status_grouping_combines_statuses(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "src": {
                "main.rs": "fn main() {}",
                "utils.rs": "pub fn util() {}",
            },
            "tests": {
                "main_test.rs": "#[test] fn test_main() {}",
            },
        }),
    )
    .await;

    fs.set_status_for_repo(
        path!("/project/.git").as_ref(),
        &[
            ("src/main.rs", StatusCode::Modified.worktree()),
            ("src/utils.rs", FileStatus::Untracked),
            ("tests/main_test.rs", StatusCode::Modified.worktree()),
        ],
    );

    let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

    cx.read(|cx| {
        project
            .read(cx)
            .worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .as_local()
            .unwrap()
            .scan_complete()
    })
    .await;

    cx.executor().run_until_parked();
    cx.update(|_window, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                let git_panel = settings.git_panel.get_or_insert_default();
                git_panel.tree_view = Some(true);
                git_panel.group_by = Some(GitPanelGroupBy::None);
            })
        });
    });

    let panel = workspace.update_in(cx, GitPanel::new);
    let handle = cx.update_window_entity(&panel, |panel, _, _| {
        std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
    });

    cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
    handle.await;

    panel.read_with(cx, |panel, _| {
        assert!(
            panel
                .entries
                .iter()
                .all(|entry| !matches!(entry, GitListEntry::Header(_))),
            "status headers should not be shown when grouping is disabled",
        );

        let tree_state = panel
            .view_mode
            .tree_state()
            .expect("tree view state should exist");
        let src_key = panel
            .entries
            .iter()
            .find_map(|entry| match entry {
                GitListEntry::Directory(dir) if dir.key.path == repo_path("src") => Some(&dir.key),
                _ => None,
            })
            .expect("src directory should exist in tree view");
        let src_descendants = tree_state
            .directory_descendants
            .get(src_key)
            .expect("src descendants should be tracked");

        assert!(
            src_descendants
                .iter()
                .any(|entry| entry.repo_path == repo_path("src/main.rs"))
        );
        assert!(
            src_descendants
                .iter()
                .any(|entry| entry.repo_path == repo_path("src/utils.rs"))
        );
    });
}

#[gpui::test]
async fn test_tree_view_reveals_collapsed_parent_on_select_entry_by_path(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "src": {
                "a": {
                    "foo.rs": "fn foo() {}",
                },
                "b": {
                    "bar.rs": "fn bar() {}",
                },
            },
        }),
    )
    .await;

    fs.set_status_for_repo(
        path!("/project/.git").as_ref(),
        &[
            ("src/a/foo.rs", StatusCode::Modified.worktree()),
            ("src/b/bar.rs", StatusCode::Modified.worktree()),
        ],
    );

    let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

    cx.read(|cx| {
        project
            .read(cx)
            .worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .as_local()
            .unwrap()
            .scan_complete()
    })
    .await;

    cx.executor().run_until_parked();

    cx.update(|_window, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.git_panel.get_or_insert_default().tree_view = Some(true);
            })
        });
    });

    let panel = workspace.update_in(cx, GitPanel::new);

    let handle = cx.update_window_entity(&panel, |panel, _, _| {
        std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
    });
    cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
    handle.await;

    let src_key = panel.read_with(cx, |panel, _| {
        panel
            .entries
            .iter()
            .find_map(|entry| match entry {
                GitListEntry::Directory(dir) if dir.key.path == repo_path("src") => {
                    Some(dir.key.clone())
                }
                _ => None,
            })
            .expect("src directory should exist in tree view")
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.toggle_directory(&src_key, window, cx);
    });

    panel.read_with(cx, |panel, _| {
        let state = panel
            .view_mode
            .tree_state()
            .expect("tree view state should exist");
        assert_eq!(state.expanded_dirs.get(&src_key.path).copied(), Some(false));
    });

    let worktree_id = cx.read(|cx| project.read(cx).worktrees(cx).next().unwrap().read(cx).id());
    let project_path = ProjectPath {
        worktree_id,
        path: RelPath::unix("src/a/foo.rs").unwrap().into_arc(),
    };

    panel.update_in(cx, |panel, window, cx| {
        panel.select_entry_by_path(project_path, window, cx);
    });

    panel.read_with(cx, |panel, _| {
        let state = panel
            .view_mode
            .tree_state()
            .expect("tree view state should exist");
        assert_eq!(state.expanded_dirs.get(&src_key.path).copied(), Some(true));

        let selected_ix = panel.selected_entry.expect("selection should be set");
        assert!(state.logical_indices.contains(&selected_ix));

        let selected_entry = panel
            .entries
            .get(selected_ix)
            .and_then(|entry| entry.status_entry())
            .expect("selected entry should be a status entry");
        assert_eq!(selected_entry.repo_path, repo_path("src/a/foo.rs"));
    });
}

#[gpui::test]
async fn test_tree_view_select_next_at_last_visible_collapsed_directory(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "bar": {
                "bar1.py": "print('bar1')",
                "bar2.py": "print('bar2')",
            },
            "foo": {
                "foo1.py": "print('foo1')",
                "foo2.py": "print('foo2')",
            },
            "foobar.py": "print('foobar')",
        }),
    )
    .await;

    fs.set_status_for_repo(
        path!("/project/.git").as_ref(),
        &[
            ("bar/bar1.py", StatusCode::Modified.worktree()),
            ("bar/bar2.py", StatusCode::Modified.worktree()),
            ("foo/foo1.py", StatusCode::Modified.worktree()),
            ("foo/foo2.py", StatusCode::Modified.worktree()),
            ("foobar.py", FileStatus::Untracked),
        ],
    );

    let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

    cx.read(|cx| {
        project
            .read(cx)
            .worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .as_local()
            .unwrap()
            .scan_complete()
    })
    .await;

    cx.executor().run_until_parked();
    cx.update(|_window, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.git_panel.get_or_insert_default().tree_view = Some(true);
            })
        });
    });

    let panel = workspace.update_in(cx, GitPanel::new);
    let handle = cx.update_window_entity(&panel, |panel, _, _| {
        std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
    });

    cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
    handle.await;

    let foo_key = panel.read_with(cx, |panel, _| {
        panel
            .entries
            .iter()
            .find_map(|entry| match entry {
                GitListEntry::Directory(dir) if dir.key.path == repo_path("foo") => {
                    Some(dir.key.clone())
                }
                _ => None,
            })
            .expect("foo directory should exist in tree view")
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.toggle_directory(&foo_key, window, cx);
    });

    let foo_idx = panel.read_with(cx, |panel, _| {
        let state = panel
            .view_mode
            .tree_state()
            .expect("tree view state should exist");
        assert_eq!(state.expanded_dirs.get(&foo_key.path).copied(), Some(false));

        let foo_idx = panel
            .entries
            .iter()
            .enumerate()
            .find_map(|(index, entry)| match entry {
                GitListEntry::Directory(dir) if dir.key.path == repo_path("foo") => Some(index),
                _ => None,
            })
            .expect("foo directory should exist in tree view");

        let foo_logical_idx = state
            .logical_indices
            .iter()
            .position(|&index| index == foo_idx)
            .expect("foo directory should be visible");
        let next_logical_idx = state.logical_indices[foo_logical_idx + 1];
        assert!(matches!(
            panel.entries.get(next_logical_idx),
            Some(GitListEntry::Header(GitHeaderEntry {
                header: Section::New
            }))
        ));

        foo_idx
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.selected_entry = Some(foo_idx);
        panel.select_next(&menu::SelectNext, window, cx);
    });

    panel.read_with(cx, |panel, _| {
        let selected_idx = panel.selected_entry.expect("selection should be set");
        let selected_entry = panel
            .entries
            .get(selected_idx)
            .and_then(|entry| entry.status_entry())
            .expect("selected entry should be a status entry");
        assert_eq!(selected_entry.repo_path, repo_path("foobar.py"));
    });
}

use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
#[ignore]
async fn test_ignored_dirs_events(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    const IGNORE_RULE: &str = "**/target";

    let root = TempTree::new(json!({
        "project": {
            "src": {
                "main.rs": "fn main() {}"
            },
            "target": {
                "debug": {
                    "important_text.txt": "important text",
                },
            },
            ".gitignore": IGNORE_RULE
        },

    }));
    let root_path = root.path();

    // Set up git repository before creating the worktree.
    let work_dir = root.path().join("project");
    let repo = git_init(work_dir.as_path());
    git_add("src/main.rs", &repo);
    git_add(".gitignore", &repo);
    git_commit("Initial commit", &repo);

    let project = Project::test(Arc::new(RealFs::new(None, cx.executor())), [root_path], cx).await;
    let repository_updates = Arc::new(Mutex::new(Vec::new()));
    let project_events = Arc::new(Mutex::new(Vec::new()));
    project.update(cx, |project, cx| {
        let repo_events = repository_updates.clone();
        cx.subscribe(project.git_store(), move |_, _, e, _| {
            if let GitStoreEvent::RepositoryUpdated(_, e, _) = e {
                repo_events.lock().push(e.clone());
            }
        })
        .detach();
        let project_events = project_events.clone();
        cx.subscribe_self(move |_, e, _| {
            if let Event::WorktreeUpdatedEntries(_, updates) = e {
                project_events.lock().extend(
                    updates
                        .iter()
                        .map(|(path, _, change)| (path.as_unix_str().to_string(), *change))
                        .filter(|(path, _)| path != "fs-event-sentinel"),
                );
            }
        })
        .detach();
    });

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    tree.update(cx, |tree, cx| {
        tree.load_file(rel_path("project/target/debug/important_text.txt"), cx)
    })
    .await
    .unwrap();
    tree.update(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_ignored))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path("project/"), false),
                (rel_path("project/.gitignore"), false),
                (rel_path("project/src"), false),
                (rel_path("project/src/main.rs"), false),
                (rel_path("project/target"), true),
                (rel_path("project/target/debug"), true),
                (rel_path("project/target/debug/important_text.txt"), true),
            ]
        );
    });

    assert_eq!(
        repository_updates.lock().drain(..).collect::<Vec<_>>(),
        vec![RepositoryEvent::StatusesChanged,],
        "Initial worktree scan should produce a repo update event"
    );
    assert_eq!(
        project_events.lock().drain(..).collect::<Vec<_>>(),
        vec![
            ("project/target".to_string(), PathChange::Loaded),
            ("project/target/debug".to_string(), PathChange::Loaded),
            (
                "project/target/debug/important_text.txt".to_string(),
                PathChange::Loaded
            ),
        ],
        "Initial project changes should show that all not-ignored and all opened files are loaded"
    );

    let deps_dir = work_dir.join("target").join("debug").join("deps");
    std::fs::create_dir_all(&deps_dir).unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();
    std::fs::write(deps_dir.join("aa.tmp"), "something tmp").unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();
    std::fs::remove_dir_all(&deps_dir).unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    tree.update(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_ignored))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path("project/"), false),
                (rel_path("project/.gitignore"), false),
                (rel_path("project/src"), false),
                (rel_path("project/src/main.rs"), false),
                (rel_path("project/target"), true),
                (rel_path("project/target/debug"), true),
                (rel_path("project/target/debug/important_text.txt"), true),
            ],
            "No stray temp files should be left after the flycheck changes"
        );
    });

    assert_eq!(
        repository_updates
            .lock()
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        Vec::new(),
        "No further RepositoryUpdated events should happen, as only ignored dirs' contents was changed",
    );
    assert_eq!(
        project_events.lock().as_slice(),
        vec![
            ("project/target/debug/deps".to_string(), PathChange::Added),
            ("project/target/debug/deps".to_string(), PathChange::Removed),
        ],
        "Due to `debug` directory being tracked, it should get updates for entries inside it.
        No updates for more nested directories should happen as those are ignored",
    );
}

// todo(jk): turning this test off until we rework it in such a way so that it is not so susceptible
// to different timings/ordering of events.
#[ignore]
#[gpui::test]
async fn test_odd_events_for_ignored_dirs(
    executor: BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            ".git": {},
            ".gitignore": "**/target/",
            "src": {
                "main.rs": "fn main() {}",
            },
            "target": {
                "debug": {
                    "foo.txt": "foo",
                    "deps": {}
                }
            }
        }),
    )
    .await;
    fs.set_head_and_index_for_repo(
        path!("/root/.git").as_ref(),
        &[
            (".gitignore", "**/target/".into()),
            ("src/main.rs", "fn main() {}".into()),
        ],
    );

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let repository_updates = Arc::new(Mutex::new(Vec::new()));
    let project_events = Arc::new(Mutex::new(Vec::new()));
    project.update(cx, |project, cx| {
        let repository_updates = repository_updates.clone();
        cx.subscribe(project.git_store(), move |_, _, e, _| {
            if let GitStoreEvent::RepositoryUpdated(_, e, _) = e {
                repository_updates.lock().push(e.clone());
            }
        })
        .detach();
        let project_events = project_events.clone();
        cx.subscribe_self(move |_, e, _| {
            if let Event::WorktreeUpdatedEntries(_, updates) = e {
                project_events.lock().extend(
                    updates
                        .iter()
                        .map(|(path, _, change)| (path.as_unix_str().to_string(), *change))
                        .filter(|(path, _)| path != "fs-event-sentinel"),
                );
            }
        })
        .detach();
    });

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.update(cx, |tree, cx| {
        tree.load_file(rel_path("target/debug/foo.txt"), cx)
    })
    .await
    .unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();
    tree.update(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_ignored))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path(".gitignore"), false),
                (rel_path("src"), false),
                (rel_path("src/main.rs"), false),
                (rel_path("target"), true),
                (rel_path("target/debug"), true),
                (rel_path("target/debug/deps"), true),
                (rel_path("target/debug/foo.txt"), true),
            ]
        );
    });

    assert_eq!(
        repository_updates.lock().drain(..).collect::<Vec<_>>(),
        vec![
            RepositoryEvent::HeadChanged,
            RepositoryEvent::StatusesChanged,
            RepositoryEvent::StatusesChanged,
        ],
        "Initial worktree scan should produce a repo update event"
    );
    assert_eq!(
        project_events.lock().drain(..).collect::<Vec<_>>(),
        vec![
            ("target".to_string(), PathChange::Loaded),
            ("target/debug".to_string(), PathChange::Loaded),
            ("target/debug/deps".to_string(), PathChange::Loaded),
            ("target/debug/foo.txt".to_string(), PathChange::Loaded),
        ],
        "All non-ignored entries and all opened firs should be getting a project event",
    );

    // Emulate a flycheck spawn: it emits a `INODE_META_MOD`-flagged FS event on target/debug/deps, then creates and removes temp files inside.
    // This may happen multiple times during a single flycheck, but once is enough for testing.
    fs.emit_fs_event("/root/target/debug/deps", None);
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    assert_eq!(
        repository_updates
            .lock()
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        Vec::new(),
        "No further RepositoryUpdated events should happen, as only ignored dirs received FS events",
    );
    assert_eq!(
        project_events.lock().as_slice(),
        Vec::new(),
        "No further project events should happen, as only ignored dirs received FS events",
    );
}

#[gpui::test]
async fn test_repos_in_invisible_worktrees(
    executor: BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                ".git": {},
                "dep1": {
                    ".git": {},
                    "src": {
                        "a.txt": "",
                    },
                },
                "b.txt": "",
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root/dir1/dep1").as_ref()], cx).await;
    let _visible_worktree =
        project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let repos = project.read_with(cx, |project, cx| {
        project
            .repositories(cx)
            .values()
            .map(|repo| repo.read(cx).work_directory_abs_path.clone())
            .collect::<Vec<_>>()
    });
    pretty_assertions::assert_eq!(repos, [Path::new(path!("/root/dir1/dep1")).into()]);

    let (_invisible_worktree, _) = project
        .update(cx, |project, cx| {
            project.worktree_store().update(cx, |worktree_store, cx| {
                worktree_store.find_or_create_worktree(path!("/root/dir1/b.txt"), false, cx)
            })
        })
        .await
        .expect("failed to create worktree");
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let repos = project.read_with(cx, |project, cx| {
        project
            .repositories(cx)
            .values()
            .map(|repo| repo.read(cx).work_directory_abs_path.clone())
            .collect::<Vec<_>>()
    });
    pretty_assertions::assert_eq!(repos, [Path::new(path!("/root/dir1/dep1")).into()]);
}

#[gpui::test(iterations = 10)]
async fn test_rescan_with_gitignore(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions = Some(Vec::new());
            });
        });
    });
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".gitignore": "ancestor-ignored-file1\nancestor-ignored-file2\n",
            "tree": {
                ".git": {},
                ".gitignore": "ignored-dir\n",
                "tracked-dir": {
                    "tracked-file1": "",
                    "ancestor-ignored-file1": "",
                },
                "ignored-dir": {
                    "ignored-file1": ""
                }
            }
        }),
    )
    .await;
    fs.set_head_and_index_for_repo(
        path!("/root/tree/.git").as_ref(),
        &[
            (".gitignore", "ignored-dir\n".into()),
            ("tracked-dir/tracked-file1", "".into()),
        ],
    );

    let project = Project::test(fs.clone(), [path!("/root/tree").as_ref()], cx).await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .manually_refresh_entries_for_paths(vec![rel_path("ignored-dir").into()])
    })
    .recv()
    .await;

    cx.read(|cx| {
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "tracked-dir/tracked-file1",
            None,
            false,
        );
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "tracked-dir/ancestor-ignored-file1",
            None,
            false,
        );
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "ignored-dir/ignored-file1",
            None,
            true,
        );
    });

    fs.create_file(
        path!("/root/tree/tracked-dir/tracked-file2").as_ref(),
        Default::default(),
    )
    .await
    .unwrap();
    fs.set_index_for_repo(
        path!("/root/tree/.git").as_ref(),
        &[
            (".gitignore", "ignored-dir\n".into()),
            ("tracked-dir/tracked-file1", "".into()),
            ("tracked-dir/tracked-file2", "".into()),
        ],
    );
    fs.create_file(
        path!("/root/tree/tracked-dir/ancestor-ignored-file2").as_ref(),
        Default::default(),
    )
    .await
    .unwrap();
    fs.create_file(
        path!("/root/tree/ignored-dir/ignored-file2").as_ref(),
        Default::default(),
    )
    .await
    .unwrap();

    cx.executor().run_until_parked();
    cx.read(|cx| {
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "tracked-dir/tracked-file2",
            Some(StatusCode::Added),
            false,
        );
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "tracked-dir/ancestor-ignored-file2",
            None,
            false,
        );
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "ignored-dir/ignored-file2",
            None,
            true,
        );
        assert!(
            tree.read(cx)
                .entry_for_path(&rel_path(".git"))
                .unwrap()
                .is_ignored
        );
    });
}

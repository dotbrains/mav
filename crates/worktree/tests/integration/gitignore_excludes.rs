use super::*;

#[gpui::test]
async fn test_private_single_file_worktree(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree("/", json!({".env": "PRIVATE=secret\n"}))
        .await;
    let tree = Worktree::local(
        Path::new("/.env"),
        true,
        fs.clone(),
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();
    cx.read(|cx| tree.read(cx).as_local().unwrap().scan_complete())
        .await;
    tree.read_with(cx, |tree, _| {
        let entry = tree.entry_for_path(rel_path("")).unwrap();
        assert!(entry.is_private);
    });
}

#[gpui::test]
async fn test_repository_above_root(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            ".git": {},
            "subproject": {
                "a.txt": "A"
            }
        }),
    )
    .await;
    let worktree = Worktree::local(
        path!("/root/subproject").as_ref(),
        true,
        fs.clone(),
        Arc::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();
    worktree
        .update(cx, |worktree, _| {
            worktree.as_local().unwrap().scan_complete()
        })
        .await;
    cx.run_until_parked();
    let repos = worktree.update(cx, |worktree, _| {
        worktree.as_local().unwrap().repositories()
    });
    pretty_assertions::assert_eq!(repos, [Path::new(path!("/root")).into()]);

    fs.touch_path(path!("/root/subproject")).await;
    worktree
        .update(cx, |worktree, _| {
            worktree.as_local().unwrap().scan_complete()
        })
        .await;
    cx.run_until_parked();

    let repos = worktree.update(cx, |worktree, _| {
        worktree.as_local().unwrap().repositories()
    });
    pretty_assertions::assert_eq!(repos, [Path::new(path!("/root")).into()]);
}

#[gpui::test]
async fn test_global_gitignore(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let home = paths::home_dir();
    let fs = FakeFs::new(executor);
    fs.insert_tree(
        home,
        json!({
            ".config": {
                "git": {
                    "ignore": "foo\n/bar\nbaz\n"
                }
            },
            "project": {
                ".git": {},
                ".gitignore": "!baz",
                "foo": "",
                "bar": "",
                "sub": {
                    "bar": "",
                },
                "subrepo": {
                    ".git": {},
                    "bar": ""
                },
                "baz": ""
            }
        }),
    )
    .await;
    let worktree = Worktree::local(
        home.join("project"),
        true,
        fs.clone(),
        Arc::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();
    worktree
        .update(cx, |worktree, _| {
            worktree.as_local().unwrap().scan_complete()
        })
        .await;
    cx.run_until_parked();

    // .gitignore overrides excludesFile, and anchored paths in excludesFile are resolved
    // relative to the nearest containing repository
    worktree.update(cx, |worktree, _cx| {
        check_worktree_entries(
            worktree,
            WorktreeExpectations {
                ignored_paths: &["foo", "bar", "subrepo/bar"],
                tracked_paths: &["sub/bar", "baz"],
                ..Default::default()
            },
        );
    });

    // Ignore statuses are updated when excludesFile changes
    fs.write(
        &home.join(".config").join("git").join("ignore"),
        "/bar\nbaz\n".as_bytes(),
    )
    .await
    .unwrap();
    worktree
        .update(cx, |worktree, _| {
            worktree.as_local().unwrap().scan_complete()
        })
        .await;
    cx.run_until_parked();

    worktree.update(cx, |worktree, _cx| {
        check_worktree_entries(
            worktree,
            WorktreeExpectations {
                ignored_paths: &["bar", "subrepo/bar"],
                tracked_paths: &["foo", "sub/bar", "baz"],
                ..Default::default()
            },
        );
    });

    // Statuses are updated when .git added/removed
    fs.remove_dir(
        &home.join("project").join("subrepo").join(".git"),
        RemoveOptions {
            recursive: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    worktree
        .update(cx, |worktree, _| {
            worktree.as_local().unwrap().scan_complete()
        })
        .await;
    cx.run_until_parked();

    worktree.update(cx, |worktree, _cx| {
        check_worktree_entries(
            worktree,
            WorktreeExpectations {
                ignored_paths: &["bar"],
                tracked_paths: &["foo", "sub/bar", "baz", "subrepo/bar"],
                ..Default::default()
            },
        );
    });
}

#[gpui::test]
async fn test_repo_exclude_in_worktree(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor);

    fs.insert_tree(
        path!("/repo"),
        json!({
            ".git": {
                "info": {
                    "exclude": ".env.*"
                },
                "worktrees": {
                    "my-worktree": {
                        "commondir": "../.."
                    }
                }
            }
        }),
    )
    .await;

    fs.insert_tree(
        path!("/worktree"),
        json!({
            // .git is pointing to the repo
            ".git": "gitdir: /repo/.git/worktrees/my-worktree",
            ".env.local": "secret=1234",
            "not-ignored.txt": "",
        }),
    )
    .await;

    let worktree = Worktree::local(
        path!("/worktree").as_ref(),
        true,
        fs.clone(),
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    worktree
        .update(cx, |worktree, _| {
            worktree.as_local().unwrap().scan_complete()
        })
        .await;
    cx.run_until_parked();

    // .env.local should be ignored via info/exclude from the repo's exclude
    worktree.update(cx, |worktree, _cx| {
        check_worktree_entries(
            worktree,
            WorktreeExpectations {
                ignored_paths: &[".env.local"],
                tracked_paths: &["not-ignored.txt"],
                ..Default::default()
            },
        );
    });
}

#[gpui::test]
async fn test_repo_exclude(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    let project_dir = Path::new(path!("/project"));
    fs.insert_tree(
        project_dir,
        json!({
            ".git": {
                "info": {
                    "exclude": ".env.*"
                }
            },
            ".env.example": "secret=xxxx",
            ".env.local": "secret=1234",
            ".gitignore": "!.env.example",
            "README.md": "# Repo Exclude",
            "src": {
                "main.rs": "fn main() {}",
            },
        }),
    )
    .await;

    let worktree = Worktree::local(
        project_dir,
        true,
        fs.clone(),
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();
    worktree
        .update(cx, |worktree, _| {
            worktree.as_local().unwrap().scan_complete()
        })
        .await;
    cx.run_until_parked();

    // .gitignore overrides .git/info/exclude
    worktree.update(cx, |worktree, _cx| {
        check_worktree_entries(
            worktree,
            WorktreeExpectations {
                ignored_paths: &[".env.local"],
                tracked_paths: &[".env.example", "README.md", "src/main.rs"],
                ..Default::default()
            },
        );
    });

    // Ignore statuses are updated when .git/info/exclude file changes
    fs.write(
        &project_dir.join(DOT_GIT).join(REPO_EXCLUDE),
        ".env.example".as_bytes(),
    )
    .await
    .unwrap();
    worktree
        .update(cx, |worktree, _| {
            worktree.as_local().unwrap().scan_complete()
        })
        .await;
    cx.run_until_parked();

    worktree.update(cx, |worktree, _cx| {
        check_worktree_entries(
            worktree,
            WorktreeExpectations {
                tracked_paths: &[".env.example", ".env.local", "README.md", "src/main.rs"],
                ..Default::default()
            },
        );
    });
}

#[gpui::test]
async fn test_repo_exclude_anchored_pattern(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    let project_dir = Path::new(path!("/project"));
    fs.insert_tree(
        project_dir,
        json!({
            ".git": {
                "info": {
                    "exclude": "vendor/cache"
                }
            },
            "vendor": {
                "cache": {
                    "blob.bin": "",
                },
                "keep.txt": "",
            },
            "elsewhere": {
                "vendor": {
                    "cache": {
                        "blob.bin": "",
                    },
                },
            },
        }),
    )
    .await;

    let worktree = Worktree::local(
        project_dir,
        true,
        fs.clone(),
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();
    worktree
        .update(cx, |worktree, _| {
            worktree.as_local().unwrap().scan_complete()
        })
        .await;
    cx.run_until_parked();

    // An anchored pattern (containing a `/`) is matched relative to the work
    // tree root, so only the top-level `vendor/cache` is ignored.
    worktree.update(cx, |worktree, _cx| {
        check_worktree_entries(
            worktree,
            WorktreeExpectations {
                ignored_paths: &["vendor/cache"],
                tracked_paths: &["vendor/keep.txt", "elsewhere/vendor/cache"],
                ..Default::default()
            },
        );
    });
}

use super::*;

#[gpui::test]
async fn test_remote_git_diffs(cx: &mut TestAppContext, server_cx: &mut TestAppContext) {
    let text_2 = "
        fn one() -> usize {
            1
        }
    "
    .unindent();
    let text_1 = "
        fn one() -> usize {
            0
        }
    "
    .unindent();

    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        "/code",
        json!({
            "project1": {
                ".git": {},
                "src": {
                    "lib.rs": text_2
                },
                "README.md": "# project 1",
            },
        }),
    )
    .await;
    fs.set_index_for_repo(
        Path::new("/code/project1/.git"),
        &[("src/lib.rs", text_1.clone())],
    );
    fs.set_head_for_repo(
        Path::new("/code/project1/.git"),
        &[("src/lib.rs", text_1.clone())],
        "deadbeef",
    );

    let (project, _headless) = init_test(&fs, cx, server_cx).await;
    let (worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/code/project1", true, cx)
        })
        .await
        .unwrap();
    let worktree_id = cx.update(|cx| worktree.read(cx).id());
    cx.executor().run_until_parked();

    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("src/lib.rs")), cx)
        })
        .await
        .unwrap();
    let diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    diff.read_with(cx, |diff, cx| {
        assert_eq!(diff.base_text_string(cx).unwrap(), text_1);
        assert_eq!(
            diff.secondary_diff()
                .unwrap()
                .read(cx)
                .base_text_string(cx)
                .unwrap(),
            text_1
        );
    });

    // stage the current buffer's contents
    fs.set_index_for_repo(
        Path::new("/code/project1/.git"),
        &[("src/lib.rs", text_2.clone())],
    );

    cx.executor().run_until_parked();
    diff.read_with(cx, |diff, cx| {
        assert_eq!(diff.base_text_string(cx).unwrap(), text_1);
        assert_eq!(
            diff.secondary_diff()
                .unwrap()
                .read(cx)
                .base_text_string(cx)
                .unwrap(),
            text_2
        );
    });

    // commit the current buffer's contents
    fs.set_head_for_repo(
        Path::new("/code/project1/.git"),
        &[("src/lib.rs", text_2.clone())],
        "deadbeef",
    );

    cx.executor().run_until_parked();
    diff.read_with(cx, |diff, cx| {
        assert_eq!(diff.base_text_string(cx).unwrap(), text_2);
        assert_eq!(
            diff.secondary_diff()
                .unwrap()
                .read(cx)
                .base_text_string(cx)
                .unwrap(),
            text_2
        );
    });
}

#[gpui::test]
async fn test_remote_git_diffs_when_recv_update_repository_delay(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        editor::init(cx);
    });

    use editor::Editor;
    use gpui::VisualContext;
    let text_2 = "
        fn one() -> usize {
            1
        }
    "
    .unindent();
    let text_1 = "
        fn one() -> usize {
            0
        }
    "
    .unindent();

    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        path!("/code"),
        json!({
            "project1": {
                "src": {
                    "lib.rs": text_2
                },
                "README.md": "# project 1",
            },
        }),
    )
    .await;

    let (project, _headless) = init_test(&fs, cx, server_cx).await;
    let (worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/code/project1"), true, cx)
        })
        .await
        .unwrap();
    let worktree_id = cx.update(|cx| worktree.read(cx).id());
    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("src/lib.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_id = cx.update(|cx| buffer.read(cx).remote_id());

    let cx = cx.add_empty_window();
    let editor = cx.new_window_entity(|window, cx| {
        Editor::for_buffer(buffer, Some(project.clone()), window, cx)
    });

    // Remote server will send proto::UpdateRepository after the instance of Editor create.
    fs.insert_tree(
        path!("/code"),
        json!({
            "project1": {
                ".git": {},
            },
        }),
    )
    .await;

    fs.set_index_for_repo(
        Path::new(path!("/code/project1/.git")),
        &[("src/lib.rs", text_1.clone())],
    );
    fs.set_head_for_repo(
        Path::new(path!("/code/project1/.git")),
        &[("src/lib.rs", text_1.clone())],
        "sha",
    );

    cx.executor().run_until_parked();
    let diff = editor
        .read_with(cx, |editor, cx| {
            editor
                .buffer()
                .read_with(cx, |buffer, _| buffer.diff_for(buffer_id))
        })
        .unwrap();

    diff.read_with(cx, |diff, cx| {
        assert_eq!(diff.base_text_string(cx).unwrap(), text_1);
        assert_eq!(
            diff.secondary_diff()
                .unwrap()
                .read(cx)
                .base_text_string(cx)
                .unwrap(),
            text_1
        );
    });

    // stage the current buffer's contents
    fs.set_index_for_repo(
        Path::new(path!("/code/project1/.git")),
        &[("src/lib.rs", text_2.clone())],
    );

    cx.executor().run_until_parked();
    diff.read_with(cx, |diff, cx| {
        assert_eq!(diff.base_text_string(cx).unwrap(), text_1);
        assert_eq!(
            diff.secondary_diff()
                .unwrap()
                .read(cx)
                .base_text_string(cx)
                .unwrap(),
            text_2
        );
    });

    // commit the current buffer's contents
    fs.set_head_for_repo(
        Path::new(path!("/code/project1/.git")),
        &[("src/lib.rs", text_2.clone())],
        "sha",
    );

    cx.executor().run_until_parked();
    diff.read_with(cx, |diff, cx| {
        assert_eq!(diff.base_text_string(cx).unwrap(), text_2);
        assert_eq!(
            diff.secondary_diff()
                .unwrap()
                .read(cx)
                .base_text_string(cx)
                .unwrap(),
            text_2
        );
    });
}

#[gpui::test]
async fn test_remote_git_branches(cx: &mut TestAppContext, server_cx: &mut TestAppContext) {
    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        path!("/code"),
        json!({
            "project1": {
                ".git": {},
                "README.md": "# project 1",
            },
        }),
    )
    .await;

    let (project, headless_project) = init_test(&fs, cx, server_cx).await;
    let branches = ["main", "dev", "feature-1"];
    let branches_set = branches
        .iter()
        .map(ToString::to_string)
        .collect::<HashSet<_>>();
    fs.insert_branches(Path::new(path!("/code/project1/.git")), &branches);

    let (_worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/code/project1"), true, cx)
        })
        .await
        .unwrap();
    // Give the worktree a bit of time to index the file system
    cx.run_until_parked();

    let repository = project.update(cx, |project, cx| project.active_repository(cx).unwrap());

    let remote_branches = repository
        .update(cx, |repository, _| repository.branches())
        .await
        .unwrap()
        .unwrap()
        .branches;

    let new_branch = branches[2];

    let remote_branches = remote_branches
        .into_iter()
        .map(|branch| branch.name().to_string())
        .collect::<HashSet<_>>();

    assert_eq!(&remote_branches, &branches_set);

    cx.update(|cx| {
        repository.update(cx, |repository, _cx| {
            repository.change_branch(new_branch.to_string())
        })
    })
    .await
    .unwrap()
    .unwrap();

    cx.run_until_parked();

    let server_branch = server_cx.update(|cx| {
        headless_project.update(cx, |headless_project, cx| {
            headless_project.git_store.update(cx, |git_store, cx| {
                git_store
                    .repositories()
                    .values()
                    .next()
                    .unwrap()
                    .read(cx)
                    .branch
                    .as_ref()
                    .unwrap()
                    .clone()
            })
        })
    });

    assert_eq!(server_branch.name(), branches[2]);

    // Also try creating a new branch
    cx.update(|cx| {
        repository.update(cx, |repo, _cx| {
            repo.create_branch("totally-new-branch".to_string(), None)
        })
    })
    .await
    .unwrap()
    .unwrap();

    cx.update(|cx| {
        repository.update(cx, |repo, _cx| {
            repo.change_branch("totally-new-branch".to_string())
        })
    })
    .await
    .unwrap()
    .unwrap();

    cx.run_until_parked();

    let server_branch = server_cx.update(|cx| {
        headless_project.update(cx, |headless_project, cx| {
            headless_project.git_store.update(cx, |git_store, cx| {
                git_store
                    .repositories()
                    .values()
                    .next()
                    .unwrap()
                    .read(cx)
                    .branch
                    .as_ref()
                    .unwrap()
                    .clone()
            })
        })
    });

    assert_eq!(server_branch.name(), "totally-new-branch");
}

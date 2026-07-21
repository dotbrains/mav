use super::*;

#[gpui::test]
async fn test_amend_commit_message_handling(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "project": {
                ".git": {},
                "src": {
                    "main.rs": "fn main() {}"
                }
            }
        }),
    )
    .await;

    fs.set_status_for_repo(
        Path::new(path!("/root/project/.git")),
        &[("src/main.rs", StatusCode::Modified.worktree())],
    );

    let project = Project::test(fs.clone(), [Path::new(path!("/root/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

    let panel = workspace.update_in(cx, GitPanel::new);

    // Test: User has commit message, enables amend (saves message), then disables (restores message)
    panel.update(cx, |panel, cx| {
        panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
            let start = buffer.anchor_before(0);
            let end = buffer.anchor_after(buffer.len());
            buffer.edit([(start..end, "Initial commit message")], None, cx);
        });

        panel.set_amend_pending(true, cx);
        assert!(panel.original_commit_message.is_some());

        panel.set_amend_pending(false, cx);
        let current_message = panel.commit_message_buffer(cx).read(cx).text();
        assert_eq!(current_message, "Initial commit message");
        assert!(panel.original_commit_message.is_none());
    });

    // Test: User has empty commit message, enables amend, then disables (clears message)
    panel.update(cx, |panel, cx| {
        panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
            let start = buffer.anchor_before(0);
            let end = buffer.anchor_after(buffer.len());
            buffer.edit([(start..end, "")], None, cx);
        });

        panel.set_amend_pending(true, cx);
        assert!(panel.original_commit_message.is_none());

        panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
            let start = buffer.anchor_before(0);
            let end = buffer.anchor_after(buffer.len());
            buffer.edit([(start..end, "Previous commit message")], None, cx);
        });

        panel.set_amend_pending(false, cx);
        let current_message = panel.commit_message_buffer(cx).read(cx).text();
        assert_eq!(current_message, "");
    });
}

#[gpui::test]
async fn test_commit_message_restored_after_reconnect(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "project-a": {
                ".git": {},
                "src": {
                    "main.rs": "fn main() {}"
                }
            },
            "project-b": {
                ".git": {},
                "src": {
                    "main.rs": "fn main() {}"
                }
            }
        }),
    )
    .await;

    fs.set_status_for_repo(
        Path::new(path!("/root/project-a/.git")),
        &[("src/main.rs", StatusCode::Modified.worktree())],
    );
    fs.set_status_for_repo(
        Path::new(path!("/root/project-b/.git")),
        &[("src/main.rs", StatusCode::Modified.worktree())],
    );

    let project = Project::test(
        fs.clone(),
        [
            Path::new(path!("/root/project-a")),
            Path::new(path!("/root/project-b")),
        ],
        cx,
    )
    .await;
    let (repository_a, repository_b) = project.read_with(cx, |project, cx| {
        let git_store = project.git_store().clone();
        let mut repository_a = None;
        let mut repository_b = None;
        for repository in git_store.read(cx).repositories().values() {
            let work_directory_abs_path = &repository.read(cx).work_directory_abs_path;
            if work_directory_abs_path.as_ref() == Path::new(path!("/root/project-a")) {
                repository_a = Some(repository.clone());
            } else if work_directory_abs_path.as_ref() == Path::new(path!("/root/project-b")) {
                repository_b = Some(repository.clone());
            }
        }
        (
            repository_a.expect("should have repository for project-a"),
            repository_b.expect("should have repository for project-b"),
        )
    });
    repository_a.update(cx, |repository, cx| repository.set_as_active_repository(cx));

    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

    register_git_commit_language(&project, cx);
    let panel = workspace.update_in(cx, GitPanel::new);
    cx.run_until_parked();

    let message_a = "Restore repository A message";
    panel.update(cx, |panel, cx| {
        panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
            let start = buffer.anchor_before(0);
            let end = buffer.anchor_after(buffer.len());
            buffer.edit([(start..end, message_a)], None, cx);
        });
    });

    repository_b.update(cx, |repository, cx| repository.set_as_active_repository(cx));
    cx.run_until_parked();

    let message_b = "Restore repository B message";
    let serialized_panel = panel.update(cx, |panel, cx| {
        panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
            let start = buffer.anchor_before(0);
            let end = buffer.anchor_after(buffer.len());
            buffer.edit([(start..end, message_b)], None, cx);
        });

        SerializedGitPanel {
            signoff_enabled: false,
            commit_messages: panel.serialized_commit_messages(cx),
        }
    });

    for repository in [&repository_a, &repository_b] {
        let buffer = repository.read_with(cx, |repository, _| {
            repository
                .commit_message_buffer()
                .expect("repository commit message buffer should be open")
                .clone()
        });
        buffer.update(cx, |buffer, cx| {
            let start = buffer.anchor_before(0);
            let end = buffer.anchor_after(buffer.len());
            buffer.edit([(start..end, "")], None, cx);
        });
    }

    let restored_panel = workspace.update_in(cx, |workspace, window, cx| {
        GitPanel::new_with_serialized_panel(workspace, Some(serialized_panel), window, cx)
    });
    cx.run_until_parked();

    restored_panel.read_with(cx, |panel, cx| {
        assert_eq!(panel.commit_message_buffer(cx).read(cx).text(), message_b);
    });

    repository_a.update(cx, |repository, cx| repository.set_as_active_repository(cx));
    cx.run_until_parked();

    restored_panel.read_with(cx, |panel, cx| {
        assert_eq!(panel.commit_message_buffer(cx).read(cx).text(), message_a);
    });

    restored_panel.update(cx, |panel, cx| {
        panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
            let start = buffer.anchor_before(0);
            let end = buffer.anchor_after(buffer.len());
            buffer.edit([(start..end, "")], None, cx);
        });
    });

    let mismatched_serialized_panel = SerializedGitPanel {
        signoff_enabled: false,
        commit_messages: BTreeMap::from_iter([(
            path!("/root/other-project").to_string(),
            SerializedCommitMessage {
                message: Some(message_a.to_string()),
                original_message: None,
                ..Default::default()
            },
        )]),
    };
    let mismatched_panel = workspace.update_in(cx, |workspace, window, cx| {
        GitPanel::new_with_serialized_panel(
            workspace,
            Some(mismatched_serialized_panel),
            window,
            cx,
        )
    });
    cx.run_until_parked();

    mismatched_panel.read_with(cx, |panel, cx| {
        // The draft is not restored because the serialized work directory
        // does not match the active repository, so it cannot leak across
        // repositories.
        assert_eq!(panel.commit_message_buffer(cx).read(cx).text(), "");
    });
}

#[gpui::test]
async fn test_amend_state_is_per_repository(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "project-a": {
                ".git": {},
                "src": {
                    "main.rs": "fn main() {}"
                }
            },
            "project-b": {
                ".git": {},
                "src": {
                    "main.rs": "fn main() {}"
                }
            }
        }),
    )
    .await;

    fs.set_status_for_repo(
        Path::new(path!("/root/project-a/.git")),
        &[("src/main.rs", StatusCode::Modified.worktree())],
    );
    fs.set_status_for_repo(
        Path::new(path!("/root/project-b/.git")),
        &[("src/main.rs", StatusCode::Modified.worktree())],
    );

    let project = Project::test(
        fs.clone(),
        [
            Path::new(path!("/root/project-a")),
            Path::new(path!("/root/project-b")),
        ],
        cx,
    )
    .await;
    let (repository_a, repository_b) = project.read_with(cx, |project, cx| {
        let git_store = project.git_store().clone();
        let mut repository_a = None;
        let mut repository_b = None;
        for repository in git_store.read(cx).repositories().values() {
            let work_directory_abs_path = &repository.read(cx).work_directory_abs_path;
            if work_directory_abs_path.as_ref() == Path::new(path!("/root/project-a")) {
                repository_a = Some(repository.clone());
            } else if work_directory_abs_path.as_ref() == Path::new(path!("/root/project-b")) {
                repository_b = Some(repository.clone());
            }
        }
        (
            repository_a.expect("should have repository for project-a"),
            repository_b.expect("should have repository for project-b"),
        )
    });
    repository_a.update(cx, |repository, cx| repository.set_as_active_repository(cx));

    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

    register_git_commit_language(&project, cx);
    let panel = workspace.update_in(cx, GitPanel::new);
    cx.run_until_parked();

    // Enter an amend on repository A, then simulate the amend flow loading
    // the last commit message into the editor.
    panel.update(cx, |panel, cx| {
        panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
            let start = buffer.anchor_before(0);
            let end = buffer.anchor_after(buffer.len());
            buffer.edit([(start..end, "Draft for A")], None, cx);
        });
        panel.set_amend_pending(true, cx);
        panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
            let start = buffer.anchor_before(0);
            let end = buffer.anchor_after(buffer.len());
            buffer.edit([(start..end, "Amended message")], None, cx);
        });
        assert!(panel.amend_pending());
    });

    // Switching the active repository away exits the amend state instead of
    // carrying it over to repository B.
    repository_b.update(cx, |repository, cx| repository.set_as_active_repository(cx));
    cx.run_until_parked();

    panel.update(cx, |panel, cx| {
        assert!(!panel.amend_pending());
        // Only the active repository may serialize a pending amend, and we
        // just left repository A's amend, so nothing is left pending.
        let serialized = panel.serialized_commit_messages(cx);
        assert!(serialized.values().all(|message| !message.amend_pending));
    });

    // Repository A's pre-amend draft is restored, discarding the amend edit.
    let buffer_a = repository_a.read_with(cx, |repository, _| {
        repository
            .commit_message_buffer()
            .expect("repository commit message buffer should be open")
            .clone()
    });
    buffer_a.read_with(cx, |buffer, _| {
        assert_eq!(buffer.text(), "Draft for A");
    });
}

#[gpui::test]
async fn test_amend(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "project": {
                ".git": {},
                "src": {
                    "main.rs": "fn main() {}"
                }
            }
        }),
    )
    .await;

    fs.set_status_for_repo(
        Path::new(path!("/root/project/.git")),
        &[("src/main.rs", StatusCode::Modified.worktree())],
    );

    let project = Project::test(fs.clone(), [Path::new(path!("/root/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

    // Wait for the project scanning to finish so that `head_commit(cx)` is
    // actually set, otherwise no head commit would be available from which
    // to fetch the latest commit message from.
    cx.executor().run_until_parked();

    let panel = workspace.update_in(cx, GitPanel::new);
    panel.read_with(cx, |panel, cx| {
        assert!(panel.active_repository.is_some());
        assert!(panel.head_commit(cx).is_some());
    });

    panel.update_in(cx, |panel, window, cx| {
        // Update the commit editor's message to ensure that its contents
        // are later restored, after amending is finished.
        panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
            buffer.set_text("refactor: update main.rs", cx);
        });

        // Start amending the previous commit.
        panel.focus_editor(&Default::default(), window, cx);
        panel.on_amend(&Amend, window, cx);
    });

    // Since `GitPanel.amend` attempts to fetch the latest commit message in
    // a background task, we need to wait for it to complete before being
    // able to assert that the commit message editor's state has been
    // updated.
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        assert_eq!(
            panel.commit_message_buffer(cx).read(cx).text(),
            "initial commit"
        );
        assert_eq!(
            panel.original_commit_message,
            Some("refactor: update main.rs".to_string())
        );

        // Finish amending the previous commit.
        panel.focus_editor(&Default::default(), window, cx);
        panel.on_amend(&Amend, window, cx);
    });

    // Since the actual commit logic is run in a background task, we need to
    // await its completion to actually ensure that the commit message
    // editor's contents are set to the original message and haven't been
    // cleared.
    cx.run_until_parked();

    panel.update_in(cx, |panel, _window, cx| {
        // After amending, the commit editor's message should be restored to
        // the original message.
        assert_eq!(
            panel.commit_message_buffer(cx).read(cx).text(),
            "refactor: update main.rs"
        );
        assert!(panel.original_commit_message.is_none());
    });
}

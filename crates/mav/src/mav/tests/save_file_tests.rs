use super::*;

#[gpui::test]
async fn test_opening_excluded_paths(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |project_settings| {
                project_settings.project.worktree.file_scan_exclusions =
                    Some(vec!["excluded_dir".to_string(), "**/.git".to_string()]);
            });
        });
    });
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                ".gitignore": "ignored_dir\n",
                ".git": {
                    "HEAD": "ref: refs/heads/main",
                },
                "regular_dir": {
                    "file": "regular file contents",
                },
                "ignored_dir": {
                    "ignored_subdir": {
                        "file": "ignored subfile contents",
                    },
                    "file": "ignored file contents",
                },
                "excluded_dir": {
                    "file": "excluded file contents",
                    "ignored_subdir": {
                        "file": "ignored subfile contents",
                    },
                },
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    project.update(cx, |project, _cx| project.languages().add(markdown_lang()));
    let window = cx.add_window({
        let project = project.clone();
        |window, cx| MultiWorkspace::test_new(project, window, cx)
    });
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let initial_entries = cx.read(|cx| workspace.file_project_paths(cx));
    let paths_to_open = [
        PathBuf::from(path!("/root/excluded_dir/file")),
        PathBuf::from(path!("/root/.git/HEAD")),
        PathBuf::from(path!("/root/excluded_dir/ignored_subdir")),
    ];
    let workspace::OpenResult {
        window: opened_workspace,
        opened_items: new_items,
        ..
    } = cx
        .update(|cx| {
            workspace::open_paths(
                &paths_to_open,
                app_state,
                workspace::OpenOptions::default(),
                cx,
            )
        })
        .await
        .unwrap();

    assert_eq!(
        opened_workspace
            .read_with(cx, |mw, _| mw.workspace().entity_id())
            .unwrap(),
        workspace.entity_id(),
        "Excluded files in subfolders of a workspace root should be opened in the workspace"
    );
    let mut opened_paths = cx.read(|cx| {
        assert_eq!(
            new_items.len(),
            paths_to_open.len(),
            "Expect to get the same number of opened items as submitted paths to open"
        );
        new_items
            .iter()
            .zip(paths_to_open.iter())
            .map(|(i, path)| {
                match i {
                    Some(Ok(i)) => Some(i.project_path(cx).map(|p| p.path)),
                    Some(Err(e)) => panic!("Excluded file {path:?} failed to open: {e:?}"),
                    None => None,
                }
                .flatten()
            })
            .collect::<Vec<_>>()
    });
    opened_paths.sort();
    assert_eq!(
        opened_paths,
        vec![
            None,
            Some(rel_path(".git/HEAD").into()),
            Some(rel_path("excluded_dir/file").into()),
        ],
        "Excluded files should get opened, excluded dir should not get opened"
    );

    let entries = cx.read(|cx| workspace.file_project_paths(cx));
    assert_eq!(
        initial_entries, entries,
        "Workspace entries should not change after opening excluded files and directories paths"
    );

    cx.read(|cx| {
                let pane = workspace.read(cx).active_pane().read(cx);
                let mut opened_buffer_paths = pane
                    .items()
                    .map(|i| {
                        i.project_path(cx)
                            .expect("all excluded files that got open should have a path")
                            .path
                    })
                    .collect::<Vec<_>>();
                opened_buffer_paths.sort();
                assert_eq!(
                    opened_buffer_paths,
                    vec![rel_path(".git/HEAD").into(), rel_path("excluded_dir/file").into()],
                    "Despite not being present in the worktrees, buffers for excluded files are opened and added to the pane"
                );
            });
}

#[gpui::test]
async fn test_save_conflicting_item(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/root"), json!({ "a.txt": "" }))
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    project.update(cx, |project, _cx| project.languages().add(markdown_lang()));
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    // Open a file within an existing worktree.
    window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.open_paths(
                    vec![PathBuf::from(path!("/root/a.txt"))],
                    OpenOptions {
                        visible: Some(OpenVisible::All),
                        ..Default::default()
                    },
                    None,
                    window,
                    cx,
                )
            })
        })
        .unwrap()
        .await;
    let editor = cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        let item = pane.active_item().unwrap();
        item.downcast::<Editor>().unwrap()
    });

    window
        .update(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| editor.handle_input("x", window, cx));
        })
        .unwrap();

    app_state
        .fs
        .as_fake()
        .insert_file(path!("/root/a.txt"), b"changed".to_vec())
        .await;

    cx.run_until_parked();
    cx.read(|cx| assert!(editor.is_dirty(cx)));
    cx.read(|cx| assert!(editor.has_conflict(cx)));

    let save_task = window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.save_active_item(SaveIntent::Save, window, cx)
            })
        })
        .unwrap();
    cx.background_executor.run_until_parked();
    cx.simulate_prompt_answer("Overwrite");
    save_task.await.unwrap();
    window
        .update(cx, |_, _, cx| {
            editor.update(cx, |editor, cx| {
                assert!(!editor.is_dirty(cx));
                assert!(!editor.has_conflict(cx));
            });
        })
        .unwrap();
}

#[gpui::test]
async fn test_open_and_save_new_file(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .create_dir(Path::new(path!("/root")))
        .await
        .unwrap();

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    project.update(cx, |project, _| {
        project.languages().add(markdown_lang());
        project.languages().add(rust_lang());
    });
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let worktree = cx.read(|cx| workspace.read(cx).worktrees(cx).next().unwrap());

    // Create a new untitled buffer
    cx.dispatch_action(window.into(), NewFile);
    let editor = cx.read(|cx| {
        workspace
            .read(cx)
            .active_item(cx)
            .unwrap()
            .downcast::<Editor>()
            .unwrap()
    });

    window
        .update(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| {
                assert!(!editor.is_dirty(cx));
                assert_eq!(editor.title(cx), "untitled");
                assert!(Arc::ptr_eq(
                    &editor
                        .buffer()
                        .read(cx)
                        .language_at(MultiBufferOffset(0), cx)
                        .unwrap(),
                    &languages::PLAIN_TEXT
                ));
                editor.handle_input("hi", window, cx);
                assert!(editor.is_dirty(cx));
            });
        })
        .unwrap();

    // Save the buffer. This prompts for a filename.
    let save_task = window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.save_active_item(SaveIntent::Save, window, cx)
            })
        })
        .unwrap();
    cx.background_executor.run_until_parked();
    cx.simulate_new_path_selection(|parent_dir| {
        assert_eq!(parent_dir, Path::new(path!("/root")));
        Some(parent_dir.join("the-new-name.rs"))
    });
    cx.read(|cx| {
        assert!(editor.is_dirty(cx));
        assert_eq!(editor.read(cx).title(cx), "hi");
    });

    // When the save completes, the buffer's title is updated and the language is assigned based
    // on the path.
    save_task.await.unwrap();
    window
        .update(cx, |_, _, cx| {
            editor.update(cx, |editor, cx| {
                assert!(!editor.is_dirty(cx));
                assert_eq!(editor.title(cx), "the-new-name.rs");
                assert_eq!(
                    editor
                        .buffer()
                        .read(cx)
                        .language_at(MultiBufferOffset(0), cx)
                        .unwrap()
                        .name(),
                    "Rust"
                );
            });
        })
        .unwrap();

    // Edit the file and save it again. This time, there is no filename prompt.
    window
        .update(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| {
                editor.handle_input(" there", window, cx);
                assert!(editor.is_dirty(cx));
            });
        })
        .unwrap();

    let save_task = window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.save_active_item(SaveIntent::Save, window, cx)
            })
        })
        .unwrap();
    save_task.await.unwrap();

    assert!(!cx.did_prompt_for_new_path());
    window
        .update(cx, |_, _, cx| {
            editor.update(cx, |editor, cx| {
                assert!(!editor.is_dirty(cx));
                assert_eq!(editor.title(cx), "the-new-name.rs")
            });
        })
        .unwrap();

    // Open the same newly-created file in another pane item. The new editor should reuse
    // the same buffer.
    cx.dispatch_action(window.into(), NewFile);
    window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.split_and_clone(
                    workspace.active_pane().clone(),
                    SplitDirection::Right,
                    window,
                    cx,
                )
            })
        })
        .unwrap()
        .await
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.open_path(
                    (worktree.read(cx).id(), rel_path("the-new-name.rs")),
                    None,
                    true,
                    window,
                    cx,
                )
            })
        })
        .unwrap()
        .await
        .unwrap();
    let editor2 = cx.read(|cx| {
        workspace
            .read(cx)
            .active_item(cx)
            .unwrap()
            .downcast::<Editor>()
            .unwrap()
    });
    cx.read(|cx| {
        assert_eq!(
            editor2.read(cx).buffer().read(cx).as_singleton().unwrap(),
            editor.read(cx).buffer().read(cx).as_singleton().unwrap()
        );
    })
}

#[gpui::test]
async fn test_setting_language_when_saving_as_single_file_worktree(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state.fs.create_dir(Path::new("/root")).await.unwrap();

    let project = Project::test(app_state.fs.clone(), [], cx).await;
    project.update(cx, |project, _| {
        project.languages().add(language::rust_lang());
        project.languages().add(language::markdown_lang());
    });
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    // Create a new untitled buffer
    cx.dispatch_action(window.into(), NewFile);
    let editor = cx.read(|cx| {
        workspace
            .read(cx)
            .active_item(cx)
            .unwrap()
            .downcast::<Editor>()
            .unwrap()
    });
    window
        .update(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| {
                assert!(Arc::ptr_eq(
                    &editor
                        .buffer()
                        .read(cx)
                        .language_at(MultiBufferOffset(0), cx)
                        .unwrap(),
                    &languages::PLAIN_TEXT
                ));
                editor.handle_input("hi", window, cx);
                assert!(editor.is_dirty(cx));
            });
        })
        .unwrap();

    // Save the buffer. This prompts for a filename.
    let save_task = window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.save_active_item(SaveIntent::Save, window, cx)
            })
        })
        .unwrap();
    cx.background_executor.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(PathBuf::from("/root/the-new-name.rs")));
    save_task.await.unwrap();
    // The buffer is not dirty anymore and the language is assigned based on the path.
    window
        .update(cx, |_, _, cx| {
            editor.update(cx, |editor, cx| {
                assert!(!editor.is_dirty(cx));
                assert_eq!(
                    editor
                        .buffer()
                        .read(cx)
                        .language_at(MultiBufferOffset(0), cx)
                        .unwrap()
                        .name(),
                    "Rust"
                )
            });
        })
        .unwrap();
}

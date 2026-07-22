use super::*;

#[gpui::test]
async fn test_go_to_prev_hunk_multibuffer(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            ".git": {},
            "a.txt": "created\n",
            "b.txt": "really changed\n",
            "c.txt": "unchanged\n"
        }),
    )
    .await;

    fs.set_head_and_index_for_repo(
        Path::new(path!("/a/.git")),
        &[
            ("b.txt", "before\n".to_string()),
            ("c.txt", "unchanged\n".to_string()),
            ("d.txt", "deleted\n".to_string()),
        ],
    );

    let project = Project::test(fs, [Path::new(path!("/a"))], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    cx.run_until_parked();

    cx.focus(&workspace);
    cx.update(|window, cx| {
        window.dispatch_action(project_diff::Diff.boxed_clone(), cx);
    });

    cx.run_until_parked();

    let item = workspace.update(cx, |workspace, cx| {
        workspace.active_item_as::<ProjectDiff>(cx).unwrap()
    });
    cx.focus(&item);
    let editor = item.read_with(cx, |item, cx| item.editor.read(cx).rhs_editor().clone());

    let mut cx = EditorTestContext::for_editor_in(editor, cx).await;

    cx.set_selections_state(indoc!(
        "
            before
            really changed

            deleted

            ˇcreated
        "
    ));

    cx.dispatch_action(editor::actions::GoToPreviousHunk);

    cx.assert_excerpts_with_selections(indoc!(
        "
            [EXCERPT]
            before
            really changed
            [EXCERPT]
            ˇ[FOLDED]
            [EXCERPT]
            created
        "
    ));

    cx.dispatch_action(editor::actions::GoToPreviousHunk);

    cx.assert_excerpts_with_selections(indoc!(
        "
            [EXCERPT]
            ˇbefore
            really changed
            [EXCERPT]
            [FOLDED]
            [EXCERPT]
            created
        "
    ));
}

#[gpui::test]
async fn test_excerpts_splitting_after_restoring_the_middle_excerpt(cx: &mut TestAppContext) {
    init_test(cx);

    let git_contents = indoc! {r#"
            #[rustfmt::skip]
            fn main() {
                let x = 0.0; // this line will be removed
                // 1
                // 2
                // 3
                let y = 0.0; // this line will be removed
                // 1
                // 2
                // 3
                let arr = [
                    0.0, // this line will be removed
                    0.0, // this line will be removed
                    0.0, // this line will be removed
                    0.0, // this line will be removed
                ];
            }
        "#};
    let buffer_contents = indoc! {"
            #[rustfmt::skip]
            fn main() {
                // 1
                // 2
                // 3
                // 1
                // 2
                // 3
                let arr = [
                ];
            }
        "};

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            ".git": {},
            "main.rs": buffer_contents,
        }),
    )
    .await;

    fs.set_head_and_index_for_repo(
        Path::new(path!("/a/.git")),
        &[("main.rs", git_contents.to_owned())],
    );

    let project = Project::test(fs, [Path::new(path!("/a"))], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    cx.run_until_parked();

    cx.focus(&workspace);
    cx.update(|window, cx| {
        window.dispatch_action(project_diff::Diff.boxed_clone(), cx);
    });

    cx.run_until_parked();

    let item = workspace.update(cx, |workspace, cx| {
        workspace.active_item_as::<ProjectDiff>(cx).unwrap()
    });
    cx.focus(&item);
    let editor = item.read_with(cx, |item, cx| item.editor.read(cx).rhs_editor().clone());

    let mut cx = EditorTestContext::for_editor_in(editor, cx).await;

    cx.assert_excerpts_with_selections(&format!("[EXCERPT]\nˇ{git_contents}"));

    cx.dispatch_action(editor::actions::GoToHunk);
    cx.dispatch_action(editor::actions::GoToHunk);
    cx.dispatch_action(git::Restore);
    cx.dispatch_action(editor::actions::MoveToBeginning);

    cx.assert_excerpts_with_selections(&format!("[EXCERPT]\nˇ{git_contents}"));
}

#[gpui::test(iterations = 50)]
async fn test_split_diff_conflict_path_transition_with_dirty_buffer_invalid_anchor_panics(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.diff_view_style = Some(DiffViewStyle::Split);
            });
        });
    });

    let build_conflict_text: fn(usize) -> String = |tag: usize| {
        let mut lines = (0..80)
            .map(|line_index| format!("line {line_index}"))
            .collect::<Vec<_>>();
        for offset in [5usize, 20, 37, 61] {
            lines[offset] = format!("base-{tag}-line-{offset}");
        }
        format!("{}\n", lines.join("\n"))
    };
    let initial_conflict_text = build_conflict_text(0);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "helper.txt": "same\n",
            "conflict.txt": initial_conflict_text,
        }),
    )
    .await;
    fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
        state
            .refs
            .insert("MERGE_HEAD".into(), "conflict-head".into());
    })
    .unwrap();
    fs.set_status_for_repo(
        path!("/project/.git").as_ref(),
        &[(
            "conflict.txt",
            FileStatus::Unmerged(UnmergedStatus {
                first_head: UnmergedStatusCode::Updated,
                second_head: UnmergedStatusCode::Updated,
            }),
        )],
    );
    fs.set_merge_base_content_for_repo(
        path!("/project/.git").as_ref(),
        &[
            ("conflict.txt", build_conflict_text(1)),
            ("helper.txt", "same\n".to_string()),
        ],
    );

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let _project_diff = cx
        .update(|window, cx| {
            ProjectDiff::new_with_default_branch(project.clone(), workspace, window, cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/project/conflict.txt"), cx)
        })
        .await
        .unwrap();
    buffer.update(cx, |buffer, cx| buffer.edit([(0..0, "dirty\n")], None, cx));
    assert!(buffer.read_with(cx, |buffer, _| buffer.is_dirty()));
    cx.run_until_parked();

    cx.update(|window, cx| {
        let fs = fs.clone();
        window
            .spawn(cx, async move |cx| {
                cx.background_executor().simulate_random_delay().await;
                fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
                    state.refs.insert("HEAD".into(), "head-1".into());
                    state.refs.remove("MERGE_HEAD");
                })
                .unwrap();
                fs.set_status_for_repo(
                    path!("/project/.git").as_ref(),
                    &[
                        (
                            "conflict.txt",
                            FileStatus::Tracked(TrackedStatus {
                                index_status: git::status::StatusCode::Modified,
                                worktree_status: git::status::StatusCode::Modified,
                            }),
                        ),
                        (
                            "helper.txt",
                            FileStatus::Tracked(TrackedStatus {
                                index_status: git::status::StatusCode::Modified,
                                worktree_status: git::status::StatusCode::Modified,
                            }),
                        ),
                    ],
                );
                // FakeFs assigns deterministic OIDs by entry position; flipping order churns
                // conflict diff identity without reaching into ProjectDiff internals.
                fs.set_merge_base_content_for_repo(
                    path!("/project/.git").as_ref(),
                    &[
                        ("helper.txt", "helper-base\n".to_string()),
                        ("conflict.txt", build_conflict_text(2)),
                    ],
                );
            })
            .detach();
    });

    cx.update(|window, cx| {
        let buffer = buffer.clone();
        window
            .spawn(cx, async move |cx| {
                cx.background_executor().simulate_random_delay().await;
                for edit_index in 0..10 {
                    if edit_index > 0 {
                        cx.background_executor().simulate_random_delay().await;
                    }
                    buffer.update(cx, |buffer, cx| {
                        let len = buffer.len();
                        if edit_index % 2 == 0 {
                            buffer.edit(
                                [(0..0, format!("status-burst-head-{edit_index}\n"))],
                                None,
                                cx,
                            );
                        } else {
                            buffer.edit(
                                [(len..len, format!("status-burst-tail-{edit_index}\n"))],
                                None,
                                cx,
                            );
                        }
                    });
                }
            })
            .detach();
    });

    cx.run_until_parked();
}

#[gpui::test]
async fn test_new_hunk_in_modified_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "foo.txt": "
                    one
                    two
                    three
                    four
                    five
                    six
                    seven
                    eight
                    nine
                    ten
                    ELEVEN
                    twelve
                ".unindent()
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
        Path::new(path!("/project/.git")),
        &[(
            "foo.txt",
            "
                    one
                    two
                    three
                    four
                    five
                    six
                    seven
                    eight
                    nine
                    ten
                    eleven
                    twelve
                "
            .unindent(),
        )],
    );
    cx.run_until_parked();

    let editor = diff.read_with(cx, |diff, cx| diff.editor.read(cx).rhs_editor().clone());

    assert_state_with_diff(
        &editor,
        cx,
        &"
                  ˇnine
                  ten
                - eleven
                + ELEVEN
                  twelve
            "
        .unindent(),
    );

    // The project diff updates its excerpts when a new hunk appears in a buffer that already has a diff.
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/project/foo.txt"), cx)
        })
        .await
        .unwrap();
    buffer.update(cx, |buffer, cx| {
        buffer.edit_via_marked_text(
            &"
                    one
                    «TWO»
                    three
                    four
                    five
                    six
                    seven
                    eight
                    nine
                    ten
                    ELEVEN
                    twelve
                "
            .unindent(),
            None,
            cx,
        );
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();

    assert_state_with_diff(
        &editor,
        cx,
        &"
                  one
                - two
                + TWO
                  three
                  four
                  five
                  ˇnine
                  ten
                - eleven
                + ELEVEN
                  twelve
            "
        .unindent(),
    );
}

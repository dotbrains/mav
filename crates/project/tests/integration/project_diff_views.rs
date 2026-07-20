use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_unstaged_diff_for_buffer(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let staged_contents = r#"
        fn main() {
            println!("hello world");
        }
    "#
    .unindent();
    let file_contents = r#"
        // print goodbye
        fn main() {
            println!("goodbye world");
        }
    "#
    .unindent();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
           "src": {
               "main.rs": file_contents,
           }
        }),
    )
    .await;

    fs.set_index_for_repo(Path::new("/dir/.git"), &[("src/main.rs", staged_contents)]);

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/main.rs", cx)
        })
        .await
        .unwrap();
    let unstaged_diff = project
        .update(cx, |project, cx| {
            project.open_unstaged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();
    unstaged_diff.update(cx, |unstaged_diff, cx| {
        let snapshot = buffer.read(cx).snapshot();
        assert_hunks(
            unstaged_diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &unstaged_diff.base_text_string(cx).unwrap(),
            &[
                (0..1, "", "// print goodbye\n", DiffHunkStatus::added_none()),
                (
                    2..3,
                    "    println!(\"hello world\");\n",
                    "    println!(\"goodbye world\");\n",
                    DiffHunkStatus::modified_none(),
                ),
            ],
        );
    });

    let staged_contents = r#"
        // print goodbye
        fn main() {
        }
    "#
    .unindent();

    fs.set_index_for_repo(Path::new("/dir/.git"), &[("src/main.rs", staged_contents)]);

    cx.run_until_parked();
    unstaged_diff.update(cx, |unstaged_diff, cx| {
        let snapshot = buffer.read(cx).snapshot();
        assert_hunks(
            unstaged_diff.snapshot(cx).hunks_intersecting_range(
                Anchor::min_max_range_for_buffer(snapshot.remote_id()),
                &snapshot,
            ),
            &snapshot,
            &unstaged_diff.base_text(cx).text(),
            &[(
                2..3,
                "",
                "    println!(\"goodbye world\");\n",
                DiffHunkStatus::added_none(),
            )],
        );
    });
}

#[gpui::test]
async fn test_reopening_unstaged_diff_after_drop(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let staged_contents = r#"
        fn main() {
            println!("hello world");
        }
    "#
    .unindent();
    let file_contents = r#"
        // print goodbye
        fn main() {
            println!("goodbye world");
        }
    "#
    .unindent();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
           "src": {
               "main.rs": file_contents,
           }
        }),
    )
    .await;
    fs.set_index_for_repo(Path::new("/dir/.git"), &[("src/main.rs", staged_contents)]);

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/main.rs", cx)
        })
        .await
        .unwrap();
    let buffer_id = buffer.read_with(cx, |buffer, _| buffer.remote_id());

    let unstaged_diff = project
        .update(cx, |project, cx| {
            project.open_unstaged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    // Drop the diff while the buffer (and its git state) stays alive.
    drop(unstaged_diff);
    cx.run_until_parked();
    project.read_with(cx, |project, cx| {
        assert!(
            project
                .git_store()
                .read(cx)
                .get_unstaged_diff(buffer_id, cx)
                .is_none(),
            "unstaged diff should have been released"
        );
    });

    // Reopen the diff. The new entity must be registered in the git store,
    // and its hunks must be recalculated.
    let unstaged_diff = project
        .update(cx, |project, cx| {
            project.open_unstaged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        let registered = project
            .git_store()
            .read(cx)
            .get_unstaged_diff(buffer_id, cx);
        assert_eq!(
            registered.as_ref(),
            Some(&unstaged_diff),
            "reopened unstaged diff should be registered in the git store"
        );
    });

    unstaged_diff.update(cx, |unstaged_diff, cx| {
        let snapshot = buffer.read(cx).snapshot();
        assert_hunks(
            unstaged_diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &unstaged_diff.base_text_string(cx).unwrap(),
            &[
                (0..1, "", "// print goodbye\n", DiffHunkStatus::added_none()),
                (
                    2..3,
                    "    println!(\"hello world\");\n",
                    "    println!(\"goodbye world\");\n",
                    DiffHunkStatus::modified_none(),
                ),
            ],
        );
    });
}

#[gpui::test]
async fn test_staged_diff_for_buffer(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let committed_contents = r#"
        fn main() {
            println!("hello world");
        }
    "#
    .unindent();
    let staged_contents = r#"
        // print goodbye
        fn main() {
            println!("goodbye world");
        }
    "#
    .unindent();
    let file_contents = r#"
        // print goodbye
        fn main() {
            println!("working copy only");
        }
    "#
    .unindent();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
           "src": {
               "main.rs": file_contents,
           }
        }),
    )
    .await;
    fs.set_head_for_repo(
        Path::new("/dir/.git"),
        &[("src/main.rs", committed_contents)],
        "deadbeef",
    );
    fs.set_index_for_repo(Path::new("/dir/.git"), &[("src/main.rs", staged_contents)]);

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let language = rust_lang();
    language_registry.add(language.clone());

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/main.rs", cx)
        })
        .await
        .unwrap();
    let unstaged_diff = project
        .update(cx, |project, cx| {
            project.open_unstaged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    unstaged_diff.read_with(cx, |diff, cx| {
        assert_eq!(diff.base_text(cx).language().cloned(), None);
    });

    let staged_diff = project
        .update(cx, |project, cx| {
            project.open_staged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();
    unstaged_diff.read_with(cx, |diff, cx| {
        assert_eq!(
            diff.base_text(cx).language().cloned(),
            Some(language.clone())
        );
    });
    staged_diff.update(cx, |staged_diff, cx| {
        let snapshot = staged_diff.snapshot(cx);
        let buffer_snapshot = snapshot.buffer_snapshot();
        assert_hunks(
            snapshot.hunks(buffer_snapshot),
            buffer_snapshot,
            &staged_diff.base_text_string(cx).unwrap(),
            &[
                (0..1, "", "// print goodbye\n", DiffHunkStatus::added_none()),
                (
                    2..3,
                    "    println!(\"hello world\");\n",
                    "    println!(\"goodbye world\");\n",
                    DiffHunkStatus::modified_none(),
                ),
            ],
        );
    });

    let staged_contents = r#"
        // print goodbye
        fn main() {
        }
    "#
    .unindent();
    fs.set_index_for_repo(Path::new("/dir/.git"), &[("src/main.rs", staged_contents)]);

    cx.run_until_parked();
    staged_diff.update(cx, |staged_diff, cx| {
        let snapshot = staged_diff.snapshot(cx);
        let buffer_snapshot = snapshot.buffer_snapshot();
        assert_hunks(
            snapshot.hunks(buffer_snapshot),
            buffer_snapshot,
            &staged_diff.base_text_string(cx).unwrap(),
            &[
                (0..1, "", "// print goodbye\n", DiffHunkStatus::added_none()),
                (
                    2..2,
                    "    println!(\"hello world\");\n",
                    "",
                    DiffHunkStatus::deleted_none(),
                ),
            ],
        );
    });
}

#[gpui::test]
async fn test_base_text_buffers_released_when_diffs_dropped(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let committed_contents = "one\ntwo\nthree\n";
    let staged_contents = "one\nTWO\nthree\n";
    let file_contents = "one\nTWO\nTHREE\n";

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
            "src": {
                "main.rs": file_contents,
            }
        }),
    )
    .await;
    fs.set_head_for_repo(
        Path::new("/dir/.git"),
        &[("src/main.rs", committed_contents.to_owned())],
        "deadbeef",
    );
    fs.set_index_for_repo(
        Path::new("/dir/.git"),
        &[("src/main.rs", staged_contents.to_owned())],
    );

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/main.rs", cx)
        })
        .await
        .unwrap();

    let uncommitted_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    let unstaged_diff = project
        .update(cx, |project, cx| {
            project.open_unstaged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let weak_head_text_buffer =
        uncommitted_diff.read_with(cx, |diff, _| diff.base_text_buffer().downgrade());
    let weak_index_text_buffer =
        unstaged_diff.read_with(cx, |diff, _| diff.base_text_buffer().downgrade());

    drop(uncommitted_diff);
    cx.run_until_parked();
    cx.update(|_| {});
    weak_head_text_buffer.assert_released();
    assert!(
        weak_index_text_buffer.upgrade().is_some(),
        "index text buffer should stay alive while the unstaged diff is open"
    );

    drop(unstaged_diff);
    cx.run_until_parked();
    cx.update(|_| {});
    weak_index_text_buffer.assert_released();

    let uncommitted_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    uncommitted_diff.update(cx, |uncommitted_diff, cx| {
        assert_eq!(
            uncommitted_diff.base_text_string(cx).as_deref(),
            Some(committed_contents),
        );
        let snapshot = buffer.read(cx).snapshot();
        assert_hunks(
            uncommitted_diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &uncommitted_diff.base_text_string(cx).unwrap(),
            &[(
                1..3,
                "two\nthree\n",
                "TWO\nTHREE\n",
                DiffHunkStatus::modified(DiffHunkSecondaryStatus::OverlapsWithSecondaryHunk),
            )],
        );
    });
}

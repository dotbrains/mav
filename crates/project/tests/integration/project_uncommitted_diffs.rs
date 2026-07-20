use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_staged_diff_without_unstaged_diff(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let committed_contents = "one\ntwo\nthree\n";
    let staged_contents = "one\nTWO\nthree\n";
    let file_contents = "one\nTWO\nthree\n";

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

    let staged_diff = project
        .update(cx, |project, cx| {
            project.open_staged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    staged_diff.update(cx, |staged_diff, cx| {
        let snapshot = staged_diff.snapshot(cx);
        let buffer_snapshot = snapshot.buffer_snapshot();
        assert_hunks(
            snapshot.hunks(buffer_snapshot),
            buffer_snapshot,
            &staged_diff.base_text_string(cx).unwrap(),
            &[(1..2, "two\n", "TWO\n", DiffHunkStatus::modified_none())],
        );
    });

    fs.set_index_for_repo(
        Path::new("/dir/.git"),
        &[("src/main.rs", "one\nTWO\nTHREE\n".to_owned())],
    );
    cx.run_until_parked();

    staged_diff.update(cx, |staged_diff, cx| {
        let snapshot = staged_diff.snapshot(cx);
        let buffer_snapshot = snapshot.buffer_snapshot();
        assert_hunks(
            snapshot.hunks(buffer_snapshot),
            buffer_snapshot,
            &staged_diff.base_text_string(cx).unwrap(),
            &[(
                1..3,
                "two\nthree\n",
                "TWO\nTHREE\n",
                DiffHunkStatus::modified_none(),
            )],
        );
    });
}

#[gpui::test]
async fn test_uncommitted_diff_for_buffer(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let committed_contents = r#"
        fn main() {
            println!("hello world");
        }
    "#
    .unindent();
    let staged_contents = r#"
        fn main() {
            println!("goodbye world");
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
               "modification.rs": file_contents,
           }
        }),
    )
    .await;

    fs.set_head_for_repo(
        Path::new("/dir/.git"),
        &[
            ("src/modification.rs", committed_contents),
            ("src/deletion.rs", "// the-deleted-contents\n".into()),
        ],
        "deadbeef",
    );
    fs.set_index_for_repo(
        Path::new("/dir/.git"),
        &[
            ("src/modification.rs", staged_contents),
            ("src/deletion.rs", "// the-deleted-contents\n".into()),
        ],
    );

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let language = rust_lang();
    language_registry.add(language.clone());

    let buffer_1 = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/modification.rs", cx)
        })
        .await
        .unwrap();
    let diff_1 = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer_1.clone(), cx)
        })
        .await
        .unwrap();
    diff_1.read_with(cx, |diff, cx| {
        assert_eq!(diff.base_text(cx).language().cloned(), Some(language))
    });
    cx.run_until_parked();
    diff_1.update(cx, |diff, cx| {
        let snapshot = buffer_1.read(cx).snapshot();
        assert_hunks(
            diff.snapshot(cx).hunks_intersecting_range(
                Anchor::min_max_range_for_buffer(snapshot.remote_id()),
                &snapshot,
            ),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..1,
                    "",
                    "// print goodbye\n",
                    DiffHunkStatus::added(DiffHunkSecondaryStatus::HasSecondaryHunk),
                ),
                (
                    2..3,
                    "    println!(\"hello world\");\n",
                    "    println!(\"goodbye world\");\n",
                    DiffHunkStatus::modified_none(),
                ),
            ],
        );
    });

    // Reset HEAD to a version that differs from both the buffer and the index.
    let committed_contents = r#"
        // print goodbye
        fn main() {
        }
    "#
    .unindent();
    fs.set_head_for_repo(
        Path::new("/dir/.git"),
        &[
            ("src/modification.rs", committed_contents.clone()),
            ("src/deletion.rs", "// the-deleted-contents\n".into()),
        ],
        "deadbeef",
    );

    // Buffer now has an unstaged hunk.
    cx.run_until_parked();
    diff_1.update(cx, |diff, cx| {
        let snapshot = buffer_1.read(cx).snapshot();
        assert_hunks(
            diff.snapshot(cx).hunks_intersecting_range(
                Anchor::min_max_range_for_buffer(snapshot.remote_id()),
                &snapshot,
            ),
            &snapshot,
            &diff.base_text(cx).text(),
            &[(
                2..3,
                "",
                "    println!(\"goodbye world\");\n",
                DiffHunkStatus::added_none(),
            )],
        );
    });

    // Open a buffer for a file that's been deleted.
    let buffer_2 = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/deletion.rs", cx)
        })
        .await
        .unwrap();
    let diff_2 = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer_2.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    diff_2.update(cx, |diff, cx| {
        let snapshot = buffer_2.read(cx).snapshot();
        assert_hunks(
            diff.snapshot(cx).hunks_intersecting_range(
                Anchor::min_max_range_for_buffer(snapshot.remote_id()),
                &snapshot,
            ),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[(
                0..0,
                "// the-deleted-contents\n",
                "",
                DiffHunkStatus::deleted(DiffHunkSecondaryStatus::HasSecondaryHunk),
            )],
        );
    });

    // Stage the deletion of this file
    fs.set_index_for_repo(
        Path::new("/dir/.git"),
        &[("src/modification.rs", committed_contents.clone())],
    );
    cx.run_until_parked();
    diff_2.update(cx, |diff, cx| {
        let snapshot = buffer_2.read(cx).snapshot();
        assert_hunks(
            diff.snapshot(cx).hunks_intersecting_range(
                Anchor::min_max_range_for_buffer(snapshot.remote_id()),
                &snapshot,
            ),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[(
                0..0,
                "// the-deleted-contents\n",
                "",
                DiffHunkStatus::deleted(DiffHunkSecondaryStatus::NoSecondaryHunk),
            )],
        );
    });
}

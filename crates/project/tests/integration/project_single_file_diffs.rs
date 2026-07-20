use super::*;

#[gpui::test]
async fn test_single_file_diffs(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let committed_contents = r#"
        fn main() {
            println!("hello from HEAD");
        }
    "#
    .unindent();
    let file_contents = r#"
        fn main() {
            println!("hello from the working copy");
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
        &[("src/main.rs", committed_contents.clone())],
        "deadbeef",
    );
    fs.set_index_for_repo(
        Path::new("/dir/.git"),
        &[("src/main.rs", committed_contents.clone())],
    );

    let project = Project::test(fs.clone(), ["/dir/src/main.rs".as_ref()], cx).await;

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

    cx.run_until_parked();
    uncommitted_diff.update(cx, |uncommitted_diff, cx| {
        let snapshot = buffer.read(cx).snapshot();
        assert_hunks(
            uncommitted_diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &uncommitted_diff.base_text_string(cx).unwrap(),
            &[(
                1..2,
                "    println!(\"hello from HEAD\");\n",
                "    println!(\"hello from the working copy\");\n",
                DiffHunkStatus {
                    kind: DiffHunkStatusKind::Modified,
                    secondary: DiffHunkSecondaryStatus::HasSecondaryHunk,
                },
            )],
        );
    });
}

// TODO: Should we test this on Windows also?
#[gpui::test]
#[cfg(not(windows))]
async fn test_staging_hunk_preserve_executable_permission(cx: &mut gpui::TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    init_test(cx);
    cx.executor().allow_parking();
    let committed_contents = "bar\n";
    let file_contents = "baz\n";
    let root = TempTree::new(json!({
        "project": {
            "foo": committed_contents
        },
    }));

    let work_dir = root.path().join("project");
    let file_path = work_dir.join("foo");
    let repo = git_init(work_dir.as_path());
    let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&file_path, perms).unwrap();
    git_add("foo", &repo);
    git_commit("Initial commit", &repo);
    std::fs::write(&file_path, file_contents).unwrap();

    let project = Project::test(
        Arc::new(RealFs::new(None, cx.executor())),
        [root.path()],
        cx,
    )
    .await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(file_path.as_path(), cx)
        })
        .await
        .unwrap();

    let snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());

    let uncommitted_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    uncommitted_diff.update(cx, |diff, cx| {
        let hunks = diff.snapshot(cx).hunks(&snapshot).collect::<Vec<_>>();
        diff.stage_or_unstage_hunks(true, &hunks, &snapshot, true, cx);
    });

    cx.run_until_parked();

    let output = smol::process::Command::new("git")
        .current_dir(&work_dir)
        .args(["diff", "--staged"])
        .output()
        .await
        .unwrap();

    let staged_diff = String::from_utf8_lossy(&output.stdout);

    assert!(
        !staged_diff.contains("new mode 100644"),
        "Staging should not change file mode from 755 to 644.\ngit diff --staged:\n{}",
        staged_diff
    );

    let output = smol::process::Command::new("git")
        .current_dir(&work_dir)
        .args(["ls-files", "-s"])
        .output()
        .await
        .unwrap();
    let index_contents = String::from_utf8_lossy(&output.stdout);

    assert!(
        index_contents.contains("100755"),
        "Index should show file as executable (100755).\ngit ls-files -s:\n{}",
        index_contents
    );
}

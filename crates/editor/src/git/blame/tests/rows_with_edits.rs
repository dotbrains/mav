use super::*;

#[gpui::test]
async fn test_blame_for_rows_with_edits(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/my-repo"),
        json!({
            ".git": {},
            "file.txt": r#"
                    Line 1
                    Line 2
                    Line 3
                "#
            .unindent()
        }),
    )
    .await;

    fs.set_blame_for_repo(
        Path::new(path!("/my-repo/.git")),
        vec![(
            repo_path("file.txt"),
            Blame {
                entries: vec![blame_entry("1b1b1b", 0..4)],
                ..Default::default()
            },
        )],
    );

    let project = Project::test(fs, [path!("/my-repo").as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/my-repo/file.txt"), cx)
        })
        .await
        .unwrap();
    let buffer_id = buffer.read_with(cx, |buffer, _| buffer.remote_id());
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));

    let git_blame = cx.new(|cx| GitBlame::new(buffer.clone(), project, false, true, cx));

    cx.executor().run_until_parked();

    git_blame.update(cx, |blame, cx| {
        // Sanity check before edits: make sure that we get the same blame entry for all
        // lines.
        assert_blame_rows(
            blame,
            buffer_id,
            0..4,
            vec![
                Some(blame_entry("1b1b1b", 0..4)),
                Some(blame_entry("1b1b1b", 0..4)),
                Some(blame_entry("1b1b1b", 0..4)),
                Some(blame_entry("1b1b1b", 0..4)),
            ],
            cx,
        );
    });

    // Modify a single line, at the start of the line
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(0, 0)..Point::new(0, 0), "X")], None, cx);
    });
    git_blame.update(cx, |blame, cx| {
        assert_blame_rows(
            blame,
            buffer_id,
            0..2,
            vec![None, Some(blame_entry("1b1b1b", 0..4))],
            cx,
        );
    });
    // Modify a single line, in the middle of the line
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(1, 2)..Point::new(1, 2), "X")], None, cx);
    });
    git_blame.update(cx, |blame, cx| {
        assert_blame_rows(
            blame,
            buffer_id,
            1..4,
            vec![
                None,
                Some(blame_entry("1b1b1b", 0..4)),
                Some(blame_entry("1b1b1b", 0..4)),
            ],
            cx,
        );
    });

    // Before we insert a newline at the end, sanity check:
    git_blame.update(cx, |blame, cx| {
        assert_blame_rows(
            blame,
            buffer_id,
            3..4,
            vec![Some(blame_entry("1b1b1b", 0..4))],
            cx,
        );
    });
    // Insert a newline at the end
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(3, 6)..Point::new(3, 6), "\n")], None, cx);
    });
    // Only the new line is marked as edited:
    git_blame.update(cx, |blame, cx| {
        assert_blame_rows(
            blame,
            buffer_id,
            3..5,
            vec![Some(blame_entry("1b1b1b", 0..4)), None],
            cx,
        );
    });

    // Before we insert a newline at the start, sanity check:
    git_blame.update(cx, |blame, cx| {
        assert_blame_rows(
            blame,
            buffer_id,
            2..3,
            vec![Some(blame_entry("1b1b1b", 0..4))],
            cx,
        );
    });

    // Usage example
    // Insert a newline at the start of the row
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(2, 0)..Point::new(2, 0), "\n")], None, cx);
    });
    // Only the new line is marked as edited:
    git_blame.update(cx, |blame, cx| {
        assert_blame_rows(
            blame,
            buffer_id,
            2..4,
            vec![None, Some(blame_entry("1b1b1b", 0..4))],
            cx,
        );
    });
}

use super::*;

#[gpui::test]
async fn test_blame_for_rows(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/my-repo",
        json!({
            ".git": {},
            "file.txt": r#"
                    AAA Line 1
                    BBB Line 2 - Modified 1
                    CCC Line 3 - Modified 2
                    modified in memory 1
                    modified in memory 1
                    DDD Line 4 - Modified 2
                    EEE Line 5 - Modified 1
                    FFF Line 6 - Modified 2
                "#
            .unindent()
        }),
    )
    .await;

    fs.set_blame_for_repo(
        Path::new("/my-repo/.git"),
        vec![(
            repo_path("file.txt"),
            Blame {
                entries: vec![
                    blame_entry("1b1b1b", 0..1),
                    blame_entry("0d0d0d", 1..2),
                    blame_entry("3a3a3a", 2..3),
                    blame_entry("3a3a3a", 5..6),
                    blame_entry("0d0d0d", 6..7),
                    blame_entry("3a3a3a", 7..8),
                ],
                ..Default::default()
            },
        )],
    );
    let project = Project::test(fs, ["/my-repo".as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/my-repo/file.txt", cx)
        })
        .await
        .unwrap();
    let buffer_id = buffer.read_with(cx, |buffer, _| buffer.remote_id());
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));

    let git_blame = cx.new(|cx| GitBlame::new(buffer.clone(), project, false, true, cx));

    cx.executor().run_until_parked();

    git_blame.update(cx, |blame, cx| {
        // All lines
        pretty_assertions::assert_eq!(
            blame
                .blame_for_rows(
                    &(0..8)
                        .map(|buffer_row| RowInfo {
                            buffer_row: Some(buffer_row),
                            buffer_id: Some(buffer_id),
                            ..Default::default()
                        })
                        .collect::<Vec<_>>(),
                    cx
                )
                .collect::<Vec<_>>(),
            vec![
                Some((buffer_id, blame_entry("1b1b1b", 0..1))),
                Some((buffer_id, blame_entry("0d0d0d", 1..2))),
                Some((buffer_id, blame_entry("3a3a3a", 2..3))),
                None,
                None,
                Some((buffer_id, blame_entry("3a3a3a", 5..6))),
                Some((buffer_id, blame_entry("0d0d0d", 6..7))),
                Some((buffer_id, blame_entry("3a3a3a", 7..8))),
            ]
        );
        // Subset of lines
        pretty_assertions::assert_eq!(
            blame
                .blame_for_rows(
                    &(1..4)
                        .map(|buffer_row| RowInfo {
                            buffer_row: Some(buffer_row),
                            buffer_id: Some(buffer_id),
                            ..Default::default()
                        })
                        .collect::<Vec<_>>(),
                    cx
                )
                .collect::<Vec<_>>(),
            vec![
                Some((buffer_id, blame_entry("0d0d0d", 1..2))),
                Some((buffer_id, blame_entry("3a3a3a", 2..3))),
                None
            ]
        );
        // Subset of lines, with some not displayed
        pretty_assertions::assert_eq!(
            blame
                .blame_for_rows(
                    &[
                        RowInfo {
                            buffer_row: Some(1),
                            buffer_id: Some(buffer_id),
                            ..Default::default()
                        },
                        Default::default(),
                        Default::default(),
                    ],
                    cx
                )
                .collect::<Vec<_>>(),
            vec![Some((buffer_id, blame_entry("0d0d0d", 1..2))), None, None]
        );
    });
}

use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_search(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "const ONE: usize = 1;",
            "two.rs": "const TWO: usize = one::ONE + one::ONE;",
            "three.rs": "const THREE: usize = one::ONE + two::TWO;",
            "four.rs": "const FOUR: usize = one::ONE + three::THREE;",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                "TWO",
                false,
                true,
                false,
                Default::default(),
                Default::default(),
                false,
                None
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap(),
        HashMap::from_iter([
            (path!("dir/two.rs").to_string(), vec![6..9]),
            (path!("dir/three.rs").to_string(), vec![37..40])
        ])
    );

    let buffer_4 = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/four.rs"), cx)
        })
        .await
        .unwrap();
    buffer_4.update(cx, |buffer, cx| {
        let text = "two::TWO";
        buffer.edit([(20..28, text), (31..43, text)], None, cx);
    });

    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                "TWO",
                false,
                true,
                false,
                Default::default(),
                Default::default(),
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap(),
        HashMap::from_iter([
            (path!("dir/two.rs").to_string(), vec![6..9]),
            (path!("dir/three.rs").to_string(), vec![37..40]),
            (path!("dir/four.rs").to_string(), vec![25..28, 36..39])
        ])
    );
}

#[gpui::test]
async fn test_search_multiline_crlf(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "crlf.rs": "alpha\r\nbeta\r\ngamma",
            "lf.rs": "alpha\nbeta\ngamma",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    // A query with CRLF line endings should match buffers regardless of their
    // on-disk line endings (buffers always store normalized `\n`).
    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                "alpha\r\nbeta",
                false,
                true,
                false,
                Default::default(),
                Default::default(),
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap(),
        HashMap::from_iter([
            (path!("dir/crlf.rs").to_string(), vec![0..10]),
            (path!("dir/lf.rs").to_string(), vec![0..10]),
        ])
    );

    // The reverse: a query with LF line endings should match a file that is
    // stored with CRLF line endings on disk.
    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                "alpha\nbeta",
                false,
                true,
                false,
                Default::default(),
                Default::default(),
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap(),
        HashMap::from_iter([
            (path!("dir/crlf.rs").to_string(), vec![0..10]),
            (path!("dir/lf.rs").to_string(), vec![0..10]),
        ])
    );
}

use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_search_with_inclusions(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let search_query = "file";

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": r#"// Rust file one"#,
            "one.ts": r#"// TypeScript file one"#,
            "two.rs": r#"// Rust file two"#,
            "two.ts": r#"// TypeScript file two"#,
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    assert!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                PathMatcher::new(&["*.odd".to_owned()], PathStyle::local()).unwrap(),
                Default::default(),
                false,
                None
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap()
        .is_empty(),
        "If no inclusions match, no files should be returned"
    );

    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                PathMatcher::new(&["*.rs".to_owned()], PathStyle::local()).unwrap(),
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
            (path!("dir/one.rs").to_string(), vec![8..12]),
            (path!("dir/two.rs").to_string(), vec![8..12]),
        ]),
        "Rust only search should give only Rust files"
    );

    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                PathMatcher::new(&["*.ts".to_owned(), "*.odd".to_owned()], PathStyle::local())
                    .unwrap(),
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
            (path!("dir/one.ts").to_string(), vec![14..18]),
            (path!("dir/two.ts").to_string(), vec![14..18]),
        ]),
        "TypeScript only search should give only TypeScript files, even if other inclusions don't match anything"
    );

    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                PathMatcher::new(
                    &["*.rs".to_owned(), "*.ts".to_owned(), "*.odd".to_owned()],
                    PathStyle::local()
                )
                .unwrap(),
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
            (path!("dir/two.ts").to_string(), vec![14..18]),
            (path!("dir/one.rs").to_string(), vec![8..12]),
            (path!("dir/one.ts").to_string(), vec![14..18]),
            (path!("dir/two.rs").to_string(), vec![8..12]),
        ]),
        "Rust and typescript search should give both Rust and TypeScript files, even if other inclusions don't match anything"
    );
}

#[gpui::test]
async fn test_search_with_exclusions(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let search_query = "file";

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": r#"// Rust file one"#,
            "one.ts": r#"// TypeScript file one"#,
            "two.rs": r#"// Rust file two"#,
            "two.ts": r#"// TypeScript file two"#,
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                Default::default(),
                PathMatcher::new(&["*.odd".to_owned()], PathStyle::local()).unwrap(),
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap(),
        HashMap::from_iter([
            (path!("dir/one.rs").to_string(), vec![8..12]),
            (path!("dir/one.ts").to_string(), vec![14..18]),
            (path!("dir/two.rs").to_string(), vec![8..12]),
            (path!("dir/two.ts").to_string(), vec![14..18]),
        ]),
        "If no exclusions match, all files should be returned"
    );

    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                Default::default(),
                PathMatcher::new(&["*.rs".to_owned()], PathStyle::local()).unwrap(),
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap(),
        HashMap::from_iter([
            (path!("dir/one.ts").to_string(), vec![14..18]),
            (path!("dir/two.ts").to_string(), vec![14..18]),
        ]),
        "Rust exclusion search should give only TypeScript files"
    );

    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                Default::default(),
                PathMatcher::new(&["*.ts".to_owned(), "*.odd".to_owned()], PathStyle::local())
                    .unwrap(),
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap(),
        HashMap::from_iter([
            (path!("dir/one.rs").to_string(), vec![8..12]),
            (path!("dir/two.rs").to_string(), vec![8..12]),
        ]),
        "TypeScript exclusion search should give only Rust files, even if other exclusions don't match anything"
    );

    assert!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                Default::default(),
                PathMatcher::new(
                    &["*.rs".to_owned(), "*.ts".to_owned(), "*.odd".to_owned()],
                    PathStyle::local(),
                )
                .unwrap(),
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap()
        .is_empty(),
        "Rust and typescript exclusion should give no files, even if other exclusions don't match anything"
    );
}

#[gpui::test]
async fn test_search_with_buffer_exclusions(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let search_query = "file";

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": r#"// Rust file one"#,
            "one.ts": r#"// TypeScript file one"#,
            "two.rs": r#"// Rust file two"#,
            "two.ts": r#"// TypeScript file two"#,
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let path_style = PathStyle::local();
    let _buffer = project.update(cx, |project, cx| {
        project.create_local_buffer("file", None, false, cx)
    });

    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                Default::default(),
                PathMatcher::new(&["*.odd".to_owned()], path_style).unwrap(),
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap(),
        HashMap::from_iter([
            (path!("dir/one.rs").to_string(), vec![8..12]),
            (path!("dir/one.ts").to_string(), vec![14..18]),
            (path!("dir/two.rs").to_string(), vec![8..12]),
            (path!("dir/two.ts").to_string(), vec![14..18]),
        ]),
        "If no exclusions match, all files should be returned"
    );

    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                Default::default(),
                PathMatcher::new(&["*.rs".to_owned()], path_style).unwrap(),
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap(),
        HashMap::from_iter([
            (path!("dir/one.ts").to_string(), vec![14..18]),
            (path!("dir/two.ts").to_string(), vec![14..18]),
        ]),
        "Rust exclusion search should give only TypeScript files"
    );

    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                Default::default(),
                PathMatcher::new(&["*.ts".to_owned(), "*.odd".to_owned()], path_style).unwrap(),
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap(),
        HashMap::from_iter([
            (path!("dir/one.rs").to_string(), vec![8..12]),
            (path!("dir/two.rs").to_string(), vec![8..12]),
        ]),
        "TypeScript exclusion search should give only Rust files, even if other exclusions don't match anything"
    );

    assert!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                Default::default(),
                PathMatcher::new(
                    &["*.rs".to_owned(), "*.ts".to_owned(), "*.odd".to_owned()],
                    PathStyle::local(),
                )
                .unwrap(),
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap()
        .is_empty(),
        "Rust and typescript exclusion should give no files, even if other exclusions don't match anything"
    );
}

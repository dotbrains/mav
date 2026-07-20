use super::*;
use pretty_assertions::{assert_eq, assert_matches};

#[gpui::test]
async fn test_search_with_exclusions_and_inclusions(cx: &mut gpui::TestAppContext) {
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
                PathMatcher::new(&["*.odd".to_owned()], PathStyle::local()).unwrap(),
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap()
        .is_empty(),
        "If both no exclusions and inclusions match, exclusions should win and return nothing"
    );

    assert!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                PathMatcher::new(&["*.ts".to_owned()], PathStyle::local()).unwrap(),
                PathMatcher::new(&["*.ts".to_owned()], PathStyle::local()).unwrap(),
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap()
        .is_empty(),
        "If both TypeScript exclusions and inclusions match, exclusions should win and return nothing files."
    );

    assert!(
        search(
            &project,
            SearchQuery::text(
                search_query,
                false,
                true,
                false,
                PathMatcher::new(&["*.ts".to_owned(), "*.odd".to_owned()], PathStyle::local())
                    .unwrap(),
                PathMatcher::new(&["*.ts".to_owned(), "*.odd".to_owned()], PathStyle::local())
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
        "Non-matching inclusions and exclusions should not change that."
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
                PathMatcher::new(&["*.rs".to_owned(), "*.odd".to_owned()], PathStyle::local())
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
            (path!("dir/one.ts").to_string(), vec![14..18]),
            (path!("dir/two.ts").to_string(), vec![14..18]),
        ]),
        "Non-intersecting TypeScript inclusions and Rust exclusions should return TypeScript files"
    );
}

#[gpui::test]
async fn test_search_multiple_worktrees_with_inclusions(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/worktree-a"),
        json!({
            "haystack.rs": r#"// NEEDLE"#,
            "haystack.ts": r#"// NEEDLE"#,
        }),
    )
    .await;
    fs.insert_tree(
        path!("/worktree-b"),
        json!({
            "haystack.rs": r#"// NEEDLE"#,
            "haystack.ts": r#"// NEEDLE"#,
        }),
    )
    .await;

    let path_style = PathStyle::local();
    let project = Project::test(
        fs.clone(),
        [path!("/worktree-a").as_ref(), path!("/worktree-b").as_ref()],
        cx,
    )
    .await;

    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                "NEEDLE",
                false,
                true,
                false,
                PathMatcher::new(&["worktree-a/*.rs".to_owned()], path_style).unwrap(),
                Default::default(),
                true,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap(),
        HashMap::from_iter([(path!("worktree-a/haystack.rs").to_string(), vec![3..9])]),
        "should only return results from included worktree"
    );
    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                "NEEDLE",
                false,
                true,
                false,
                PathMatcher::new(&["worktree-b/*.rs".to_owned()], path_style).unwrap(),
                Default::default(),
                true,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap(),
        HashMap::from_iter([(path!("worktree-b/haystack.rs").to_string(), vec![3..9])]),
        "should only return results from included worktree"
    );

    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                "NEEDLE",
                false,
                true,
                false,
                PathMatcher::new(&["*.ts".to_owned()], path_style).unwrap(),
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
            (path!("worktree-a/haystack.ts").to_string(), vec![3..9]),
            (path!("worktree-b/haystack.ts").to_string(), vec![3..9])
        ]),
        "should return results from both worktrees"
    );
}

#[gpui::test]
async fn test_search_in_gitignored_dirs(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            ".git": {},
            ".gitignore": "**/target\n/node_modules\n",
            "target": {
                "index.txt": "index_key:index_value"
            },
            "node_modules": {
                "eslint": {
                    "index.ts": "const eslint_key = 'eslint value'",
                    "package.json": r#"{ "some_key": "some value" }"#,
                },
                "prettier": {
                    "index.ts": "const prettier_key = 'prettier value'",
                    "package.json": r#"{ "other_key": "other value" }"#,
                },
            },
            "package.json": r#"{ "main_key": "main value" }"#,
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    let query = "key";
    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                query,
                false,
                false,
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
        HashMap::from_iter([(path!("dir/package.json").to_string(), vec![8..11])]),
        "Only one non-ignored file should have the query"
    );

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let path_style = PathStyle::local();
    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                query,
                false,
                false,
                true,
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
            (path!("dir/package.json").to_string(), vec![8..11]),
            (path!("dir/target/index.txt").to_string(), vec![6..9]),
            (
                path!("dir/node_modules/prettier/package.json").to_string(),
                vec![9..12]
            ),
            (
                path!("dir/node_modules/prettier/index.ts").to_string(),
                vec![15..18]
            ),
            (
                path!("dir/node_modules/eslint/index.ts").to_string(),
                vec![13..16]
            ),
            (
                path!("dir/node_modules/eslint/package.json").to_string(),
                vec![8..11]
            ),
        ]),
        "Unrestricted search with ignored directories should find every file with the query"
    );

    let files_to_include =
        PathMatcher::new(&["node_modules/prettier/**".to_owned()], path_style).unwrap();
    let files_to_exclude = PathMatcher::new(&["*.ts".to_owned()], path_style).unwrap();
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                query,
                false,
                false,
                true,
                files_to_include,
                files_to_exclude,
                false,
                None,
            )
            .unwrap(),
            cx
        )
        .await
        .unwrap(),
        HashMap::from_iter([(
            path!("dir/node_modules/prettier/package.json").to_string(),
            vec![9..12]
        )]),
        "With search including ignored prettier directory and excluding TS files, only one file should be found"
    );
}

#[gpui::test]
async fn test_search_with_unicode(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "// ПРИВЕТ? привет!",
            "two.rs": "// ПРИВЕТ.",
            "three.rs": "// привет",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let unicode_case_sensitive_query = SearchQuery::text(
        "привет",
        false,
        true,
        false,
        Default::default(),
        Default::default(),
        false,
        None,
    );
    assert_matches!(unicode_case_sensitive_query, Ok(SearchQuery::Text { .. }));
    assert_eq!(
        search(&project, unicode_case_sensitive_query.unwrap(), cx)
            .await
            .unwrap(),
        HashMap::from_iter([
            (path!("dir/one.rs").to_string(), vec![17..29]),
            (path!("dir/three.rs").to_string(), vec![3..15]),
        ])
    );

    let unicode_case_insensitive_query = SearchQuery::text(
        "привет",
        false,
        false,
        false,
        Default::default(),
        Default::default(),
        false,
        None,
    );
    assert_matches!(
        unicode_case_insensitive_query,
        Ok(SearchQuery::Regex { .. })
    );
    assert_eq!(
        search(&project, unicode_case_insensitive_query.unwrap(), cx)
            .await
            .unwrap(),
        HashMap::from_iter([
            (path!("dir/one.rs").to_string(), vec![3..15, 17..29]),
            (path!("dir/two.rs").to_string(), vec![3..15]),
            (path!("dir/three.rs").to_string(), vec![3..15]),
        ])
    );

    assert_eq!(
        search(
            &project,
            SearchQuery::text(
                "привет.",
                false,
                false,
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
        HashMap::from_iter([(path!("dir/two.rs").to_string(), vec![3..16]),])
    );
}

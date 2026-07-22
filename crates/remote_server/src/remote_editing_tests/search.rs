use super::*;

async fn do_search_and_assert(
    project: &Entity<Project>,
    query: &str,
    files_to_include: PathMatcher,
    match_full_paths: bool,
    expected_paths: &[&str],
    mut cx: TestAppContext,
) -> Vec<Entity<Buffer>> {
    let query = query.to_string();
    let receiver = project.update(&mut cx, |project, cx| {
        project.search(
            SearchQuery::text(
                query,
                false,
                true,
                false,
                files_to_include,
                Default::default(),
                match_full_paths,
                None,
            )
            .unwrap(),
            cx,
        )
    });

    let mut buffers = Vec::new();
    for expected_path in expected_paths {
        let buffer = loop {
            let response = receiver.rx.recv().await.unwrap();
            match response {
                SearchResult::Buffer { buffer, .. } => break buffer,
                SearchResult::LimitReached => panic!("incorrect result"),
                SearchResult::WaitingForScan | SearchResult::Searching => continue,
            }
        };
        buffer.update(&mut cx, |buffer, cx| {
            assert_eq!(
                buffer.file().unwrap().full_path(cx).to_string_lossy(),
                *expected_path
            )
        });
        buffers.push(buffer);
    }

    assert!(receiver.rx.recv().await.is_err());
    buffers
}

#[gpui::test]
async fn test_remote_project_search(cx: &mut TestAppContext, server_cx: &mut TestAppContext) {
    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        path!("/code"),
        json!({
            "project1": {
                ".git": {},
                "README.md": "# project 1",
                "src": {
                    "lib.rs": "fn one() -> usize { 1 }"
                }
            },
        }),
    )
    .await;

    let (project, headless) = init_test(&fs, cx, server_cx).await;

    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/code/project1"), true, cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    let buffers = do_search_and_assert(
        &project,
        "project",
        Default::default(),
        false,
        &[path!("project1/README.md")],
        cx.clone(),
    )
    .await;
    let buffer = buffers.into_iter().next().unwrap();

    // test that the headless server is tracking which buffers we have open correctly.
    cx.run_until_parked();
    headless.update(server_cx, |headless, cx| {
        assert!(headless.buffer_store.read(cx).has_shared_buffers())
    });
    do_search_and_assert(
        &project,
        "project",
        Default::default(),
        false,
        &[path!("project1/README.md")],
        cx.clone(),
    )
    .await;
    server_cx.run_until_parked();
    cx.update(|_| {
        drop(buffer);
    });
    cx.run_until_parked();
    server_cx.run_until_parked();
    headless.update(server_cx, |headless, cx| {
        assert!(!headless.buffer_store.read(cx).has_shared_buffers())
    });

    do_search_and_assert(
        &project,
        "project",
        Default::default(),
        false,
        &[path!("project1/README.md")],
        cx.clone(),
    )
    .await;
}

#[gpui::test]
async fn test_remote_project_search_single_cpu(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        path!("/code"),
        json!({
            "project1": {
                ".git": {},
                "README.md": "# project 1",
                "src": {
                    "lib.rs": "fn one() -> usize { 1 }"
                }
            },
        }),
    )
    .await;

    // Simulate a single-CPU environment (e.g. a devcontainer with 1 visible CPU).
    // This causes the worker pool in project search to spawn num_cpus - 1 = 0 workers,
    // which silently drops all search channels and produces zero results.
    server_cx.executor().set_num_cpus(1);

    let (project, _) = init_test(&fs, cx, server_cx).await;

    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/code/project1"), true, cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    do_search_and_assert(
        &project,
        "project",
        Default::default(),
        false,
        &[path!("project1/README.md")],
        cx.clone(),
    )
    .await;
}

#[gpui::test]
async fn test_remote_project_search_inclusion(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        path!("/code"),
        json!({
            "project1": {
                "README.md": "# project 1",
            },
            "project2": {
                "README.md": "# project 2",
            },
        }),
    )
    .await;

    let (project, _) = init_test(&fs, cx, server_cx).await;

    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/code/project1"), true, cx)
        })
        .await
        .unwrap();

    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/code/project2"), true, cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    // Case 1: Test search with path matcher limiting to only one worktree
    let path_matcher = PathMatcher::new(
        &["project1/*.md".to_owned()],
        util::paths::PathStyle::local(),
    )
    .unwrap();
    do_search_and_assert(
        &project,
        "project",
        path_matcher,
        true, // should be true in case of multiple worktrees
        &[path!("project1/README.md")],
        cx.clone(),
    )
    .await;

    // Case 2: Test search without path matcher, matching both worktrees
    do_search_and_assert(
        &project,
        "project",
        Default::default(),
        true, // should be true in case of multiple worktrees
        &[path!("project1/README.md"), path!("project2/README.md")],
        cx.clone(),
    )
    .await;
}

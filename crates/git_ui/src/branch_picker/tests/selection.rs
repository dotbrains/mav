use super::*;

#[test]
fn test_select_branch_preserves_selected_remote_upstream_and_prioritizes_active_remote_branches() {
    let selected_branch = SharedString::from("origin/main");
    let branches: Arc<[Branch]> = Arc::from([
        create_test_branch_with_upstream(
            "feature",
            true,
            None,
            Some(1200),
            Some("refs/remotes/origin/feature"),
        ),
        create_test_branch_with_upstream(
            "main",
            false,
            None,
            Some(1100),
            Some("refs/remotes/origin/main"),
        ),
        create_test_branch("main", false, Some("origin"), Some(1000)),
        create_test_branch("feature", false, Some("origin"), Some(900)),
        create_test_branch("main", false, Some("fork"), Some(800)),
    ]);

    let processed_branches = process_branches(&branches, Some(&selected_branch));
    assert!(
        processed_branches
            .iter()
            .any(|branch| branch.name() == "origin/main"),
        "the selected remote branch should be preserved even when a local branch tracks it"
    );
    assert!(
        processed_branches
            .iter()
            .all(|branch| branch.name() != "origin/feature"),
        "the active branch's unselected remote upstream should still be collapsed"
    );

    let mut entries = processed_branches
        .into_iter()
        .map(|branch| Entry::Branch {
            branch,
            positions: Vec::new(),
        })
        .collect::<Vec<_>>();
    let selection_context = BranchSelectionContext {
        selected_branch: Some(selected_branch),
        active_branch_ref_name: Some("refs/heads/feature".into()),
        active_branch_upstream_ref_name: Some("refs/remotes/origin/feature".into()),
        active_branch_remote_name: Some("origin".into()),
    };

    sort_branch_entries(&mut entries, Some(&selection_context));

    let ordered_branch_names = entries.iter().map(Entry::name).collect::<Vec<_>>();
    assert_eq!(ordered_branch_names.first(), Some(&"origin/main"));
    assert!(
        ordered_branch_names.iter().position(|name| *name == "main")
            < ordered_branch_names
                .iter()
                .position(|name| *name == "fork/main"),
        "branches on the active branch's remote should be prioritized"
    );
}

async fn init_branch_list_test(
    repository: Option<Entity<Repository>>,
    branches: Vec<Branch>,
    cx: &mut TestAppContext,
) -> (Entity<BranchList>, VisualTestContext) {
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

    let window_handle = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let branch_list = window_handle
        .update(cx, |_multi_workspace, window, cx| {
            cx.new(|cx| {
                let mut delegate = BranchListDelegate::new(
                    workspace.downgrade(),
                    repository,
                    BranchListStyle::Modal,
                    BranchSelectionBehavior::Checkout,
                    cx,
                );
                delegate.all_branches = branches;
                let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx));
                let picker_focus_handle = picker.focus_handle(cx);
                picker.update(cx, |picker, _| {
                    picker.delegate.focus_handle = picker_focus_handle.clone();
                });

                let _subscription = cx.subscribe(&picker, |_, _, _, cx| {
                    cx.emit(DismissEvent);
                });

                BranchList {
                    picker,
                    picker_focus_handle,
                    _subscriptions: vec![_subscription],
                    embedded: false,
                }
            })
        })
        .unwrap();

    let cx = VisualTestContext::from_window(window_handle.into(), cx);

    (branch_list, cx)
}

async fn init_fake_repository_with_fs(
    cx: &mut TestAppContext,
) -> (Arc<FakeFs>, Entity<Project>, Entity<Repository>) {
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            ".git": {},
            "file.txt": "buffer_text".to_string()
        }),
    )
    .await;
    fs.set_head_for_repo(
        path!("/dir/.git").as_ref(),
        &[("file.txt", "test".to_string())],
        "deadbeef",
    );
    fs.set_index_for_repo(
        path!("/dir/.git").as_ref(),
        &[("file.txt", "index_text".to_string())],
    );

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let repository = cx.read(|cx| project.read(cx).active_repository(cx));

    (fs, project, repository.unwrap())
}

async fn init_fake_repository(cx: &mut TestAppContext) -> (Entity<Project>, Entity<Repository>) {
    let (_, project, repository) = init_fake_repository_with_fs(cx).await;
    (project, repository)
}

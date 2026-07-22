use super::*;

#[gpui::test]
async fn test_branch_filter_shows_all_then_remotes_and_applies_query(cx: &mut TestAppContext) {
    init_test(cx);

    let branches = vec![
        create_test_branch("main", true, Some("origin"), Some(1000)),
        create_test_branch("feature-auth", false, Some("fork"), Some(900)),
        create_test_branch("feature-ui", false, None, Some(800)),
        create_test_branch("develop", false, None, Some(700)),
    ];

    let (branch_list, mut ctx) = init_branch_list_test(None, branches, cx).await;
    let cx = &mut ctx;

    update_branch_list_matches_with_empty_query(&branch_list, cx).await;

    branch_list.update(cx, |branch_list, cx| {
        branch_list.picker.update(cx, |picker, _cx| {
            assert_eq!(picker.delegate.matches.len(), 4);

            let branches = picker
                .delegate
                .matches
                .iter()
                .map(|be| be.name())
                .collect::<HashSet<_>>();
            assert_eq!(
                branches,
                ["origin/main", "fork/feature-auth", "feature-ui", "develop"]
                    .into_iter()
                    .collect::<HashSet<_>>()
            );

            // Locals should be listed before remotes.
            let ordered = picker
                .delegate
                .matches
                .iter()
                .map(|be| be.name())
                .collect::<Vec<_>>();
            assert_eq!(
                ordered,
                vec!["feature-ui", "develop", "origin/main", "fork/feature-auth"]
            );

            // Verify the last entry is NOT the "create new branch" option
            let last_match = picker.delegate.matches.last().unwrap();
            assert!(!last_match.is_new_branch());
            assert!(!last_match.is_new_url());
        })
    });

    branch_list.update(cx, |branch_list, cx| {
        branch_list.picker.update(cx, |picker, _cx| {
            picker.delegate.branch_filter = BranchFilter::Remote;
        })
    });

    update_branch_list_matches_with_empty_query(&branch_list, cx).await;

    branch_list
        .update_in(cx, |branch_list, window, cx| {
            branch_list.picker.update(cx, |picker, cx| {
                assert_eq!(picker.delegate.matches.len(), 2);
                let branches = picker
                    .delegate
                    .matches
                    .iter()
                    .map(|be| be.name())
                    .collect::<HashSet<_>>();
                assert_eq!(
                    branches,
                    ["origin/main", "fork/feature-auth"]
                        .into_iter()
                        .collect::<HashSet<_>>()
                );

                // Verify the last entry is NOT the "create new branch" option
                let last_match = picker.delegate.matches.last().unwrap();
                assert!(!last_match.is_new_url());
                picker.delegate.branch_filter = BranchFilter::Remote;
                picker
                    .delegate
                    .update_matches(String::from("fork"), window, cx)
            })
        })
        .await;
    cx.run_until_parked();

    branch_list.update(cx, |branch_list, cx| {
        branch_list.picker.update(cx, |picker, _cx| {
            // Should have 1 existing branch + 1 "create new branch" entry = 2 total
            assert_eq!(picker.delegate.matches.len(), 2);
            assert!(
                picker
                    .delegate
                    .matches
                    .iter()
                    .any(|m| m.name() == "fork/feature-auth")
            );
            // Verify the last entry is the "create new branch" option
            let last_match = picker.delegate.matches.last().unwrap();
            assert!(last_match.is_new_branch());
        })
    });
}

#[gpui::test]
async fn test_new_branch_creation_with_query(cx: &mut TestAppContext) {
    const MAIN_BRANCH: &str = "main";
    const FEATURE_BRANCH: &str = "feature";
    const NEW_BRANCH: &str = "new-feature-branch";

    init_test(cx);
    let (_project, repository) = init_fake_repository(cx).await;

    let branches = vec![
        create_test_branch(MAIN_BRANCH, true, None, Some(1000)),
        create_test_branch(FEATURE_BRANCH, false, None, Some(900)),
    ];

    let (branch_list, mut ctx) = init_branch_list_test(repository.into(), branches, cx).await;
    let cx = &mut ctx;

    branch_list
        .update_in(cx, |branch_list, window, cx| {
            branch_list.picker.update(cx, |picker, cx| {
                // Surrounding the branch with whitespace allows us to then
                // assert that this whitespace is trimmed away.
                picker
                    .delegate
                    .update_matches(format!(" {NEW_BRANCH} "), window, cx)
            })
        })
        .await;

    cx.run_until_parked();

    branch_list.update_in(cx, |branch_list, window, cx| {
        branch_list.picker.update(cx, |picker, cx| {
            let last_match = picker.delegate.matches.last().unwrap();
            assert!(last_match.is_new_branch());
            assert_eq!(last_match.name(), NEW_BRANCH);
            // State is NewBranch because no existing branches fuzzy-match the query
            assert!(matches!(picker.delegate.state, PickerState::NewBranch));
            picker.delegate.confirm(false, window, cx);
        })
    });
    cx.run_until_parked();

    let branches = branch_list
        .update(cx, |branch_list, cx| {
            branch_list.picker.update(cx, |picker, cx| {
                picker
                    .delegate
                    .repo
                    .as_ref()
                    .unwrap()
                    .update(cx, |repo, _cx| repo.branches())
            })
        })
        .await
        .unwrap()
        .unwrap()
        .branches;

    let new_branch = branches
        .into_iter()
        .find(|branch| branch.name() == NEW_BRANCH)
        .expect("new-feature-branch should exist");
    assert_eq!(
        new_branch.ref_name.as_ref(),
        &format!("refs/heads/{NEW_BRANCH}"),
        "branch ref_name should not have duplicate refs/heads/ prefix"
    );
}

#[test]
fn test_normalize_branch_name() {
    assert_eq!(normalize_branch_name(" branch-name "), "branch-name");
    assert_eq!(normalize_branch_name("branch name"), "branch-name");
    assert_eq!(normalize_branch_name("  branch  name  "), "branch--name");
}

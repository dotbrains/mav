use super::*;

#[gpui::test]
async fn test_update_branch_matches_with_query(cx: &mut TestAppContext) {
    init_test(cx);

    let branches = create_test_branches();
    let (branch_list, mut ctx) = init_branch_list_test(None, branches, cx).await;
    let cx = &mut ctx;

    branch_list
        .update_in(cx, |branch_list, window, cx| {
            let query = "feature".to_string();
            branch_list.picker.update(cx, |picker, cx| {
                picker.delegate.update_matches(query, window, cx)
            })
        })
        .await;
    cx.run_until_parked();

    branch_list.update(cx, |branch_list, cx| {
        branch_list.picker.update(cx, |picker, _cx| {
            // Should have 2 existing branches + 1 "create new branch" entry = 3 total
            assert_eq!(picker.delegate.matches.len(), 3);
            assert!(
                picker
                    .delegate
                    .matches
                    .iter()
                    .any(|m| m.name() == "feature-auth")
            );
            assert!(
                picker
                    .delegate
                    .matches
                    .iter()
                    .any(|m| m.name() == "feature-ui")
            );
            // Verify the last entry is the "create new branch" option
            let last_match = picker.delegate.matches.last().unwrap();
            assert!(last_match.is_new_branch());
        })
    });
}

async fn update_branch_list_matches_with_empty_query(
    branch_list: &Entity<BranchList>,
    cx: &mut VisualTestContext,
) {
    branch_list
        .update_in(cx, |branch_list, window, cx| {
            branch_list.picker.update(cx, |picker, cx| {
                picker.delegate.update_matches(String::new(), window, cx)
            })
        })
        .await;
    cx.run_until_parked();
}

#[gpui::test]
async fn test_delete_branch(cx: &mut TestAppContext) {
    init_test(cx);
    let (_project, repository) = init_fake_repository(cx).await;

    let branches = create_test_branches();

    let branch_names = branches
        .iter()
        .map(|branch| branch.name().to_string())
        .collect::<Vec<String>>();
    let repo = repository.clone();
    cx.spawn(async move |mut cx| {
        for branch in branch_names {
            repo.update(&mut cx, |repo, _| repo.create_branch(branch, None))
                .await
                .unwrap()
                .unwrap();
        }
    })
    .await;
    cx.run_until_parked();

    let (branch_list, mut ctx) = init_branch_list_test(repository.into(), branches, cx).await;
    let cx = &mut ctx;

    update_branch_list_matches_with_empty_query(&branch_list, cx).await;

    let branch_to_delete = branch_list.update_in(cx, |branch_list, window, cx| {
        branch_list.picker.update(cx, |picker, cx| {
            assert_eq!(picker.delegate.matches.len(), 4);
            let branch_to_delete = picker.delegate.matches.get(1).unwrap().name().to_string();
            picker.delegate.delete_at(1, false, window, cx);
            branch_to_delete
        })
    });
    cx.run_until_parked();

    let expected_branches = ["main", "feature-auth", "feature-ui", "develop"]
        .into_iter()
        .filter(|name| name != &branch_to_delete)
        .collect::<HashSet<_>>();
    let repo_branches = branch_list
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
    let repo_branches = repo_branches
        .iter()
        .map(|b| b.name())
        .collect::<HashSet<_>>();
    assert_eq!(&repo_branches, &expected_branches);

    branch_list.update(cx, move |branch_list, cx| {
        branch_list.picker.update(cx, move |picker, _cx| {
            assert_eq!(picker.delegate.matches.len(), 3);
            let branches = picker
                .delegate
                .matches
                .iter()
                .map(|be| be.name())
                .collect::<HashSet<_>>();
            assert_eq!(branches, expected_branches);
        })
    });
}

#[gpui::test]
async fn test_delete_unmerged_branch_prompts_for_force_delete(cx: &mut TestAppContext) {
    init_test(cx);
    let (fs, _project, repository) = init_fake_repository_with_fs(cx).await;

    let branches = create_test_branches();
    let branch_names = branches
        .iter()
        .map(|branch| branch.name().to_string())
        .collect::<Vec<String>>();
    let repo = repository.clone();
    cx.spawn(async move |mut cx| {
        for branch in branch_names {
            repo.update(&mut cx, |repo, _| repo.create_branch(branch, None))
                .await
                .unwrap()
                .unwrap();
        }
    })
    .await;
    cx.run_until_parked();

    let branch_to_delete = "feature-auth";
    fs.with_git_state(path!("/dir/.git").as_ref(), true, |state| {
        state
            .branches_requiring_force_delete
            .insert(branch_to_delete.to_string());
    })
    .expect("failed to mark test branch as requiring force delete");

    let (branch_list, mut ctx) = init_branch_list_test(repository.into(), branches, cx).await;
    let cx = &mut ctx;
    update_branch_list_matches_with_empty_query(&branch_list, cx).await;

    branch_list.update_in(cx, |branch_list, window, cx| {
        branch_list.picker.update(cx, |picker, cx| {
            let branch_index = picker
                .delegate
                .matches
                .iter()
                .position(|entry| entry.name() == branch_to_delete)
                .unwrap();
            picker.delegate.delete_at(branch_index, false, window, cx);
        })
    });
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());

    cx.simulate_prompt_answer("Force Delete");
    cx.run_until_parked();

    let repo_branches = branch_list
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
    assert!(
        repo_branches
            .iter()
            .all(|branch| branch.name() != branch_to_delete)
    );
}

#[gpui::test]
async fn test_delete_unmerged_branch_cancel_keeps_branch(cx: &mut TestAppContext) {
    init_test(cx);
    let (fs, _project, repository) = init_fake_repository_with_fs(cx).await;

    let branches = create_test_branches();
    let branch_names = branches
        .iter()
        .map(|branch| branch.name().to_string())
        .collect::<Vec<String>>();
    let repo = repository.clone();
    cx.spawn(async move |mut cx| {
        for branch in branch_names {
            repo.update(&mut cx, |repo, _| repo.create_branch(branch, None))
                .await
                .unwrap()
                .unwrap();
        }
    })
    .await;
    cx.run_until_parked();

    let branch_to_delete = "feature-auth";
    fs.with_git_state(path!("/dir/.git").as_ref(), true, |state| {
        state
            .branches_requiring_force_delete
            .insert(branch_to_delete.to_string());
    })
    .expect("failed to mark test branch as requiring force delete");

    let (branch_list, mut ctx) = init_branch_list_test(repository.into(), branches, cx).await;
    let cx = &mut ctx;
    update_branch_list_matches_with_empty_query(&branch_list, cx).await;

    let initial_match_count = branch_list.update(cx, |branch_list, cx| {
        branch_list
            .picker
            .update(cx, |picker, _| picker.delegate.matches.len())
    });

    branch_list.update_in(cx, |branch_list, window, cx| {
        branch_list.picker.update(cx, |picker, cx| {
            let branch_index = picker
                .delegate
                .matches
                .iter()
                .position(|entry| entry.name() == branch_to_delete)
                .unwrap();
            picker.delegate.delete_at(branch_index, false, window, cx);
        })
    });
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());

    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());

    let repo_branches = branch_list
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
    assert!(
        repo_branches
            .iter()
            .any(|branch| branch.name() == branch_to_delete),
        "branch should still exist after cancelling the force-delete prompt"
    );

    let final_match_count = branch_list.update(cx, |branch_list, cx| {
        branch_list
            .picker
            .update(cx, |picker, _| picker.delegate.matches.len())
    });
    assert_eq!(
        initial_match_count, final_match_count,
        "picker matches should be unchanged after cancel"
    );
}

#[gpui::test]
async fn test_force_delete_click_deletes_branch_without_prompt(cx: &mut TestAppContext) {
    init_test(cx);
    let (fs, _project, repository) = init_fake_repository_with_fs(cx).await;

    let branches = create_test_branches();
    let branch_names = branches
        .iter()
        .map(|branch| branch.name().to_string())
        .collect::<Vec<String>>();
    let repo = repository.clone();
    cx.spawn(async move |mut cx| {
        for branch in branch_names {
            repo.update(&mut cx, |repo, _| repo.create_branch(branch, None))
                .await
                .unwrap()
                .unwrap();
        }
    })
    .await;
    cx.run_until_parked();

    let branch_to_delete = "feature-auth";
    fs.with_git_state(path!("/dir/.git").as_ref(), true, |state| {
        state
            .branches_requiring_force_delete
            .insert(branch_to_delete.to_string());
    })
    .expect("failed to mark test branch as requiring force delete");

    let (branch_list, mut ctx) = init_branch_list_test(repository.into(), branches, cx).await;
    let cx = &mut ctx;
    update_branch_list_matches_with_empty_query(&branch_list, cx).await;

    branch_list.update_in(cx, |branch_list, window, cx| {
        branch_list.picker.update(cx, |picker, cx| {
            picker.delegate.modifiers = Modifiers::alt();
            let branch_index = picker
                .delegate
                .matches
                .iter()
                .position(|entry| entry.name() == branch_to_delete)
                .unwrap();
            picker.delegate.delete_at(branch_index, true, window, cx);
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());

    let repo_branches = branch_list
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
    assert!(
        repo_branches
            .iter()
            .all(|branch| branch.name() != branch_to_delete)
    );
}

#[gpui::test]
async fn test_delete_remote_branch(cx: &mut TestAppContext) {
    init_test(cx);
    let (_project, repository) = init_fake_repository(cx).await;
    let branches = vec![
        create_test_branch("main", true, Some("origin"), Some(1000)),
        create_test_branch("feature-auth", false, Some("origin"), Some(900)),
        create_test_branch("feature-ui", false, Some("fork"), Some(800)),
        create_test_branch("develop", false, Some("private"), Some(700)),
    ];

    let branch_names = branches
        .iter()
        .map(|branch| branch.name().to_string())
        .collect::<Vec<String>>();
    let repo = repository.clone();
    cx.spawn(async move |mut cx| {
        for branch in branch_names {
            repo.update(&mut cx, |repo, _| repo.create_branch(branch, None))
                .await
                .unwrap()
                .unwrap();
        }
    })
    .await;
    cx.run_until_parked();

    let (branch_list, mut ctx) = init_branch_list_test(repository.into(), branches, cx).await;
    let cx = &mut ctx;
    // Enable remote filter
    branch_list.update(cx, |branch_list, cx| {
        branch_list.picker.update(cx, |picker, _cx| {
            picker.delegate.branch_filter = BranchFilter::Remote;
        });
    });
    update_branch_list_matches_with_empty_query(&branch_list, cx).await;

    // Check matches, it should match all existing branches and no option to create new branch
    let branch_to_delete = branch_list.update_in(cx, |branch_list, window, cx| {
        branch_list.picker.update(cx, |picker, cx| {
            assert_eq!(picker.delegate.matches.len(), 4);
            let branch_to_delete = picker.delegate.matches.get(1).unwrap().name().to_string();
            picker.delegate.delete_at(1, false, window, cx);
            branch_to_delete
        })
    });
    cx.run_until_parked();

    let expected_branches = [
        "origin/main",
        "origin/feature-auth",
        "fork/feature-ui",
        "private/develop",
    ]
    .into_iter()
    .filter(|name| name != &branch_to_delete)
    .collect::<HashSet<_>>();
    let repo_branches = branch_list
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
    let repo_branches = repo_branches
        .iter()
        .map(|b| b.name())
        .collect::<HashSet<_>>();
    assert_eq!(&repo_branches, &expected_branches);

    // Check matches, it should match one less branch than before
    branch_list.update(cx, move |branch_list, cx| {
        branch_list.picker.update(cx, move |picker, _cx| {
            assert_eq!(picker.delegate.matches.len(), 3);
            let branches = picker
                .delegate
                .matches
                .iter()
                .map(|be| be.name())
                .collect::<HashSet<_>>();
            assert_eq!(branches, expected_branches);
        })
    });
}

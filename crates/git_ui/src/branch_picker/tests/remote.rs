use super::*;

#[gpui::test]
async fn test_remote_url_detection_https(cx: &mut TestAppContext) {
    init_test(cx);
    let (_project, repository) = init_fake_repository(cx).await;
    let branches = vec![create_test_branch("main", true, None, Some(1000))];

    let (branch_list, mut ctx) = init_branch_list_test(repository.into(), branches, cx).await;
    let cx = &mut ctx;

    branch_list
        .update_in(cx, |branch_list, window, cx| {
            branch_list.picker.update(cx, |picker, cx| {
                let query = "https://github.com/user/repo.git".to_string();
                picker.delegate.update_matches(query, window, cx)
            })
        })
        .await;

    cx.run_until_parked();

    branch_list
        .update_in(cx, |branch_list, window, cx| {
            branch_list.picker.update(cx, |picker, cx| {
                let last_match = picker.delegate.matches.last().unwrap();
                assert!(last_match.is_new_url());
                assert!(matches!(picker.delegate.state, PickerState::NewRemote));
                picker.delegate.confirm(false, window, cx);
                assert_eq!(picker.delegate.matches.len(), 0);
                if let PickerState::CreateRemote(remote_url) = &picker.delegate.state
                    && remote_url.as_ref() == "https://github.com/user/repo.git"
                {
                } else {
                    panic!("wrong picker state");
                }
                picker
                    .delegate
                    .update_matches("my_new_remote".to_string(), window, cx)
            })
        })
        .await;

    cx.run_until_parked();

    branch_list.update_in(cx, |branch_list, window, cx| {
        branch_list.picker.update(cx, |picker, cx| {
            assert_eq!(picker.delegate.matches.len(), 1);
            assert!(matches!(
                picker.delegate.matches.first(),
                Some(Entry::NewRemoteName { name, url })
                    if name == "my_new_remote" && url.as_ref() == "https://github.com/user/repo.git"
            ));
            picker.delegate.confirm(false, window, cx);
        })
    });
    cx.run_until_parked();

    // List remotes
    let remotes = branch_list
        .update(cx, |branch_list, cx| {
            branch_list.picker.update(cx, |picker, cx| {
                picker
                    .delegate
                    .repo
                    .as_ref()
                    .unwrap()
                    .update(cx, |repo, _cx| repo.get_remotes(None, false))
            })
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        remotes,
        vec![Remote {
            name: SharedString::from("my_new_remote")
        }]
    );
}

#[gpui::test]
async fn test_confirm_remote_url_transitions(cx: &mut TestAppContext) {
    init_test(cx);

    let branches = vec![create_test_branch("main_branch", true, None, Some(1000))];
    let (branch_list, mut ctx) = init_branch_list_test(None, branches, cx).await;
    let cx = &mut ctx;

    branch_list
        .update_in(cx, |branch_list, window, cx| {
            branch_list.picker.update(cx, |picker, cx| {
                let query = "https://github.com/user/repo.git".to_string();
                picker.delegate.update_matches(query, window, cx)
            })
        })
        .await;
    cx.run_until_parked();

    // Try to create a new remote but cancel in the middle of the process
    branch_list
        .update_in(cx, |branch_list, window, cx| {
            branch_list.picker.update(cx, |picker, cx| {
                picker.delegate.selected_index = picker.delegate.matches.len() - 1;
                picker.delegate.confirm(false, window, cx);

                assert!(matches!(
                    picker.delegate.state,
                    PickerState::CreateRemote(_)
                ));
                if let PickerState::CreateRemote(ref url) = picker.delegate.state {
                    assert_eq!(url.as_ref(), "https://github.com/user/repo.git");
                }
                assert_eq!(picker.delegate.matches.len(), 0);
                picker.delegate.dismissed(window, cx);
                assert!(matches!(picker.delegate.state, PickerState::List));
                let query = "main".to_string();
                picker.delegate.update_matches(query, window, cx)
            })
        })
        .await;
    cx.run_until_parked();

    // Try to search a branch again to see if the state is restored properly
    branch_list.update(cx, |branch_list, cx| {
        branch_list.picker.update(cx, |picker, _cx| {
            // Should have 1 existing branch + 1 "create new branch" entry = 2 total
            assert_eq!(picker.delegate.matches.len(), 2);
            assert!(
                picker
                    .delegate
                    .matches
                    .iter()
                    .any(|m| m.name() == "main_branch")
            );
            // Verify the last entry is the "create new branch" option
            let last_match = picker.delegate.matches.last().unwrap();
            assert!(last_match.is_new_branch());
        })
    });
}

#[gpui::test]
async fn test_confirm_remote_url_does_not_dismiss(cx: &mut TestAppContext) {
    const REMOTE_URL: &str = "https://github.com/user/repo.git";

    init_test(cx);
    let branches = vec![create_test_branch("main", true, None, Some(1000))];

    let (branch_list, mut ctx) = init_branch_list_test(None, branches, cx).await;
    let cx = &mut ctx;

    let subscription = cx.update(|_, cx| {
        cx.subscribe(&branch_list, |_, _: &DismissEvent, _| {
            panic!("DismissEvent should not be emitted when confirming a remote URL");
        })
    });

    branch_list
        .update_in(cx, |branch_list, window, cx| {
            window.focus(&branch_list.picker_focus_handle, cx);
            assert!(
                branch_list.picker_focus_handle.is_focused(window),
                "Branch picker should be focused when selecting an entry"
            );

            branch_list.picker.update(cx, |picker, cx| {
                picker
                    .delegate
                    .update_matches(REMOTE_URL.to_string(), window, cx)
            })
        })
        .await;

    cx.run_until_parked();

    branch_list.update_in(cx, |branch_list, window, cx| {
            // Re-focus the picker since workspace initialization during run_until_parked
            window.focus(&branch_list.picker_focus_handle, cx);

            branch_list.picker.update(cx, |picker, cx| {
                let last_match = picker.delegate.matches.last().unwrap();
                assert!(last_match.is_new_url());
                assert!(matches!(picker.delegate.state, PickerState::NewRemote));

                picker.delegate.confirm(false, window, cx);

                assert!(
                    matches!(picker.delegate.state, PickerState::CreateRemote(ref url) if url.as_ref() == REMOTE_URL),
                    "State should transition to CreateRemote with the URL"
                );
            });

            assert!(
                branch_list.picker_focus_handle.is_focused(window),
                "Branch list picker should still be focused after confirming remote URL"
            );
        });

    cx.run_until_parked();

    drop(subscription);
}

#[gpui::test(iterations = 10)]
async fn test_empty_query_displays_all_branches(mut rng: StdRng, cx: &mut TestAppContext) {
    init_test(cx);
    let branch_count = rng.random_range(13..540);

    let branches: Vec<Branch> = (0..branch_count)
        .map(|i| create_test_branch(&format!("branch-{:02}", i), i == 0, None, Some(i * 100)))
        .collect();

    let (branch_list, mut ctx) = init_branch_list_test(None, branches, cx).await;
    let cx = &mut ctx;

    update_branch_list_matches_with_empty_query(&branch_list, cx).await;

    branch_list.update(cx, |branch_list, cx| {
        branch_list.picker.update(cx, |picker, _cx| {
            assert_eq!(picker.delegate.matches.len(), branch_count as usize);
        })
    });
}

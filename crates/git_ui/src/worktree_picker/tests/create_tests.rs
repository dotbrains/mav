use super::*;

#[gpui::test]
async fn test_remote_default_branch_is_preferred_create_target(cx: &mut TestAppContext) {
    let (_fs, worktree_picker, _repository, _worktree_path, mut cx) =
        init_worktree_picker_test(cx).await;

    worktree_picker.update(&mut cx, |worktree_picker, cx| {
        worktree_picker.picker.update(cx, |picker, _| {
            assert_eq!(picker.delegate.selected_index, 0);
            match picker.delegate.matches.first() {
                Some(WorktreeEntry::CreateFromDefaultBranch { default_branch }) => {
                    assert_eq!(default_branch.display_name(), "origin/main");
                }
                _ => panic!("remote default branch should be the first create target"),
            }
        })
    });

    let update_matches = worktree_picker.update_in(&mut cx, |worktree_picker, window, cx| {
        worktree_picker.picker.update(cx, |picker, cx| {
            picker
                .delegate
                .update_matches("feature".to_string(), window, cx)
        })
    });
    update_matches.await;
    cx.run_until_parked();

    worktree_picker.update(&mut cx, |worktree_picker, cx| {
        worktree_picker
            .picker
            .update(cx, |picker, _| match picker.delegate.matches.first() {
                Some(WorktreeEntry::CreateNamed {
                    from_branch: Some(default_branch),
                    ..
                }) => {
                    assert_eq!(default_branch.display_name(), "origin/main");
                }
                _ => panic!("named worktree creation should prefer the remote default branch"),
            })
    });
}

#[gpui::test]
async fn test_current_branch_create_target_is_shown_without_default_branch(
    cx: &mut TestAppContext,
) {
    let (_fs, worktree_picker, _repository, _worktree_path, mut cx) =
        init_worktree_picker_test(cx).await;

    worktree_picker.update_in(&mut cx, |worktree_picker, window, cx| {
        worktree_picker.picker.update(cx, |picker, cx| {
            picker.delegate.default_branch = None;
            picker.refresh(window, cx);
        });
    });
    cx.run_until_parked();

    worktree_picker.update(&mut cx, |worktree_picker, cx| {
        worktree_picker.picker.update(cx, |picker, _| {
            assert!(matches!(
                picker.delegate.matches.first(),
                Some(WorktreeEntry::CreateFromCurrentBranch)
            ));
            assert!(
                !picker
                    .delegate
                    .matches
                    .iter()
                    .any(|entry| matches!(entry, WorktreeEntry::CreateFromDefaultBranch { .. }))
            );
        });
    });
}

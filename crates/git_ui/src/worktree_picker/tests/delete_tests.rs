use super::*;

#[gpui::test]
async fn test_delete_worktree_marks_row_pending_immediately(cx: &mut TestAppContext) {
    let (_, worktree_picker, _repository, worktree_path, mut cx) =
        init_worktree_picker_test(cx).await;

    let index = worktree_index(&worktree_picker, &worktree_path, &mut cx);
    worktree_picker.update_in(&mut cx, |worktree_picker, window, cx| {
        worktree_picker.picker.update(cx, |picker, cx| {
            picker.delegate.delete_worktree(index, false, window, cx);
        })
    });

    let pending_paths = deleting_worktree_paths(&worktree_picker, &mut cx);
    assert_eq!(pending_paths.len(), 1);
    assert!(pending_paths.contains(&worktree_path));

    cx.run_until_parked();
}

#[gpui::test]
async fn test_delete_worktree_clears_pending_and_removes_row_on_success(cx: &mut TestAppContext) {
    let (_, worktree_picker, repository, worktree_path, mut cx) =
        init_worktree_picker_test(cx).await;

    let index = worktree_index(&worktree_picker, &worktree_path, &mut cx);
    worktree_picker.update_in(&mut cx, |worktree_picker, window, cx| {
        worktree_picker.picker.update(cx, |picker, cx| {
            picker.delegate.delete_worktree(index, false, window, cx);
        })
    });
    assert!(deleting_worktree_paths(&worktree_picker, &mut cx).contains(&worktree_path));

    cx.run_until_parked();

    assert!(deleting_worktree_paths(&worktree_picker, &mut cx).is_empty());
    assert!(!picker_contains_worktree(
        &worktree_picker,
        &worktree_path,
        &mut cx
    ));
    assert!(
        !repo_contains_worktree(&repository, &worktree_path, &mut cx).await,
        "worktree should be removed after successful delete"
    );
}
#[gpui::test]
async fn test_delete_dirty_worktree_prompts_for_force_delete(cx: &mut TestAppContext) {
    let (fs, worktree_picker, repository, worktree_path, mut cx) =
        init_worktree_picker_test(cx).await;

    fs.with_git_state(path!("/root/project/.git").as_ref(), true, |state| {
        state
            .worktrees_requiring_force_delete
            .insert(worktree_path.clone());
    })
    .expect("failed to mark test worktree as requiring force delete");

    let index = worktree_index(&worktree_picker, &worktree_path, &mut cx);
    worktree_picker.update_in(&mut cx, |worktree_picker, window, cx| {
        worktree_picker.picker.update(cx, |picker, cx| {
            picker.delegate.delete_worktree(index, false, window, cx);
        })
    });
    assert!(deleting_worktree_paths(&worktree_picker, &mut cx).contains(&worktree_path));

    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    assert!(
        !deleting_worktree_paths(&worktree_picker, &mut cx).contains(&worktree_path),
        "pending delete state should clear while waiting for force-delete confirmation"
    );

    cx.simulate_prompt_answer("Force Delete");
    cx.run_until_parked();

    assert!(!cx.has_pending_prompt());
    assert!(deleting_worktree_paths(&worktree_picker, &mut cx).is_empty());
    assert!(!picker_contains_worktree(
        &worktree_picker,
        &worktree_path,
        &mut cx
    ));
    assert!(
        !repo_contains_worktree(&repository, &worktree_path, &mut cx).await,
        "worktree should be removed after confirming force delete"
    );
}

#[gpui::test]
async fn test_duplicate_delete_worktree_is_ignored_while_pending(cx: &mut TestAppContext) {
    let (fs, worktree_picker, _repository, worktree_path, mut cx) =
        init_worktree_picker_test(cx).await;

    fs.with_git_state(path!("/root/project/.git").as_ref(), true, |state| {
        state
            .worktrees_requiring_force_delete
            .insert(worktree_path.clone());
    })
    .expect("failed to mark test worktree as requiring force delete");

    let index = worktree_index(&worktree_picker, &worktree_path, &mut cx);
    worktree_picker.update_in(&mut cx, |worktree_picker, window, cx| {
        worktree_picker.picker.update(cx, |picker, cx| {
            picker.delegate.delete_worktree(index, false, window, cx);
            picker.delegate.delete_worktree(index, false, window, cx);
        })
    });

    let pending_paths = deleting_worktree_paths(&worktree_picker, &mut cx);
    assert_eq!(pending_paths.len(), 1);
    assert!(pending_paths.contains(&worktree_path));

    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    assert!(deleting_worktree_paths(&worktree_picker, &mut cx).is_empty());

    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();

    assert!(!cx.has_pending_prompt());
    assert!(picker_contains_worktree(
        &worktree_picker,
        &worktree_path,
        &mut cx
    ));
}

#[gpui::test]
async fn test_selected_deleting_worktree_cannot_be_opened(cx: &mut TestAppContext) {
    let (_, worktree_picker, _repository, worktree_path, mut cx) =
        init_worktree_picker_test(cx).await;

    let subscription = cx.update(|_, cx| {
        cx.subscribe(&worktree_picker, |_, _: &DismissEvent, _| {
            panic!("DismissEvent should not be emitted for a deleting worktree");
        })
    });

    let index = worktree_index(&worktree_picker, &worktree_path, &mut cx);
    worktree_picker.update_in(&mut cx, |worktree_picker, window, cx| {
        worktree_picker.picker.update(cx, |picker, cx| {
            picker.delegate.selected_index = index;
            picker.delegate.delete_worktree(index, false, window, cx);
            picker.delegate.confirm(false, window, cx);
        })
    });

    assert!(deleting_worktree_paths(&worktree_picker, &mut cx).contains(&worktree_path));

    drop(subscription);
    cx.run_until_parked();
}

#[gpui::test]
async fn test_force_delete_worktree_deletes_without_prompt(cx: &mut TestAppContext) {
    let (fs, worktree_picker, repository, worktree_path, mut cx) =
        init_worktree_picker_test(cx).await;

    fs.with_git_state(path!("/root/project/.git").as_ref(), true, |state| {
        state
            .worktrees_requiring_force_delete
            .insert(worktree_path.clone());
    })
    .expect("failed to mark test worktree as requiring force delete");

    let index = worktree_index(&worktree_picker, &worktree_path, &mut cx);
    worktree_picker.update_in(&mut cx, |worktree_picker, window, cx| {
        worktree_picker.picker.update(cx, |picker, cx| {
            picker.delegate.modifiers = Modifiers::alt();
            picker.delegate.delete_worktree(index, true, window, cx);
        })
    });
    assert!(deleting_worktree_paths(&worktree_picker, &mut cx).contains(&worktree_path));

    cx.run_until_parked();

    assert!(!cx.has_pending_prompt());
    assert!(deleting_worktree_paths(&worktree_picker, &mut cx).is_empty());
    assert!(!picker_contains_worktree(
        &worktree_picker,
        &worktree_path,
        &mut cx
    ));
    assert!(
        !repo_contains_worktree(&repository, &worktree_path, &mut cx).await,
        "worktree should be removed by explicit force delete"
    );
}

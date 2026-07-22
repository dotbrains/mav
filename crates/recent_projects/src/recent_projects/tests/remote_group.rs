use super::*;

#[gpui::test]
async fn test_remote_project_group_confirm_does_not_create_local_workspace(
    cx: &mut TestAppContext,
) {
    // Regression test: confirming a ProjectGroup entry with a remote host
    // should call find_or_create_workspace with the host, not
    // find_or_create_local_workspace.
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree("/local", json!({}))
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from("/local")],
            app_state,
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    cx.run_until_parked();

    let mw = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());
    let remote_key = remote_project_group(1);

    // Get workspace info via WindowHandle::read_with (returns Result)
    let (workspace, groups, fh) = mw
        .read_with(cx, |mw, _cx| {
            let ws = mw.workspace().clone();
            (
                ws.clone(),
                mw.project_group_keys(),
                ws.read(_cx).focus_handle(_cx),
            )
        })
        .unwrap();

    let mut augmented_groups = groups.clone();
    augmented_groups.push(remote_key.clone());

    // Create the popover (same as the title bar does)
    let popover: Entity<RecentProjects> = cx.update(|cx| {
        let window = cx.windows()[0];
        window
            .update(cx, |_, window, cx| {
                RecentProjects::popover(
                    workspace.downgrade(),
                    augmented_groups,
                    Some(false),
                    fh,
                    window,
                    cx,
                )
            })
            .unwrap()
    });

    cx.run_until_parked();

    // Get the picker from the popover
    let picker: Entity<Picker<RecentProjectsDelegate>> = cx.update(|cx| {
        let window = cx.windows()[0];
        window
            .update(cx, |_, _window, cx| popover.read(cx).picker.clone())
            .unwrap()
    });

    cx.run_until_parked();

    // Find the remote project group entry index via Entity::read_with (no unwrap)
    let filtered = picker.read_with(cx, |p, _| p.delegate.filtered_entries.clone());
    let remote_idx = filtered
            .iter()
            .position(|entry| {
                matches!(entry, ProjectPickerEntry::ProjectGroup(m) if m.candidate_id == groups.len())
            })
            .expect("remote project group entry should exist");

    // Select and confirm the remote entry via Entity::update
    let _ = cx.update(|cx| {
        let window = cx.windows()[0];
        window.update(cx, |_, window, cx| {
            picker.update(cx, |picker, cx| {
                picker.delegate.set_selected_index(remote_idx, window, cx);
                picker.delegate.confirm(false, window, cx);
            });
        })
    });

    cx.run_until_parked();

    // Verify no local workspace was created for the remote paths
    let has_local = mw
        .read_with(cx, |mw, cx| {
            mw.workspace_for_paths(remote_key.path_list(), None, cx)
                .is_some()
        })
        .unwrap();
    assert!(
        !has_local,
        "remote project group confirm should not create a local workspace"
    );
}

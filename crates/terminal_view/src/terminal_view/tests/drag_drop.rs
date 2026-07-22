use super::*;
use common::*;

// Terminal drag/drop test

#[gpui::test]
async fn test_handle_drop_writes_paths_for_all_drop_types(cx: &mut TestAppContext) {
    let (project, _workspace, window_handle) = init_test_with_window(cx).await;

    let (worktree, _) = create_folder_wt(project.clone(), "/root/", cx).await;
    let first_entry = create_file_in_worktree(worktree.clone(), "first.txt", cx).await;
    let second_entry = create_file_in_worktree(worktree.clone(), "second.txt", cx).await;

    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
    let first_path = project
        .read_with(cx, |project, cx| {
            project.absolute_path(
                &ProjectPath {
                    worktree_id,
                    path: first_entry.path.clone(),
                },
                cx,
            )
        })
        .unwrap();
    let second_path = project
        .read_with(cx, |project, cx| {
            project.absolute_path(
                &ProjectPath {
                    worktree_id,
                    path: second_entry.path.clone(),
                },
                cx,
            )
        })
        .unwrap();

    let (active_pane, terminal, terminal_view) =
        add_display_only_terminal(&project, window_handle, false, cx);

    let tab_item = window_handle
        .update(cx, |_, window, cx| {
            let tab_project_item = cx.new(|_| TestProjectItem {
                entry_id: Some(second_entry.id),
                project_path: Some(ProjectPath {
                    worktree_id,
                    path: second_entry.path.clone(),
                }),
                is_dirty: false,
            });
            let tab_item = cx.new(|cx| TestItem::new(cx).with_project_items(&[tab_project_item]));
            active_pane.update(cx, |pane, cx| {
                pane.add_item(Box::new(tab_item.clone()), true, false, None, window, cx);
            });
            tab_item
        })
        .unwrap();

    cx.run_until_parked();

    window_handle
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspace().clone();
            let terminal_view_index = active_pane.read(cx).index_for_item(&terminal_view).unwrap();
            let dragged_tab_index = active_pane.read(cx).index_for_item(&tab_item).unwrap();

            assert!(
                workspace.read(cx).pane_for(&terminal_view).is_some(),
                "terminal view not registered with workspace after run_until_parked"
            );

            // Dragging an external file should write its path to the terminal
            let external_paths = ExternalPaths(vec![first_path.clone()].into());
            assert_drop_writes_to_terminal(
                &active_pane,
                terminal_view_index,
                &terminal,
                &external_paths,
                &expected_drop_text(std::slice::from_ref(&first_path)),
                window,
                cx,
            );

            // Dragging a tab should write the path of the tab's item to the terminal
            let dragged_tab = DraggedTab {
                pane: active_pane.clone(),
                item: Box::new(tab_item.clone()),
                ix: dragged_tab_index,
                detail: 0,
                is_active: false,
            };
            assert_drop_writes_to_terminal(
                &active_pane,
                terminal_view_index,
                &terminal,
                &dragged_tab,
                &expected_drop_text(std::slice::from_ref(&second_path)),
                window,
                cx,
            );

            // Dragging multiple selections should write both paths to the terminal
            let dragged_selection = DraggedSelection {
                active_selection: SelectedEntry {
                    worktree_id,
                    entry_id: first_entry.id,
                },
                marked_selections: Arc::from([
                    SelectedEntry {
                        worktree_id,
                        entry_id: first_entry.id,
                    },
                    SelectedEntry {
                        worktree_id,
                        entry_id: second_entry.id,
                    },
                ]),
                source_pane: None,
                active_selection_is_file: true,
            };
            assert_drop_writes_to_terminal(
                &active_pane,
                terminal_view_index,
                &terminal,
                &dragged_selection,
                &expected_drop_text(&[first_path.clone(), second_path.clone()]),
                window,
                cx,
            );

            // Dropping a project entry should write the entry's path to the terminal
            let dropped_entry_id = first_entry.id;
            assert_drop_writes_to_terminal(
                &active_pane,
                terminal_view_index,
                &terminal,
                &dropped_entry_id,
                &expected_drop_text(&[first_path]),
                window,
                cx,
            );
        })
        .unwrap();
}

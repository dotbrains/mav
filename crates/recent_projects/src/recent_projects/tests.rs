use super::*;

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, UpdateGlobal, VisualTestContext};

    use serde_json::json;
    use settings::SettingsStore;
    use util::path;
    use workspace::{AppState, open_paths};

    use super::*;

    // Test picker for the empty query:
    //
    //   [0] Header("Current Folders")
    //   [1] OpenFolder(0)
    //   [2] OpenFolder(1)
    //   [3] Header("This Window")
    //   [4] ProjectGroup(0)
    //   [5] ProjectGroup(1)
    //   [6] Header("Recent Projects")
    //   [7..=26] RecentProject(0..=19)
    //
    const RECENT_PROJECT_COUNT: usize = 20;
    const FIRST_RECENT_PROJECT: usize = 7;
    const LAST_RECENT_PROJECT: usize = FIRST_RECENT_PROJECT + RECENT_PROJECT_COUNT - 1;

    fn open_folder(index: usize) -> OpenFolderEntry {
        OpenFolderEntry {
            worktree_id: WorktreeId::from_usize(index),
            name: format!("project-folder-{index}").into(),
            path: PathBuf::from(format!("/current/project-folder-{index}")),
            branch: None,
            is_active: false,
            connection_options: None,
        }
    }

    fn project_group(index: usize) -> ProjectGroupKey {
        ProjectGroupKey::new(
            None,
            PathList::new(&[PathBuf::from(format!("/this-window/project-{index}"))]),
        )
    }

    fn remote_project_group(index: usize) -> ProjectGroupKey {
        ProjectGroupKey::new(
            Some(RemoteConnectionOptions::Mock(
                remote::MockConnectionOptions { id: index as u64 },
            )),
            PathList::new(&[PathBuf::from(format!(
                "/this-window/remote-project-{index}"
            ))]),
        )
    }

    fn recent_workspace(index: usize) -> RecentWorkspace {
        let paths = PathList::new(&[PathBuf::from(format!("/recent/project-{index:02}"))]);
        RecentWorkspace {
            workspace_id: WorkspaceId::from_i64(index as i64),
            location: SerializedWorkspaceLocation::Local,
            paths: paths.clone(),
            identity_paths: paths,
            timestamp: Utc::now(),
        }
    }

    fn recent_workspaces() -> Vec<RecentWorkspace> {
        (0..RECENT_PROJECT_COUNT).map(recent_workspace).collect()
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear());
    }

    fn build_picker(
        cx: &mut TestAppContext,
    ) -> (
        Entity<Picker<RecentProjectsDelegate>>,
        &mut VisualTestContext,
    ) {
        init_test(cx);
        let (picker, cx) = cx.add_window_view(|window, cx| {
            let mut delegate = RecentProjectsDelegate::new(
                WeakEntity::new_invalid(),
                false,
                cx.focus_handle(),
                vec![open_folder(0), open_folder(1)],
                vec![project_group(0), project_group(1)],
                ProjectPickerStyle::Modal,
            );
            delegate.set_workspaces(recent_workspaces());
            Picker::list(delegate, window, cx)
                .list_measure_all()
                .show_scrollbar(true)
                .max_height(Rems::from_pixels(px(240.0), window))
        });
        draw(cx);
        (picker, cx)
    }

    fn scroll_to_and_select(
        picker: &Entity<Picker<RecentProjectsDelegate>>,
        cx: &mut VisualTestContext,
        index: usize,
    ) -> usize {
        picker.update_in(cx, |picker, window, cx| {
            picker.set_selected_index(index, None, true, window, cx);
        });
        draw(cx);
        picker.update(cx, |picker, _| picker.logical_scroll_top_index())
    }

    fn delete_recent_project_in_picker(
        picker: &Entity<Picker<RecentProjectsDelegate>>,
        cx: &mut VisualTestContext,
        index: usize,
    ) {
        picker.update_in(cx, |picker, window, cx| {
            let Some(ProjectPickerEntry::RecentProject(hit)) =
                picker.delegate.filtered_entries.get(index)
            else {
                panic!("expected entry at {index} to be a recent project");
            };
            let mut workspaces = picker.delegate.workspaces.clone();
            workspaces.remove(hit.candidate_id);
            RecentProjectsDelegate::update_picker_after_recent_project_deletion(
                picker, index, workspaces, window, cx,
            );
        });
    }

    #[track_caller]
    fn assert_scroll_top_is(
        picker: &Entity<Picker<RecentProjectsDelegate>>,
        cx: &mut VisualTestContext,
        expected: usize,
        phase: &str,
    ) {
        picker.update(cx, |picker, _| {
            assert_eq!(
                picker.logical_scroll_top_index(),
                expected,
                "scroll top should remain at {expected} ({phase})"
            );
            assert_selected_entry_is_recent_project(picker);
        });
    }

    #[track_caller]
    fn assert_pinned_to_bottom(
        picker: &Entity<Picker<RecentProjectsDelegate>>,
        cx: &mut VisualTestContext,
        phase: &str,
    ) {
        picker.update(cx, |picker, _| {
            assert_eq!(
                picker.is_scrolled_to_end(),
                Some(true),
                "picker should remain pinned to the bottom ({phase})"
            );
            assert!(
                picker.logical_scroll_top_index() > 0,
                "picker should not jump to the top while pinned to the bottom ({phase})"
            );
            assert_selected_entry_is_recent_project(picker);
        });
    }

    #[track_caller]
    fn assert_selected_entry_is_recent_project(picker: &Picker<RecentProjectsDelegate>) {
        assert!(matches!(
            picker
                .delegate
                .filtered_entries
                .get(picker.delegate.selected_index),
            Some(ProjectPickerEntry::RecentProject(_))
        ));
    }

    fn init_test(cx: &mut TestAppContext) -> Arc<AppState> {
        cx.update(|cx| {
            let state = AppState::test(cx);
            crate::init(cx);
            editor::init(cx);
            state
        })
    }

    mod dev_container;
    mod local_project;
    mod remote_group;
    #[gpui::test]
    fn this_window_project_icons_use_each_project_group_host(cx: &mut TestAppContext) {
        init_test(cx);

        let mut delegate = RecentProjectsDelegate::new(
            WeakEntity::new_invalid(),
            false,
            cx.update(|cx| cx.focus_handle()),
            Vec::new(),
            vec![project_group(0), remote_project_group(1)],
            ProjectPickerStyle::Modal,
        );
        delegate.filtered_entries = vec![
            ProjectPickerEntry::ProjectGroup(StringMatch {
                candidate_id: 0,
                score: 0.0,
                positions: Vec::new(),
                string: Default::default(),
            }),
            ProjectPickerEntry::ProjectGroup(StringMatch {
                candidate_id: 1,
                score: 0.0,
                positions: Vec::new(),
                string: Default::default(),
            }),
        ];

        assert!(!delegate.entry_is_remote_project(&delegate.filtered_entries[0]));
        assert!(delegate.entry_is_remote_project(&delegate.filtered_entries[1]));
        assert!(delegate.filtered_entries_include_remote_project());
        assert_eq!(
            icon_for_project_group(&delegate.window_project_groups[0]),
            IconName::Screen
        );
        assert_eq!(
            icon_for_project_group(&delegate.window_project_groups[1]),
            IconName::Server
        );
    }

    #[gpui::test]
    fn deleting_top_recent_project_preserves_scroll_position(cx: &mut TestAppContext) {
        let target = FIRST_RECENT_PROJECT;
        let (picker, cx) = build_picker(cx);
        let scroll_top = scroll_to_and_select(&picker, cx, target);
        assert!(
            scroll_top > 0,
            "test should start scrolled away from the top"
        );

        delete_recent_project_in_picker(&picker, cx, target);
        assert_scroll_top_is(&picker, cx, scroll_top, "after delete");

        // The picker re-runs layout on the next frame; the scroll position
        // must still be preserved after that redraw.
        draw(cx);
        assert_scroll_top_is(&picker, cx, scroll_top, "after redraw");
    }

    #[gpui::test]
    fn deleting_middle_recent_project_preserves_scroll_position(cx: &mut TestAppContext) {
        let target = FIRST_RECENT_PROJECT + RECENT_PROJECT_COUNT / 2;
        let (picker, cx) = build_picker(cx);
        let scroll_top = scroll_to_and_select(&picker, cx, target);
        assert!(
            scroll_top > 0,
            "test should start scrolled away from the top"
        );

        delete_recent_project_in_picker(&picker, cx, target);
        assert_scroll_top_is(&picker, cx, scroll_top, "after delete");

        draw(cx);
        assert_scroll_top_is(&picker, cx, scroll_top, "after redraw");
    }

    #[gpui::test]
    fn deleting_last_recent_project_preserves_scroll_position(cx: &mut TestAppContext) {
        let target = LAST_RECENT_PROJECT;
        let (picker, cx) = build_picker(cx);
        scroll_to_and_select(&picker, cx, target);

        picker.update(cx, |picker, _| {
            assert_eq!(
                picker.is_scrolled_to_end(),
                Some(true),
                "selecting the last entry should leave the picker pinned to the bottom"
            );
        });

        delete_recent_project_in_picker(&picker, cx, target);
        assert_pinned_to_bottom(&picker, cx, "after delete");

        draw(cx);
        assert_pinned_to_bottom(&picker, cx, "after redraw");
    }
}

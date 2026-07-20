use super::*;

#[gpui::test]
async fn test_flexible_panel_left_dock_sizing(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    workspace.update(cx, |workspace, _cx| {
        workspace.bounds.size.width = px(900.);
    });

    // Step 1: Add a tab to the center pane then open a flexible panel in the left
    // dock. With one full-width center pane the default ratio is 0.5, so the panel
    // and the center pane each take half the workspace width.
    workspace.update_in(cx, |workspace, window, cx| {
        let item = cx.new(|cx| {
            TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "one.txt", cx)])
        });
        workspace.add_item_to_active_pane(Box::new(item), None, true, window, cx);

        let panel = cx.new(|cx| TestPanel::new_flexible(DockPosition::Left, 100, cx));
        workspace.add_panel(panel, window, cx);
        workspace.toggle_dock(DockPosition::Left, window, cx);

        let left_dock = workspace.left_dock().read(cx);
        let left_width = workspace
            .dock_size(&left_dock, window, cx)
            .expect("left dock should have an active panel");

        assert_eq!(
            left_width,
            workspace.bounds.size.width / 2.,
            "flexible left panel should split evenly with the center pane"
        );
    });

    // Step 2: Split the center pane left/right. The flexible panel is treated as one
    // average center column, so with two center columns it should take one third of
    // the workspace width.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(
            workspace.active_pane().clone(),
            SplitDirection::Right,
            window,
            cx,
        );

        let left_dock = workspace.left_dock().read(cx);
        let left_width = workspace
            .dock_size(&left_dock, window, cx)
            .expect("left dock should still have an active panel after horizontal split");

        assert_eq!(
            left_width,
            workspace.bounds.size.width / 3.,
            "flexible left panel width should match the average center column width"
        );
    });

    // Step 3: Split the active center pane vertically (top/bottom). Vertical splits do
    // not change the number of center columns, so the flexible panel width stays the same.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(
            workspace.active_pane().clone(),
            SplitDirection::Down,
            window,
            cx,
        );

        let left_dock = workspace.left_dock().read(cx);
        let left_width = workspace
            .dock_size(&left_dock, window, cx)
            .expect("left dock should still have an active panel after vertical split");

        assert_eq!(
            left_width,
            workspace.bounds.size.width / 3.,
            "flexible left panel width should still match the average center column width"
        );
    });

    // Step 4: Open a fixed-width panel in the right dock. The right dock's default
    // size reduces the available width, so the flexible left panel keeps matching one
    // average center column within the remaining space.
    workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| TestPanel::new(DockPosition::Right, 200, cx));
        workspace.add_panel(panel, window, cx);
        workspace.toggle_dock(DockPosition::Right, window, cx);

        let right_dock = workspace.right_dock().read(cx);
        let right_width = workspace
            .dock_size(&right_dock, window, cx)
            .expect("right dock should have an active panel");

        let left_dock = workspace.left_dock().read(cx);
        let left_width = workspace
            .dock_size(&left_dock, window, cx)
            .expect("left dock should still have an active panel");

        let available_width = workspace.bounds.size.width - right_width;
        assert_eq!(
            left_width,
            available_width / 3.,
            "flexible left panel should keep matching one average center column"
        );
    });

    // Step 5: Toggle the right dock's panel to flexible. Now both docks use
    // column-equivalent flex sizing and the workspace width is divided among
    // left-flex, two center columns, and right-flex.
    workspace.update_in(cx, |workspace, window, cx| {
        let right_dock = workspace.right_dock().clone();
        let right_panel = right_dock
            .read(cx)
            .visible_panel()
            .expect("right dock should have a visible panel")
            .clone();
        workspace.toggle_dock_panel_flexible_size(&right_dock, right_panel.as_ref(), window, cx);

        let right_dock = right_dock.read(cx);
        let right_panel = right_dock
            .visible_panel()
            .expect("right dock should still have a visible panel");
        assert!(
            right_panel.has_flexible_size(window, cx),
            "right panel should now be flexible"
        );

        let right_size_state = right_dock
            .stored_panel_size_state(right_panel.as_ref())
            .expect("right panel should have a stored size state after toggling");
        let right_flex = right_size_state
            .flex
            .expect("right panel should have a flex value after toggling");

        let left_dock = workspace.left_dock().read(cx);
        let left_width = workspace
            .dock_size(&left_dock, window, cx)
            .expect("left dock should still have an active panel");
        let right_width = workspace
            .dock_size(&right_dock, window, cx)
            .expect("right dock should still have an active panel");

        let left_flex = workspace
            .default_dock_flex(DockPosition::Left)
            .expect("left dock should have a default flex");
        let center_column_count = workspace.center.full_height_column_count() as f32;

        let total_flex = left_flex + center_column_count + right_flex;
        let expected_left = left_flex / total_flex * workspace.bounds.size.width;
        let expected_right = right_flex / total_flex * workspace.bounds.size.width;
        assert_eq!(
            left_width, expected_left,
            "flexible left panel should share workspace width via flex ratios"
        );
        assert_eq!(
            right_width, expected_right,
            "flexible right panel should share workspace width via flex ratios"
        );
    });
}

struct TestModal(FocusHandle);

impl TestModal {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self(cx.focus_handle())
    }
}

impl EventEmitter<DismissEvent> for TestModal {}

impl Focusable for TestModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.0.clone()
    }
}

impl ModalView for TestModal {}

impl Render for TestModal {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<TestModal>) -> impl IntoElement {
        div().track_focus(&self.0)
    }
}

// Registers its focus handle as a reopenable picker on construction, like a real
// `Picker` does, so the modal layer recognizes it by focus identity.
struct ReopenableTestModal(FocusHandle);

impl ReopenableTestModal {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        register_reopenable_picker(&focus_handle, cx);
        Self(focus_handle)
    }
}

impl EventEmitter<DismissEvent> for ReopenableTestModal {}

impl Focusable for ReopenableTestModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.0.clone()
    }
}

impl ModalView for ReopenableTestModal {}

impl Render for ReopenableTestModal {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<ReopenableTestModal>,
    ) -> impl IntoElement {
        div().track_focus(&self.0)
    }
}

use super::*;

impl Workspace {
    pub(crate) fn adjust_padding(padding: Option<f32>) -> f32 {
        padding
            .unwrap_or(CenteredPaddingSettings::default().0)
            .clamp(
                CenteredPaddingSettings::MIN_PADDING,
                CenteredPaddingSettings::MAX_PADDING,
            )
    }

    pub fn for_window(window: &Window, cx: &App) -> Option<Entity<Workspace>> {
        window
            .root::<MultiWorkspace>()
            .flatten()
            .map(|multi_workspace| multi_workspace.read(cx).workspace().clone())
    }

    pub fn zoomed_item(&self) -> Option<&AnyWeakView> {
        self.zoomed.as_ref()
    }

    pub fn activate_next_window(&mut self, cx: &mut Context<Self>) {
        let Some(current_window_id) = cx.active_window().map(|a| a.window_id()) else {
            return;
        };
        let windows = cx.windows();
        let next_window =
            SystemWindowTabController::get_next_tab_group_window(cx, current_window_id).or_else(
                || {
                    windows
                        .iter()
                        .cycle()
                        .skip_while(|window| window.window_id() != current_window_id)
                        .nth(1)
                },
            );

        if let Some(window) = next_window {
            window
                .update(cx, |_, window, _| window.activate_window())
                .ok();
        }
    }

    pub fn activate_previous_window(&mut self, cx: &mut Context<Self>) {
        let Some(current_window_id) = cx.active_window().map(|a| a.window_id()) else {
            return;
        };
        let windows = cx.windows();
        let prev_window =
            SystemWindowTabController::get_prev_tab_group_window(cx, current_window_id).or_else(
                || {
                    windows
                        .iter()
                        .rev()
                        .cycle()
                        .skip_while(|window| window.window_id() != current_window_id)
                        .nth(1)
                },
            );

        if let Some(window) = prev_window {
            window
                .update(cx, |_, window, _| window.activate_window())
                .ok();
        }
    }

    pub(crate) fn resize_left_dock(&mut self, new_size: Pixels, window: &mut Window, cx: &mut App) {
        let workspace_width = self.bounds.size.width;
        let mut size = new_size.min(workspace_width - RESIZE_HANDLE_SIZE);

        self.right_dock.read_with(cx, |right_dock, cx| {
            let right_dock_size = right_dock
                .stored_active_panel_size(window, cx)
                .unwrap_or(Pixels::ZERO);
            if right_dock_size + size > workspace_width {
                size = workspace_width - right_dock_size
            }
        });

        let flex_grow = self.dock_flex_for_size(DockPosition::Left, size, window, cx);
        self.left_dock.update(cx, |left_dock, cx| {
            if WorkspaceSettings::get_global(cx)
                .resize_all_panels_in_dock
                .contains(&DockPosition::Left)
            {
                left_dock.resize_all_panels(Some(size), flex_grow, window, cx);
            } else {
                left_dock.resize_active_panel(Some(size), flex_grow, window, cx);
            }
        });
    }

    pub(crate) fn resize_right_dock(
        &mut self,
        new_size: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        let workspace_width = self.bounds.size.width;
        let mut size = new_size.min(workspace_width - RESIZE_HANDLE_SIZE);
        self.left_dock.read_with(cx, |left_dock, cx| {
            let left_dock_size = left_dock
                .stored_active_panel_size(window, cx)
                .unwrap_or(Pixels::ZERO);
            if left_dock_size + size > workspace_width {
                size = workspace_width - left_dock_size
            }
        });
        let flex_grow = self.dock_flex_for_size(DockPosition::Right, size, window, cx);
        self.right_dock.update(cx, |right_dock, cx| {
            if WorkspaceSettings::get_global(cx)
                .resize_all_panels_in_dock
                .contains(&DockPosition::Right)
            {
                right_dock.resize_all_panels(Some(size), flex_grow, window, cx);
            } else {
                right_dock.resize_active_panel(Some(size), flex_grow, window, cx);
            }
        });
    }
}

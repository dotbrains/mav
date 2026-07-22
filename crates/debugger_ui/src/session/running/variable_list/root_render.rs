use super::*;

impl Render for VariableList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .track_focus(&self.focus_handle)
            .key_context("VariableList")
            .id("variable-list")
            .group("variable-list")
            .size_full()
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::expand_selected_entry))
            .on_action(cx.listener(Self::collapse_selected_entry))
            .on_action(cx.listener(Self::copy_variable_name))
            .on_action(cx.listener(Self::copy_variable_value))
            .on_action(cx.listener(Self::edit_variable))
            .on_action(cx.listener(Self::add_watcher))
            .on_action(cx.listener(Self::remove_watcher))
            .on_action(cx.listener(Self::toggle_data_breakpoint))
            .on_action(cx.listener(Self::jump_to_variable_memory))
            .child(
                uniform_list(
                    "variable-list",
                    self.entries.len(),
                    cx.processor(move |this, range: Range<usize>, window, cx| {
                        this.render_entries(range, window, cx)
                    }),
                )
                .track_scroll(&self.list_handle)
                .with_width_from_item(self.max_width_index)
                .with_sizing_behavior(gpui::ListSizingBehavior::Auto)
                .with_horizontal_sizing_behavior(gpui::ListHorizontalSizingBehavior::Unconstrained)
                .gap_1_5()
                .size_full()
                .flex_grow_1(),
            )
            .children(self.open_context_menu.as_ref().map(|(menu, position, _)| {
                deferred(
                    anchored()
                        .position(*position)
                        .anchor(gpui::Anchor::TopLeft)
                        .child(menu.clone()),
                )
                .with_priority(1)
            }))
            // .vertical_scrollbar_for(&self.list_handle, window, cx)
            .custom_scrollbars(
                ui::Scrollbars::new(ScrollAxes::Both)
                    .tracked_scroll_handle(&self.list_handle)
                    .with_track_along(ScrollAxes::Both, cx.theme().colors().panel_background)
                    .tracked_entity(cx.entity_id()),
                window,
                cx,
            )
    }
}

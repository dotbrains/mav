use super::*;

impl Render for MemoryView {
    fn render(
        &mut self,
        window: &mut ui::Window,
        cx: &mut ui::Context<Self>,
    ) -> impl ui::IntoElement {
        let (icon, tooltip_text) = if self.is_writing_memory {
            (IconName::Pencil, "Edit memory at a selected address")
        } else {
            (
                IconName::LocationEdit,
                "Change address of currently viewed memory",
            )
        };
        v_flex()
            .id("Memory-view")
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::go_to_address))
            .p_1()
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::toggle_data_breakpoint))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::page_up))
            .size_full()
            .track_focus(&self.focus_handle)
            .child(
                h_flex()
                    .w_full()
                    .mb_0p5()
                    .gap_1()
                    .child(
                        h_flex()
                            .w_full()
                            .rounded_md()
                            .border_1()
                            .gap_x_2()
                            .px_2()
                            .py_0p5()
                            .mb_0p5()
                            .bg(cx.theme().colors().editor_background)
                            .when_else(
                                self.query_editor
                                    .focus_handle(cx)
                                    .contains_focused(window, cx),
                                |this| this.border_color(cx.theme().colors().border_focused),
                                |this| this.border_color(cx.theme().colors().border_transparent),
                            )
                            .child(
                                div()
                                    .id("memory-view-editor-icon")
                                    .child(Icon::new(icon).size(ui::IconSize::XSmall))
                                    .tooltip(Tooltip::text(tooltip_text)),
                            )
                            .child(self.render_query_bar(cx)),
                    )
                    .child(self.render_width_picker(window, cx)),
            )
            .child(Divider::horizontal())
            .child(
                v_flex()
                    .size_full()
                    .on_drag_move(cx.listener(|this, evt, _, _| {
                        this.handle_memory_drag(evt);
                    }))
                    .child(self.render_memory(cx).size_full())
                    .children(self.open_context_menu.as_ref().map(|(menu, position, _)| {
                        deferred(
                            anchored()
                                .position(*position)
                                .anchor(gpui::Anchor::TopLeft)
                                .child(menu.clone()),
                        )
                        .with_priority(1)
                    }))
                    .custom_scrollbars(
                        ui::Scrollbars::new(ui::ScrollAxes::Both)
                            .tracked_scroll_handle(&self.view_state_handle)
                            .with_track_along(
                                ui::ScrollAxes::Both,
                                cx.theme().colors().panel_background,
                            )
                            .tracked_entity(cx.entity_id()),
                        window,
                        cx,
                    ),
            )
    }
}

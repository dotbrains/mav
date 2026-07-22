use super::*;

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = theme_settings::setup_ui_font(window, cx);

        client_side_decorations(
            v_flex()
                .text_color(cx.theme().colors().text)
                .size_full()
                .children(self.title_bar.clone())
                .child(
                    div()
                        .id("settings-window")
                        .key_context("SettingsWindow")
                        .track_focus(&self.focus_handle)
                        .on_action(cx.listener(|this, _: &OpenCurrentFile, window, cx| {
                            this.open_current_settings_file(window, cx);
                        }))
                        .on_action(|_: &Minimize, window, _cx| {
                            window.minimize_window();
                        })
                        .on_action(cx.listener(|this, _: &search::FocusSearch, window, cx| {
                            this.search_bar.focus_handle(cx).focus(window, cx);
                        }))
                        .on_action(cx.listener(|this, _: &ToggleFocusNav, window, cx| {
                            if this
                                .navbar_focus_handle
                                .focus_handle(cx)
                                .contains_focused(window, cx)
                            {
                                this.open_and_scroll_to_navbar_entry(
                                    this.navbar_entry,
                                    None,
                                    true,
                                    window,
                                    cx,
                                );
                            } else {
                                this.focus_and_scroll_to_nav_entry(this.navbar_entry, window, cx);
                            }
                        }))
                        .on_action(cx.listener(
                            |this, FocusFile(file_index): &FocusFile, window, cx| {
                                this.focus_file_at_index(*file_index as usize, window, cx);
                            },
                        ))
                        .on_action(cx.listener(|this, _: &FocusNextFile, window, cx| {
                            let next_index = usize::min(
                                this.focused_file_index(window, cx) + 1,
                                this.files.len().saturating_sub(1),
                            );
                            this.focus_file_at_index(next_index, window, cx);
                        }))
                        .on_action(cx.listener(|this, _: &FocusPreviousFile, window, cx| {
                            let prev_index = this.focused_file_index(window, cx).saturating_sub(1);
                            this.focus_file_at_index(prev_index, window, cx);
                        }))
                        .on_action(cx.listener(|this, _: &menu::SelectNext, window, cx| {
                            if this
                                .search_bar
                                .focus_handle(cx)
                                .contains_focused(window, cx)
                            {
                                this.focus_and_scroll_to_first_visible_nav_entry(window, cx);
                            } else {
                                window.focus_next(cx);
                            }
                        }))
                        .on_action(|_: &menu::SelectPrevious, window, cx| {
                            window.focus_prev(cx);
                        })
                        .flex()
                        .flex_row()
                        .flex_1()
                        .min_h_0()
                        .font(ui_font)
                        .bg(cx.theme().colors().background)
                        .text_color(cx.theme().colors().text)
                        .when(!cfg!(target_os = "macos"), |this| {
                            this.border_t_1().border_color(cx.theme().colors().border)
                        })
                        .child(self.render_nav(window, cx))
                        .child(self.render_page(window, cx)),
                ),
            window,
            cx,
            Tiling::default(),
        )
    }
}

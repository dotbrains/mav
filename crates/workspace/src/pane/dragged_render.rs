use super::*;

impl Render for DraggedTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = ThemeSettings::get_global(cx).ui_font.clone();
        let label = self.item.tab_content(
            TabContentParams {
                detail: Some(self.detail),
                selected: false,
                preview: false,
                deemphasized: false,
                max_title_len: None,
                truncate_title_middle: false,
            },
            window,
            cx,
        );
        Tab::new("")
            .toggle_state(self.is_active)
            .child(label)
            .render(window, cx)
            .font(ui_font)
    }
}

impl Render for DraggedPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .h(Tab::container_height(cx))
            .flex()
            .items_center()
            .border_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().tab_bar_background)
            .child(Label::new("Pane").color(Color::Muted))
    }
}

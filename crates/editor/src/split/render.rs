use super::*;

impl EventEmitter<EditorEvent> for SplittableEditor {}
impl EventEmitter<SearchEvent> for SplittableEditor {}
impl Focusable for SplittableEditor {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.focused_editor().read(cx).focus_handle(cx)
    }
}

impl Render for SplittableEditor {
    fn render(
        &mut self,
        _window: &mut ui::Window,
        cx: &mut ui::Context<Self>,
    ) -> impl ui::IntoElement {
        let is_split = self.lhs.is_some();
        let inner = if is_split {
            let style = self.rhs_editor.read(cx).create_style(cx);
            SplitEditorView::new(cx.entity(), style, self.split_state.clone()).into_any_element()
        } else {
            self.rhs_editor.clone().into_any_element()
        };

        let this = cx.entity().downgrade();
        let last_width = self.last_width;

        div()
            .id("splittable-editor")
            .on_action(cx.listener(Self::toggle_split))
            .on_action(cx.listener(Self::activate_pane_left))
            .on_action(cx.listener(Self::activate_pane_right))
            .on_action(cx.listener(Self::intercept_toggle_breakpoint))
            .on_action(cx.listener(Self::intercept_enable_breakpoint))
            .on_action(cx.listener(Self::intercept_disable_breakpoint))
            .on_action(cx.listener(Self::intercept_edit_log_breakpoint))
            .on_action(cx.listener(Self::intercept_inline_assist))
            .capture_action(cx.listener(Self::toggle_soft_wrap))
            .size_full()
            .child(inner)
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let width = bounds.size.width;
                        if last_width == Some(width) {
                            return;
                        }
                        window.defer(cx, move |window, cx| {
                            this.update(cx, |this, cx| {
                                this.width_changed(width, window, cx);
                            })
                            .ok();
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }
}

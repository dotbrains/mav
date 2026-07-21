use super::*;

impl EditorElement {
    pub(super) fn request_layout_impl(
        &mut self,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, EditorRequestLayoutState) {
        let rem_size = self.rem_size(cx);
        window.with_rem_size(rem_size, |window| {
            self.editor.update(cx, |editor, cx| {
                editor.set_style(self.style.clone(), window, cx);

                let layout_id = match editor.mode {
                    EditorMode::SingleLine => {
                        let rem_size = window.rem_size();
                        let height = self.style.text.line_height_in_pixels(rem_size);
                        let mut style = Style::default();
                        style.size.height = height.into();
                        style.size.width = relative(1.).into();
                        window.request_layout(style, None, cx)
                    }
                    EditorMode::AutoHeight {
                        min_lines,
                        max_lines,
                    } => {
                        let editor_handle = cx.entity();
                        window.request_measured_layout(
                            Style::default(),
                            move |known_dimensions, available_space, window, cx| {
                                editor_handle
                                    .update(cx, |editor, cx| {
                                        compute_auto_height_layout(
                                            editor,
                                            min_lines,
                                            max_lines,
                                            known_dimensions,
                                            available_space.width,
                                            window,
                                            cx,
                                        )
                                    })
                                    .unwrap_or_default()
                            },
                        )
                    }
                    EditorMode::Minimap { .. } => {
                        let mut style = Style::default();
                        style.size.width = relative(1.).into();
                        style.size.height = relative(1.).into();
                        window.request_layout(style, None, cx)
                    }
                    EditorMode::Full {
                        sizing_behavior, ..
                    } => {
                        let mut style = Style::default();
                        style.size.width = relative(1.).into();
                        if sizing_behavior == SizingBehavior::SizeByContent {
                            let snapshot = editor.snapshot(window, cx);
                            let line_height =
                                self.style.text.line_height_in_pixels(window.rem_size());
                            let scroll_height =
                                (snapshot.max_point().row().next_row().0 as f32) * line_height;
                            style.size.height = scroll_height.into();
                        } else {
                            style.size.height = relative(1.).into();
                        }
                        window.request_layout(style, None, cx)
                    }
                };

                (layout_id, EditorRequestLayoutState::default())
            })
        })
    }
}

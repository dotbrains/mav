use super::*;

impl Editor {
    pub(super) fn initial_display_map(
        multi_buffer: &Entity<MultiBuffer>,
        display_map: Option<Entity<DisplayMap>>,
        diagnostics_max_severity: DiagnosticSeverity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<DisplayMap> {
        display_map.unwrap_or_else(|| {
            let style = window.text_style();
            let font_size = style.font_size.to_pixels(window.rem_size());
            let editor = cx.entity().downgrade();
            let fold_placeholder = FoldPlaceholder {
                constrain_width: false,
                render: Arc::new(move |fold_id, fold_range, cx| {
                    let editor = editor.clone();
                    FoldPlaceholder::fold_element(fold_id, cx)
                        .cursor_pointer()
                        .child("⋯")
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(move |_, _window, cx| {
                            editor
                                .update(cx, |editor, cx| {
                                    editor.unfold_ranges(
                                        &[fold_range.start..fold_range.end],
                                        true,
                                        false,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                })
                                .ok();
                        })
                        .into_any()
                }),
                merge_adjacent: true,
                ..FoldPlaceholder::default()
            };
            cx.new(|cx| {
                DisplayMap::new(
                    multi_buffer.clone(),
                    style.font(),
                    font_size,
                    None,
                    FILE_HEADER_HEIGHT,
                    MULTI_BUFFER_EXCERPT_HEADER_HEIGHT,
                    fold_placeholder,
                    diagnostics_max_severity,
                    cx,
                )
            })
        })
    }
}

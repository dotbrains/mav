use super::*;

pub fn render_breadcrumb_text(
    mut segments: Vec<HighlightedText>,
    breadcrumb_font: Option<Font>,
    prefix: Option<gpui::AnyElement>,
    active_item: &dyn ItemHandle,
    multibuffer_header: bool,
    window: &mut Window,
    cx: &App,
) -> gpui::AnyElement {
    const MAX_SEGMENTS: usize = 12;

    let element = h_flex().flex_grow_1().text_ui(cx);

    let prefix_end_ix = cmp::min(segments.len(), MAX_SEGMENTS / 2);
    let suffix_start_ix = cmp::max(
        prefix_end_ix,
        segments.len().saturating_sub(MAX_SEGMENTS / 2),
    );

    if suffix_start_ix > prefix_end_ix {
        segments.splice(
            prefix_end_ix..suffix_start_ix,
            Some(HighlightedText {
                text: "⋯".into(),
                highlights: vec![],
            }),
        );
    }

    let highlighted_segments = segments.into_iter().enumerate().map(|(index, segment)| {
        let mut text_style = window.text_style();
        if let Some(font) = &breadcrumb_font {
            text_style.font_family = font.family.clone();
            text_style.font_features = font.features.clone();
            text_style.font_style = font.style;
            text_style.font_weight = font.weight;
        }
        text_style.color = Color::Muted.color(cx);

        if index == 0
            && !workspace::TabBarSettings::get_global(cx).show
            && active_item.is_dirty(cx)
            && let Some(styled_element) = apply_dirty_filename_style(&segment, &text_style, cx)
        {
            return styled_element;
        }

        StyledText::new(segment.text.replace('\n', " "))
            .with_default_highlights(&text_style, segment.highlights)
            .into_any()
    });

    let breadcrumbs = Itertools::intersperse_with(highlighted_segments, || {
        Label::new("›").color(Color::Placeholder).into_any_element()
    });

    let breadcrumbs_stack = h_flex()
        .gap_1()
        .when(multibuffer_header, |this| {
            this.pl_2()
                .border_l_1()
                .border_color(cx.theme().colors().border.opacity(0.6))
        })
        .children(breadcrumbs);

    let breadcrumbs = if let Some(prefix) = prefix {
        h_flex().gap_1p5().child(prefix).child(breadcrumbs_stack)
    } else {
        breadcrumbs_stack
    };

    let editor = active_item
        .downcast::<Editor>()
        .map(|editor| editor.downgrade());

    let has_project_path = active_item.project_path(cx).is_some();

    match editor {
        Some(editor) => element
            .id("breadcrumb_container")
            .when(!multibuffer_header, |this| this.overflow_x_scroll())
            .child(
                ButtonLike::new("toggle outline view")
                    .child(breadcrumbs)
                    .when(multibuffer_header, |this| {
                        this.style(ButtonStyle::Transparent)
                    })
                    .when(!multibuffer_header, |this| {
                        let focus_handle = editor.upgrade().unwrap().focus_handle(&cx);

                        this.tooltip(Tooltip::element(move |_window, cx| {
                            v_flex()
                                .gap_1()
                                .child(
                                    h_flex()
                                        .gap_1()
                                        .justify_between()
                                        .child(Label::new("Show Symbol Outline"))
                                        .child(ui::KeyBinding::for_action_in(
                                            &mav_actions::outline::ToggleOutline,
                                            &focus_handle,
                                            cx,
                                        )),
                                )
                                .when(has_project_path, |this| {
                                    this.child(
                                        h_flex()
                                            .gap_1()
                                            .justify_between()
                                            .pt_1()
                                            .border_t_1()
                                            .border_color(cx.theme().colors().border_variant)
                                            .child(Label::new("Right-Click to Copy Path")),
                                    )
                                })
                                .into_any_element()
                        }))
                        .on_click({
                            let editor = editor.clone();
                            move |_, window, cx| {
                                if let Some((editor, callback)) = editor
                                    .upgrade()
                                    .zip(mav_actions::outline::TOGGLE_OUTLINE.get())
                                {
                                    callback(editor.to_any_view(), window, cx);
                                }
                            }
                        })
                        .when(has_project_path, |this| {
                            this.on_right_click({
                                let editor = editor.clone();
                                move |_, _, cx| {
                                    if let Some(abs_path) = editor.upgrade().and_then(|editor| {
                                        editor.update(cx, |editor, cx| {
                                            editor.target_file_abs_path(cx)
                                        })
                                    }) {
                                        if let Some(path_str) = abs_path.to_str() {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                path_str.to_string(),
                                            ));
                                        }
                                    }
                                }
                            })
                        })
                    }),
            )
            .into_any_element(),
        None => element
            .h(rems_from_px(22.))
            .pl_1()
            .child(breadcrumbs)
            .into_any_element(),
    }
}

fn apply_dirty_filename_style(
    segment: &HighlightedText,
    text_style: &gpui::TextStyle,
    cx: &App,
) -> Option<gpui::AnyElement> {
    let text = segment.text.replace('\n', " ");

    let filename_position = std::path::Path::new(segment.text.as_ref())
        .file_name()
        .and_then(|f| {
            let filename_str = f.to_string_lossy();
            segment.text.rfind(filename_str.as_ref())
        })?;

    let bold_weight = FontWeight::BOLD;
    let default_color = Color::Default.color(cx);

    if filename_position == 0 {
        let mut filename_style = text_style.clone();
        filename_style.font_weight = bold_weight;
        filename_style.color = default_color;

        return Some(
            StyledText::new(text)
                .with_default_highlights(&filename_style, [])
                .into_any(),
        );
    }

    let highlight_style = gpui::HighlightStyle {
        font_weight: Some(bold_weight),
        color: Some(default_color),
        ..Default::default()
    };

    let highlight = vec![(filename_position..text.len(), highlight_style)];
    Some(
        StyledText::new(text)
            .with_default_highlights(text_style, highlight)
            .into_any(),
    )
}

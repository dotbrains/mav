use super::*;

static EMPTY_LENS_FALLBACK_TITLE: SharedString = SharedString::new_static("0 references");
pub(crate) const CODE_LENS_SEPARATOR: &str = " | ";

pub(crate) fn rendered_text_matches(a: &CodeLensLine, b: &CodeLensLine) -> bool {
    a.indent_column == b.indent_column
        && a.items.len() == b.items.len()
        && a.items
            .iter()
            .zip(&b.items)
            .all(|(x, y)| displayed_title(x) == displayed_title(y))
}

/// Text rendered for a code lens item, or `None` if it should not render
/// (placeholder while resolve is in flight).
pub(crate) fn displayed_title(item: &CodeLensItem) -> Option<&SharedString> {
    item.title
        .as_ref()
        .or_else(|| item.action.resolved.then_some(&EMPTY_LENS_FALLBACK_TITLE))
}

pub(crate) fn group_lenses_by_row(
    lenses: Vec<(Anchor, CodeLensItem)>,
    snapshot: &MultiBufferSnapshot,
) -> impl Iterator<Item = CodeLensLine> {
    lenses
        .into_iter()
        .into_group_map_by(|(position, _)| {
            let row = position.to_point(snapshot).row;
            MultiBufferRow(row)
        })
        .into_iter()
        .sorted_by_key(|(row, _)| *row)
        .filter_map(|(row, entries)| {
            let position = entries.first()?.0;
            let items = entries.into_iter().map(|(_, item)| item).collect();
            let indent_column = snapshot.indent_size_for_line(row).len;
            Some(CodeLensLine {
                position,
                indent_column,
                items,
            })
        })
}

pub(crate) fn build_code_lens_renderer(
    line: CodeLensLine,
    editor: WeakEntity<Editor>,
) -> RenderBlock {
    Arc::new(move |cx| {
        let resolved_items = line
            .items
            .iter()
            .filter_map(|item| {
                let title = displayed_title(item)?;
                let action = item.title.is_some().then(|| item.action.clone());
                Some((title, action))
            })
            .collect::<Vec<_>>();
        let mut children = Vec::with_capacity((2 * resolved_items.len()).saturating_sub(1));
        let text_style = &cx.editor_style.text;
        let font = text_style.font();
        let font_size = text_style.font_size.to_pixels(cx.window.rem_size()) * 0.9;

        for (i, (title, action)) in resolved_items.into_iter().enumerate() {
            if i > 0 {
                children.push(
                    div()
                        .font(font.clone())
                        .text_size(font_size)
                        .text_color(cx.app.theme().colors().text_muted)
                        .child(CODE_LENS_SEPARATOR)
                        .into_any_element(),
                );
            }

            children.push(
                div()
                    .id(ElementId::from(i))
                    .font(font.clone())
                    .text_size(font_size)
                    .text_color(cx.app.theme().colors().text_muted)
                    .child(title.clone())
                    .when_some(action, |code_lens_div, action| {
                        let position = line.position;
                        let editor_handle = editor.clone();

                        code_lens_div
                            .cursor_pointer()
                            .hover(|style| style.text_color(cx.app.theme().colors().text))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click({
                                move |_event, window, cx| {
                                    if let Some(editor) = editor_handle.upgrade() {
                                        editor.update(cx, |editor, cx| {
                                            editor.change_selections(
                                                SelectionEffects::default(),
                                                window,
                                                cx,
                                                |s| {
                                                    s.select_anchor_ranges([position..position]);
                                                },
                                            );

                                            let action = action.clone();
                                            if let Some(workspace) = editor.workspace() {
                                                if try_handle_client_command(
                                                    &action, editor, &workspace, window, cx,
                                                ) {
                                                    return;
                                                }

                                                let project = workspace.read(cx).project().clone();
                                                if let Some(buffer) = editor
                                                    .buffer()
                                                    .read(cx)
                                                    .buffer(action.range.start.buffer_id)
                                                {
                                                    project
                                                        .update(cx, |project, cx| {
                                                            project.apply_code_action(
                                                                buffer, action, true, cx,
                                                            )
                                                        })
                                                        .detach_and_log_err(cx);
                                                }
                                            }
                                        });
                                    }
                                }
                            })
                    })
                    .into_any_element(),
            );
        }

        div()
            .id(cx.block_id)
            .pl(cx.em_width * (line.indent_column as f32 + 0.5))
            .h_full()
            .flex()
            .flex_row()
            .items_end()
            .children(children)
            .into_any_element()
    })
}

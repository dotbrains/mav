use super::*;

impl Editor {
    /// Opens a multibuffer with the given project locations in it.
    pub(super) fn open_locations_in_multibuffer(
        workspace: &mut Workspace,
        locations: std::collections::HashMap<Entity<Buffer>, Vec<Range<Point>>>,
        title: String,
        split: bool,
        allow_preview: bool,
        multibuffer_selection_mode: MultibufferSelectionMode,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Option<(Entity<Editor>, Entity<Pane>)> {
        if locations.is_empty() {
            log::error!("bug: open_locations_in_multibuffer called with empty list of locations");
            return None;
        }

        let capability = workspace.project().read(cx).capability();
        let mut ranges = <Vec<Range<Anchor>>>::new();

        // a key to find existing multibuffer editors with the same set of locations
        // to prevent us from opening more and more multibuffer tabs for searches and the like
        let mut key = (title.clone(), vec![]);
        let excerpt_buffer = cx.new(|cx| {
            let key = &mut key.1;
            let mut multibuffer = MultiBuffer::new(capability);
            for (buffer, mut ranges_for_buffer) in locations {
                ranges_for_buffer.sort_by_key(|range| (range.start, Reverse(range.end)));
                key.push((buffer.read(cx).remote_id(), ranges_for_buffer.clone()));
                multibuffer.set_excerpts_for_path(
                    PathKey::for_buffer(&buffer, cx),
                    buffer.clone(),
                    ranges_for_buffer.clone(),
                    multibuffer_context_lines(cx),
                    cx,
                );
                let snapshot = multibuffer.snapshot(cx);
                let buffer_snapshot = buffer.read(cx).snapshot();
                ranges.extend(ranges_for_buffer.into_iter().filter_map(|range| {
                    let text_range = buffer_snapshot.anchor_range_inside(range);
                    let start = snapshot.anchor_in_buffer(text_range.start)?;
                    let end = snapshot.anchor_in_buffer(text_range.end)?;
                    Some(start..end)
                }))
            }

            let final_snapshot = multibuffer.snapshot(cx);
            ranges.sort_by(|a, b| a.start.cmp(&b.start, &final_snapshot));

            multibuffer.with_title(title)
        });
        let existing = workspace.active_pane().update(cx, |pane, cx| {
            pane.items()
                .filter_map(|item| item.downcast::<Editor>())
                .find(|editor| {
                    editor
                        .read(cx)
                        .lookup_key
                        .as_ref()
                        .and_then(|it| {
                            it.downcast_ref::<(String, Vec<(BufferId, Vec<Range<Point>>)>)>()
                        })
                        .is_some_and(|it| *it == key)
                })
        });
        let was_existing = existing.is_some();
        let editor = existing.unwrap_or_else(|| {
            cx.new(|cx| {
                let mut editor = Editor::for_multibuffer(
                    excerpt_buffer,
                    Some(workspace.project().clone()),
                    window,
                    cx,
                );
                editor.lookup_key = Some(Box::new(key));
                editor
            })
        });
        editor.update(cx, |editor, cx| match multibuffer_selection_mode {
            MultibufferSelectionMode::First => {
                if let Some(first_range) = ranges.first() {
                    editor.change_selections(
                        SelectionEffects::no_scroll(),
                        window,
                        cx,
                        |selections| {
                            selections.clear_disjoint();
                            selections.select_anchor_ranges(std::iter::once(first_range.clone()));
                        },
                    );
                }
                editor.highlight_background(
                    HighlightKey::Editor,
                    &ranges,
                    |_, theme| theme.colors().editor_highlighted_line_background,
                    cx,
                );
            }
            MultibufferSelectionMode::All => {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    selections.clear_disjoint();
                    selections.select_anchor_ranges(ranges);
                });
            }
        });

        let item = Box::new(editor.clone());

        let pane = if split {
            workspace.adjacent_pane(window, cx)
        } else {
            workspace.active_pane().clone()
        };
        let activate_pane = split;

        let mut destination_index = None;
        pane.update(cx, |pane, cx| {
            if allow_preview && !was_existing {
                destination_index = pane.replace_preview_item_id(item.item_id(), window, cx);
            }
            if was_existing && !allow_preview {
                pane.unpreview_item_if_preview(item.item_id());
            }
            pane.add_item(item, activate_pane, true, destination_index, window, cx);
        });

        Some((editor, pane))
    }
}

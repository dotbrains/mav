use super::*;

impl Editor {
    fn navigation_data(&self, cursor_anchor: Anchor, cx: &mut Context<Self>) -> NavigationData {
        let display_snapshot = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let buffer = self.buffer.read(cx).read(cx);
        let cursor_position = cursor_anchor.to_point(&buffer);
        let scroll_anchor = self.scroll_manager.native_anchor(&display_snapshot, cx);
        let scroll_top_row = scroll_anchor.top_row(&buffer);
        drop(buffer);

        NavigationData {
            cursor_anchor,
            cursor_position,
            scroll_anchor,
            scroll_top_row,
        }
    }

    fn expand_excerpts_for_direction(
        &mut self,
        lines: u32,
        direction: ExpandExcerptDirection,
        cx: &mut Context<Self>,
    ) {
        let selections = self.selections.disjoint_anchors_arc();

        let lines = if lines == 0 {
            EditorSettings::get_global(cx).expand_excerpt_lines
        } else {
            lines
        };

        let snapshot = self.buffer.read(cx).snapshot(cx);
        let excerpt_anchors = selections
            .iter()
            .flat_map(|selection| {
                snapshot
                    .range_to_buffer_ranges(selection.range())
                    .into_iter()
                    .filter_map(|(buffer_snapshot, range, _)| {
                        snapshot.anchor_in_excerpt(buffer_snapshot.anchor_after(range.start))
                    })
            })
            .collect::<Vec<_>>();

        if self.delegate_expand_excerpts {
            cx.emit(EditorEvent::ExpandExcerptsRequested {
                excerpt_anchors,
                lines,
                direction,
            });
            return;
        }

        self.buffer.update(cx, |buffer, cx| {
            buffer.expand_excerpts(excerpt_anchors, lines, direction, cx)
        })
    }

    fn go_to_definition_of_kind(
        &mut self,
        kind: GotoDefinitionKind,
        split: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<Navigated>> {
        let Some(provider) = self.semantics_provider.clone() else {
            return Task::ready(Ok(Navigated::No));
        };
        let head = self
            .selections
            .newest::<MultiBufferOffset>(&self.display_snapshot(cx))
            .head();
        let buffer = self.buffer.read(cx);
        let Some((buffer, head)) = buffer.text_anchor_for_position(head, cx) else {
            return Task::ready(Ok(Navigated::No));
        };
        let Some(definitions) = provider.definitions(&buffer, head, kind, cx) else {
            return Task::ready(Ok(Navigated::No));
        };

        let nav_entry = self.navigation_entry(self.selections.newest_anchor().head(), cx);

        cx.spawn_in(window, async move |editor, cx| {
            let Some(definitions) = definitions.await? else {
                return Ok(Navigated::No);
            };
            let navigated = editor
                .update_in(cx, |editor, window, cx| {
                    editor.navigate_to_hover_links(
                        Some(kind),
                        definitions
                            .into_iter()
                            .filter(|location| {
                                crate::hover_links::exclude_link_to_position(
                                    &buffer, &head, location, cx,
                                )
                            })
                            .map(HoverLink::Text)
                            .collect::<Vec<_>>(),
                        nav_entry,
                        split,
                        window,
                        cx,
                    )
                })?
                .await?;
            anyhow::Ok(navigated)
        })
    }

    fn compute_target_location(
        &self,
        lsp_location: lsp::Location,
        server_id: LanguageServerId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Option<Location>>> {
        let Some(project) = self.project.clone() else {
            return Task::ready(Ok(None));
        };

        cx.spawn_in(window, async move |editor, cx| {
            let location_task = editor.update(cx, |_, cx| {
                project.update(cx, |project, cx| {
                    project.open_local_buffer_via_lsp(lsp_location.uri.clone(), server_id, cx)
                })
            })?;
            let location = Some({
                let target_buffer_handle = location_task.await.context("open local buffer")?;
                let range = target_buffer_handle.read_with(cx, |target_buffer, _| {
                    let target_start = target_buffer
                        .clip_point_utf16(point_from_lsp(lsp_location.range.start), Bias::Left);
                    let target_end = target_buffer
                        .clip_point_utf16(point_from_lsp(lsp_location.range.end), Bias::Left);
                    target_buffer.anchor_after(target_start)
                        ..target_buffer.anchor_before(target_end)
                });
                Location {
                    buffer: target_buffer_handle,
                    range,
                }
            });
            Ok(location)
        })
    }

    fn go_to_singleton_buffer_range_impl(
        &mut self,
        range: Range<Point>,
        record_nav_history: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let multibuffer = self.buffer().read(cx);
        if !multibuffer.is_singleton() {
            return;
        };
        let anchor_range = range.to_anchors(&multibuffer.snapshot(cx));
        self.change_selections(
            SelectionEffects::scroll(Autoscroll::for_go_to_definition(
                self.cursor_top_offset(cx),
                cx,
            ))
            .nav_history(record_nav_history),
            window,
            cx,
            |s| s.select_anchor_ranges([anchor_range]),
        );
    }

    fn go_to_document_highlight_before_or_after_position(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let snapshot = self.snapshot(window, cx);
        let buffer = &snapshot.buffer_snapshot();
        let position = self
            .selections
            .newest::<Point>(&snapshot.display_snapshot)
            .head();
        let anchor_position = buffer.anchor_after(position);

        // Get all document highlights (both read and write)
        let mut all_highlights = Vec::new();

        if let Some((_, read_highlights)) = self
            .background_highlights
            .get(&HighlightKey::DocumentHighlightRead)
        {
            all_highlights.extend(read_highlights.iter());
        }

        if let Some((_, write_highlights)) = self
            .background_highlights
            .get(&HighlightKey::DocumentHighlightWrite)
        {
            all_highlights.extend(write_highlights.iter());
        }

        if all_highlights.is_empty() {
            return;
        }

        // Sort highlights by position
        all_highlights.sort_by(|a, b| a.start.cmp(&b.start, buffer));

        let target_highlight = match direction {
            Direction::Next => {
                // Find the first highlight after the current position
                all_highlights
                    .iter()
                    .find(|highlight| highlight.start.cmp(&anchor_position, buffer).is_gt())
            }
            Direction::Prev => {
                // Find the last highlight before the current position
                all_highlights
                    .iter()
                    .rev()
                    .find(|highlight| highlight.end.cmp(&anchor_position, buffer).is_lt())
            }
        };

        if let Some(highlight) = target_highlight {
            let destination = highlight.start.to_point(buffer);
            let autoscroll = Autoscroll::center();

            self.unfold_ranges(&[destination..destination], false, false, cx);
            self.change_selections(SelectionEffects::scroll(autoscroll), window, cx, |s| {
                s.select_ranges([destination..destination]);
            });
        }
    }
}

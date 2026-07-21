use super::*;

impl EditorElement {
    pub(super) fn collect_fast_scrollbar_markers(
        &self,
        layout: &EditorLayout,
        scrollbar_layout: &ScrollbarLayout,
        cx: &mut App,
    ) -> Vec<PaintQuad> {
        const LIMIT: usize = 100;
        if !EditorSettings::get_global(cx).scrollbar.cursors || layout.cursors.len() > LIMIT {
            return vec![];
        }
        let cursor_ranges = layout
            .cursors
            .iter()
            .map(|(point, color)| ColoredRange {
                start: point.row(),
                end: point.row(),
                color: *color,
            })
            .collect_vec();
        scrollbar_layout.marker_quads_for_ranges(cursor_ranges, None)
    }

    pub(super) fn refresh_slow_scrollbar_markers(
        &self,
        layout: &EditorLayout,
        scrollbar_layout: &ScrollbarLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.editor.update(cx, |editor, cx| {
            if editor.buffer_kind(cx) != ItemBufferKind::Singleton
                || !editor
                    .scrollbar_marker_state
                    .should_refresh(scrollbar_layout.hitbox.size)
            {
                return;
            }

            let scrollbar_layout = scrollbar_layout.clone();
            let background_highlights = editor.background_highlights.clone();
            let snapshot = layout.position_map.snapshot.clone();
            let theme = cx.theme().clone();
            let scrollbar_settings = EditorSettings::get_global(cx).scrollbar;

            editor.scrollbar_marker_state.dirty = false;
            editor.scrollbar_marker_state.pending_refresh =
                Some(cx.spawn_in(window, async move |editor, cx| {
                    let scrollbar_size = scrollbar_layout.hitbox.size;
                    let scrollbar_markers = cx
                        .background_spawn(async move {
                            let max_point = snapshot.display_snapshot.buffer_snapshot().max_point();
                            let mut marker_quads = Vec::new();
                            if scrollbar_settings.git_diff {
                                let marker_row_ranges =
                                    snapshot.buffer_snapshot().diff_hunks().map(|hunk| {
                                        let start_display_row =
                                            MultiBufferPoint::new(hunk.row_range.start.0, 0)
                                                .to_display_point(&snapshot.display_snapshot)
                                                .row();
                                        let mut end_display_row =
                                            MultiBufferPoint::new(hunk.row_range.end.0, 0)
                                                .to_display_point(&snapshot.display_snapshot)
                                                .row();
                                        if end_display_row != start_display_row {
                                            end_display_row.0 -= 1;
                                        }
                                        let color = match &hunk.status().kind {
                                            DiffHunkStatusKind::Added => {
                                                theme.colors().version_control_added
                                            }
                                            DiffHunkStatusKind::Modified => {
                                                theme.colors().version_control_modified
                                            }
                                            DiffHunkStatusKind::Deleted => {
                                                theme.colors().version_control_deleted
                                            }
                                        };
                                        ColoredRange {
                                            start: start_display_row,
                                            end: end_display_row,
                                            color,
                                        }
                                    });

                                marker_quads.extend(
                                    scrollbar_layout
                                        .marker_quads_for_ranges(marker_row_ranges, Some(0)),
                                );
                            }

                            for (background_highlight_id, (_, background_ranges)) in
                                background_highlights.iter()
                            {
                                let is_search_highlights = *background_highlight_id
                                    == HighlightKey::BufferSearchHighlights;
                                let is_text_highlights =
                                    *background_highlight_id == HighlightKey::SelectedTextHighlight;
                                let is_symbol_occurrences = *background_highlight_id
                                    == HighlightKey::DocumentHighlightRead
                                    || *background_highlight_id
                                        == HighlightKey::DocumentHighlightWrite;
                                if (is_search_highlights && scrollbar_settings.search_results)
                                    || (is_text_highlights && scrollbar_settings.selected_text)
                                    || (is_symbol_occurrences && scrollbar_settings.selected_symbol)
                                {
                                    let mut color = theme.status().info;
                                    if is_symbol_occurrences {
                                        color.fade_out(0.5);
                                    }
                                    let marker_row_ranges = background_ranges.iter().map(|range| {
                                        let display_start = range
                                            .start
                                            .to_display_point(&snapshot.display_snapshot);
                                        let display_end =
                                            range.end.to_display_point(&snapshot.display_snapshot);
                                        ColoredRange {
                                            start: display_start.row(),
                                            end: display_end.row(),
                                            color,
                                        }
                                    });
                                    marker_quads.extend(
                                        scrollbar_layout
                                            .marker_quads_for_ranges(marker_row_ranges, Some(1)),
                                    );
                                }
                            }

                            if scrollbar_settings.diagnostics != ScrollbarDiagnostics::None {
                                let diagnostics = snapshot
                                    .buffer_snapshot()
                                    .diagnostics_in_range::<Point>(Point::zero()..max_point)
                                    // Don't show diagnostics the user doesn't care about
                                    .filter(|diagnostic| {
                                        match (
                                            scrollbar_settings.diagnostics,
                                            diagnostic.diagnostic.severity,
                                        ) {
                                            (ScrollbarDiagnostics::All, _) => true,
                                            (
                                                ScrollbarDiagnostics::Error,
                                                lsp::DiagnosticSeverity::ERROR,
                                            ) => true,
                                            (
                                                ScrollbarDiagnostics::Warning,
                                                lsp::DiagnosticSeverity::ERROR
                                                | lsp::DiagnosticSeverity::WARNING,
                                            ) => true,
                                            (
                                                ScrollbarDiagnostics::Information,
                                                lsp::DiagnosticSeverity::ERROR
                                                | lsp::DiagnosticSeverity::WARNING
                                                | lsp::DiagnosticSeverity::INFORMATION,
                                            ) => true,
                                            (_, _) => false,
                                        }
                                    })
                                    // We want to sort by severity, in order to paint the most severe diagnostics last.
                                    .sorted_by_key(|diagnostic| {
                                        std::cmp::Reverse(diagnostic.diagnostic.severity)
                                    });

                                let marker_row_ranges = diagnostics.into_iter().map(|diagnostic| {
                                    let start_display = diagnostic
                                        .range
                                        .start
                                        .to_display_point(&snapshot.display_snapshot);
                                    let end_display = diagnostic
                                        .range
                                        .end
                                        .to_display_point(&snapshot.display_snapshot);
                                    let color = match diagnostic.diagnostic.severity {
                                        lsp::DiagnosticSeverity::ERROR => theme.status().error,
                                        lsp::DiagnosticSeverity::WARNING => theme.status().warning,
                                        lsp::DiagnosticSeverity::INFORMATION => theme.status().info,
                                        _ => theme.status().hint,
                                    };
                                    ColoredRange {
                                        start: start_display.row(),
                                        end: end_display.row(),
                                        color,
                                    }
                                });
                                marker_quads.extend(
                                    scrollbar_layout
                                        .marker_quads_for_ranges(marker_row_ranges, Some(2)),
                                );
                            }

                            Arc::from(marker_quads)
                        })
                        .await;

                    editor.update(cx, |editor, cx| {
                        editor.scrollbar_marker_state.markers = scrollbar_markers;
                        editor.scrollbar_marker_state.scrollbar_size = scrollbar_size;
                        editor.scrollbar_marker_state.pending_refresh = None;
                        cx.notify();
                    })?;

                    Ok(())
                }));
        });
    }
}

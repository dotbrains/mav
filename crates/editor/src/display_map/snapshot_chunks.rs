use super::*;

impl DisplaySnapshot {
    pub fn chunks(
        &self,
        display_rows: Range<DisplayRow>,
        language_aware: LanguageAwareStyling,
        highlight_styles: HighlightStyles,
    ) -> DisplayChunks<'_> {
        self.block_snapshot.chunks(
            BlockRow(display_rows.start.0)..BlockRow(display_rows.end.0),
            language_aware,
            self.masked,
            Highlights {
                text_highlights: Some(&self.text_highlights),
                inlay_highlights: Some(&self.inlay_highlights),
                semantic_token_highlights: Some(&self.semantic_token_highlights),
                styles: highlight_styles,
            },
        )
    }

    #[instrument(skip_all)]
    pub fn highlighted_chunks<'a>(
        &'a self,
        display_rows: Range<DisplayRow>,
        language_aware: LanguageAwareStyling,
        editor_style: &'a EditorStyle,
    ) -> impl Iterator<Item = HighlightedChunk<'a>> {
        self.chunks(
            display_rows,
            language_aware,
            HighlightStyles {
                inlay_hint: Some(editor_style.inlay_hints_style),
                edit_prediction: Some(editor_style.edit_prediction_styles),
            },
        )
        .flat_map({
            // track the current underline style so that we can apply it to
            // inlay hints within the diagnostic's span
            let mut current_diagnostic_underline: Option<UnderlineStyle> = None;

            move |chunk| {
                let syntax_highlight_style = chunk
                    .syntax_highlight_id
                    .and_then(|id| editor_style.syntax.get(id).cloned());

                let chunk_highlight = chunk.highlight_style.map(|chunk_highlight| {
                    HighlightStyle {
                        // For color inlays, blend the color with the editor background
                        // if the color has transparency (alpha < 1.0)
                        color: chunk_highlight.color.map(|color| {
                            if chunk.is_inlay && !color.is_opaque() {
                                editor_style.background.blend(color)
                            } else {
                                color
                            }
                        }),
                        underline: chunk_highlight
                            .underline
                            .filter(|_| editor_style.show_underlines),
                        ..chunk_highlight
                    }
                });

                let diagnostic_highlight = if chunk.is_inlay {
                    current_diagnostic_underline.map(|underline| HighlightStyle {
                        underline: Some(underline),
                        ..Default::default()
                    })
                } else {
                    let highlight = chunk
                        .diagnostic_severity
                        .filter(|severity| {
                            self.diagnostics_max_severity
                                .into_lsp()
                                .is_some_and(|max_severity| severity <= &max_severity)
                        })
                        .map(|severity| HighlightStyle {
                            fade_out: chunk
                                .is_unnecessary
                                .then_some(editor_style.unnecessary_code_fade),
                            underline: (chunk.underline
                                && editor_style.show_underlines
                                && !(chunk.is_unnecessary
                                    && severity > lsp::DiagnosticSeverity::WARNING))
                                .then(|| {
                                    let diagnostic_color =
                                        diagnostic_style(severity, &editor_style.status);
                                    UnderlineStyle {
                                        color: Some(diagnostic_color),
                                        thickness: 1.0.into(),
                                        wavy: true,
                                    }
                                }),
                            ..Default::default()
                        });

                    current_diagnostic_underline = highlight.as_ref().and_then(|h| h.underline);
                    highlight
                };

                let style = [
                    syntax_highlight_style,
                    chunk_highlight,
                    diagnostic_highlight,
                ]
                .into_iter()
                .flatten()
                .reduce(|acc, highlight| acc.highlight(highlight));

                HighlightedChunk {
                    text: chunk.text,
                    style,
                    is_tab: chunk.is_tab,
                    is_inlay: chunk.is_inlay,
                    replacement: chunk.renderer.map(ChunkReplacement::Renderer),
                }
                .highlight_invisibles(editor_style)
            }
        })
    }

    /// Returns combined highlight styles (tree-sitter syntax + semantic tokens)
    /// for a byte range within the specified buffer.
    /// Returned ranges are 0-based relative to `buffer_range.start`.
    pub(super) fn combined_highlights(
        &self,
        multibuffer_range: Range<MultiBufferOffset>,
        syntax_theme: &theme::SyntaxTheme,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let multibuffer = self.buffer_snapshot();

        let chunks = custom_highlights::CustomHighlightsChunks::new(
            multibuffer_range,
            LanguageAwareStyling {
                tree_sitter: true,
                diagnostics: true,
            },
            None,
            Some(&self.semantic_token_highlights),
            multibuffer,
        );

        let mut highlights = Vec::new();
        let mut offset = 0usize;
        for chunk in chunks {
            let chunk_len = chunk.text.len();
            if chunk_len == 0 {
                continue;
            }

            let syntax_style = chunk
                .syntax_highlight_id
                .and_then(|id| syntax_theme.get(id).cloned());

            let overlay_style = chunk.highlight_style;

            let combined = match (syntax_style, overlay_style) {
                (Some(syntax), Some(overlay)) => Some(syntax.highlight(overlay)),
                (some @ Some(_), None) | (None, some @ Some(_)) => some,
                (None, None) => None,
            };

            if let Some(style) = combined {
                highlights.push((offset..offset + chunk_len, style));
            }
            offset += chunk_len;
        }
        highlights
    }

    #[instrument(skip_all)]
    pub fn layout_row(
        &self,
        display_row: DisplayRow,
        TextLayoutDetails {
            text_system,
            editor_style,
            rem_size,
            scroll_anchor: _,
            visible_rows: _,
            vertical_scroll_margin: _,
        }: &TextLayoutDetails,
    ) -> Arc<LineLayout> {
        let mut runs = Vec::new();
        let mut line = String::new();

        let range = display_row..display_row.next_row();
        for chunk in self.highlighted_chunks(
            range,
            LanguageAwareStyling {
                tree_sitter: false,
                diagnostics: false,
            },
            editor_style,
        ) {
            line.push_str(chunk.text);

            let text_style = if let Some(style) = chunk.style {
                Cow::Owned(editor_style.text.clone().highlight(style))
            } else {
                Cow::Borrowed(&editor_style.text)
            };

            runs.push(text_style.to_run(chunk.text.len()))
        }

        if line.ends_with('\n') {
            line.pop();
            if let Some(last_run) = runs.last_mut() {
                last_run.len -= 1;
                if last_run.len == 0 {
                    runs.pop();
                }
            }
        }

        let font_size = editor_style.text.font_size.to_pixels(*rem_size);
        text_system.layout_line(&line, font_size, &runs, None)
    }

    pub fn x_for_display_point(
        &self,
        display_point: DisplayPoint,
        text_layout_details: &TextLayoutDetails,
    ) -> Pixels {
        let line = self.layout_row(display_point.row(), text_layout_details);
        line.x_for_index(display_point.column() as usize)
    }

    pub fn display_column_for_x(
        &self,
        display_row: DisplayRow,
        x: Pixels,
        details: &TextLayoutDetails,
    ) -> u32 {
        let layout_line = self.layout_row(display_row, details);
        layout_line.closest_index_for_x(x) as u32
    }

    #[instrument(skip_all)]
    pub fn grapheme_at(&self, mut point: DisplayPoint) -> Option<SharedString> {
        point = DisplayPoint(self.block_snapshot.clip_point(point.0, Bias::Left));
        let chars = self
            .text_chunks(point.row())
            .flat_map(str::chars)
            .skip_while({
                let mut column = 0;
                move |char| {
                    let at_point = column >= point.column();
                    column += char.len_utf8() as u32;
                    !at_point
                }
            })
            .take_while({
                let mut prev = false;
                move |char| {
                    let now = char.is_ascii();
                    let end = char.is_ascii() && (char.is_ascii_whitespace() || prev);
                    prev = now;
                    !end
                }
            });
        chars.collect::<String>().graphemes(true).next().map(|s| {
            if let Some(invisible) = s.chars().next().filter(|&c| is_invisible(c)) {
                replacement(invisible).unwrap_or(s).to_owned().into()
            } else if s == "\n" {
                " ".into()
            } else {
                s.to_owned().into()
            }
        })
    }

    pub fn buffer_chars_at(
        &self,
        mut offset: MultiBufferOffset,
    ) -> impl Iterator<Item = (char, MultiBufferOffset)> + '_ {
        self.buffer_snapshot().chars_at(offset).map(move |ch| {
            let ret = (ch, offset);
            offset += ch.len_utf8();
            ret
        })
    }

    pub fn reverse_buffer_chars_at(
        &self,
        mut offset: MultiBufferOffset,
    ) -> impl Iterator<Item = (char, MultiBufferOffset)> + '_ {
        self.buffer_snapshot()
            .reversed_chars_at(offset)
            .map(move |ch| {
                offset -= ch.len_utf8();
                (ch, offset)
            })
    }

    pub fn clip_point(&self, point: DisplayPoint, bias: Bias) -> DisplayPoint {
        let mut clipped = self.block_snapshot.clip_point(point.0, bias);
        if self.clip_at_line_ends {
            clipped = self.clip_at_line_end(DisplayPoint(clipped)).0
        }
        DisplayPoint(clipped)
    }

    pub fn clip_ignoring_line_ends(&self, point: DisplayPoint, bias: Bias) -> DisplayPoint {
        DisplayPoint(self.block_snapshot.clip_point(point.0, bias))
    }

    pub fn inlay_bias_at(&self, point: DisplayPoint) -> Option<Bias> {
        let wrap_point = self.block_snapshot.to_wrap_point(point.0, Bias::Left);
        let tab_point = self.block_snapshot.to_tab_point(wrap_point);
        let (fold_point, _, _) = self
            .block_snapshot
            .tab_snapshot
            .tab_point_to_fold_point(tab_point, Bias::Left);
        let inlay_point =
            fold_point.to_inlay_point(&self.block_snapshot.tab_snapshot.fold_snapshot);
        self.block_snapshot
            .tab_snapshot
            .fold_snapshot
            .inlay_bias_at_point(inlay_point)
    }

    pub fn clip_at_line_end(&self, display_point: DisplayPoint) -> DisplayPoint {
        let mut point = self.display_point_to_point(display_point, Bias::Left);

        if point.column != self.buffer_snapshot().line_len(MultiBufferRow(point.row)) {
            return display_point;
        }
        point.column = point.column.saturating_sub(1);
        point = self.buffer_snapshot().clip_point(point, Bias::Left);
        self.point_to_display_point(point, Bias::Left)
    }

    pub fn folds_in_range<T>(&self, range: Range<T>) -> impl Iterator<Item = &Fold>
    where
        T: ToOffset,
    {
        self.fold_snapshot().folds_in_range(range)
    }

    pub fn blocks_in_range(
        &self,
        rows: Range<DisplayRow>,
    ) -> impl Iterator<Item = (DisplayRow, &Block)> {
        self.block_snapshot
            .blocks_in_range(BlockRow(rows.start.0)..BlockRow(rows.end.0))
            .map(|(row, block)| (DisplayRow(row.0), block))
    }
}

use super::*;

impl LineWithInvisibles {
    pub(crate) fn from_chunks<'a>(
        chunks: impl Iterator<Item = HighlightedChunk<'a>>,
        editor_style: &EditorStyle,
        max_line_len: usize,
        max_line_count: usize,
        editor_mode: &EditorMode,
        text_width: Pixels,
        is_row_soft_wrapped: impl Copy + Fn(usize) -> bool,
        bg_segments_per_row: &[Vec<(Range<DisplayPoint>, Hsla)>],
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<Self> {
        let text_style = &editor_style.text;
        let mut layouts = Vec::with_capacity(max_line_count);
        let mut fragments: SmallVec<[LineFragment; 1]> = SmallVec::new();
        let mut line = String::new();
        // Byte offset into the logical line used to position invisible markers.
        // Unlike `line`, this is not cleared when we flush `shape_line` for
        // mid-line inlays/replacements, so marker offsets stay correct in that case.
        let mut line_byte_offset: usize = 0;
        let mut invisibles = Vec::new();
        let mut width = Pixels::ZERO;
        let mut len = 0;
        let mut styles = Vec::new();
        let mut non_whitespace_added = false;
        let mut row = 0;
        let mut line_exceeded_max_len = false;
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let min_contrast = EditorSettings::get_global(cx).minimum_contrast_for_highlights;

        let ellipsis = SharedString::from("⋯");

        for highlighted_chunk in chunks.chain([HighlightedChunk {
            text: "\n",
            style: None,
            is_tab: false,
            is_inlay: false,
            replacement: None,
        }]) {
            if let Some(replacement) = highlighted_chunk.replacement {
                if line_exceeded_max_len {
                    continue;
                }

                if len + line.len() + highlighted_chunk.text.len() > max_line_len {
                    line_exceeded_max_len = true;
                    continue;
                }

                if !line.is_empty() {
                    let segments = bg_segments_per_row.get(row).map(|v| &v[..]).unwrap_or(&[]);
                    let text_runs: &[TextRun] = if segments.is_empty() {
                        &styles
                    } else {
                        &Self::split_runs_by_bg_segments(&styles, segments, min_contrast, len)
                    };
                    let shaped_line = window.text_system().shape_line(
                        line.clone().into(),
                        font_size,
                        text_runs,
                        None,
                    );
                    width += shaped_line.width;
                    len += shaped_line.len;
                    fragments.push(LineFragment::Text(shaped_line));
                    line.clear();
                    styles.clear();
                }

                match replacement {
                    ChunkReplacement::Renderer(renderer) => {
                        let available_width = if renderer.constrain_width {
                            let chunk = if highlighted_chunk.text == ellipsis.as_ref() {
                                ellipsis.clone()
                            } else {
                                SharedString::from(Arc::from(highlighted_chunk.text))
                            };
                            let shaped_line = window.text_system().shape_line(
                                chunk,
                                font_size,
                                &[text_style.to_run(highlighted_chunk.text.len())],
                                None,
                            );
                            AvailableSpace::Definite(shaped_line.width)
                        } else {
                            AvailableSpace::MinContent
                        };

                        let mut element = (renderer.render)(&mut ChunkRendererContext {
                            context: cx,
                            window,
                            max_width: text_width,
                        });
                        let line_height = text_style.line_height_in_pixels(window.rem_size());
                        let size = element.layout_as_root(
                            size(available_width, AvailableSpace::Definite(line_height)),
                            window,
                            cx,
                        );

                        width += size.width;
                        len += highlighted_chunk.text.len();
                        line_byte_offset += highlighted_chunk.text.len();
                        fragments.push(LineFragment::Element {
                            id: renderer.id,
                            element: Some(element),
                            size,
                            len: highlighted_chunk.text.len(),
                        });
                    }
                    ChunkReplacement::Str(x) => {
                        let text_style = if let Some(style) = highlighted_chunk.style {
                            Cow::Owned(text_style.clone().highlight(style))
                        } else {
                            Cow::Borrowed(text_style)
                        };

                        let run = TextRun {
                            len: x.len(),
                            font: text_style.font(),
                            color: text_style.color,
                            background_color: text_style.background_color,
                            underline: text_style.underline,
                            strikethrough: text_style.strikethrough,
                        };
                        let line_layout = window
                            .text_system()
                            .shape_line(x, font_size, &[run], None)
                            .with_len(highlighted_chunk.text.len());

                        width += line_layout.width;
                        len += highlighted_chunk.text.len();
                        line_byte_offset += highlighted_chunk.text.len();
                        fragments.push(LineFragment::Text(line_layout))
                    }
                }
            } else {
                for (ix, mut line_chunk) in highlighted_chunk.text.split('\n').enumerate() {
                    if ix > 0 {
                        let segments = bg_segments_per_row.get(row).map(|v| &v[..]).unwrap_or(&[]);
                        let text_runs = if segments.is_empty() {
                            &styles
                        } else {
                            &Self::split_runs_by_bg_segments(&styles, segments, min_contrast, len)
                        };
                        let shaped_line = window.text_system().shape_line(
                            line.clone().into(),
                            font_size,
                            text_runs,
                            None,
                        );
                        width += shaped_line.width;
                        len += shaped_line.len;
                        fragments.push(LineFragment::Text(shaped_line));
                        layouts.push(Self {
                            width: mem::take(&mut width),
                            len: mem::take(&mut len),
                            fragments: mem::take(&mut fragments),
                            invisibles: std::mem::take(&mut invisibles),
                            font_size,
                        });

                        line.clear();
                        line_byte_offset = 0;
                        styles.clear();
                        row += 1;
                        line_exceeded_max_len = false;
                        non_whitespace_added = false;
                        if row == max_line_count {
                            return layouts;
                        }
                    }

                    if !line_chunk.is_empty() && !line_exceeded_max_len {
                        let text_style = if let Some(style) = highlighted_chunk.style {
                            Cow::Owned(text_style.clone().highlight(style))
                        } else {
                            Cow::Borrowed(text_style)
                        };

                        let current_line_len = len + line.len();
                        if current_line_len + line_chunk.len() > max_line_len {
                            let mut chunk_len = max_line_len - current_line_len;
                            while !line_chunk.is_char_boundary(chunk_len) {
                                chunk_len -= 1;
                            }
                            line_chunk = &line_chunk[..chunk_len];
                            line_exceeded_max_len = true;
                        }

                        if line_chunk.is_empty() {
                            continue;
                        }

                        styles.push(TextRun {
                            len: line_chunk.len(),
                            font: text_style.font(),
                            color: text_style.color,
                            background_color: text_style.background_color,
                            underline: text_style.underline,
                            strikethrough: text_style.strikethrough,
                        });

                        if editor_mode.is_full() && !highlighted_chunk.is_inlay {
                            // Line wrap pads its contents with fake whitespaces,
                            // avoid printing them
                            let is_soft_wrapped = is_row_soft_wrapped(row);
                            if highlighted_chunk.is_tab {
                                if non_whitespace_added || !is_soft_wrapped {
                                    invisibles.push(Invisible::Tab {
                                        line_start_offset: line_byte_offset,
                                        line_end_offset: line_byte_offset + line_chunk.len(),
                                    });
                                }
                            } else {
                                invisibles.extend(line_chunk.char_indices().filter_map(
                                    |(index, c)| {
                                        let is_whitespace = c.is_whitespace();
                                        non_whitespace_added |= !is_whitespace;
                                        if is_whitespace
                                            && (non_whitespace_added || !is_soft_wrapped)
                                        {
                                            Some(Invisible::Whitespace {
                                                line_start_offset: line_byte_offset + index,
                                                line_end_offset: line_byte_offset
                                                    + index
                                                    + c.len_utf8(),
                                            })
                                        } else {
                                            None
                                        }
                                    },
                                ))
                            }
                        }

                        line.push_str(line_chunk);
                        line_byte_offset += line_chunk.len();
                    }
                }
            }
        }

        layouts
    }
}

impl EditorElement {
    pub(super) fn layout_lines(
        rows: Range<DisplayRow>,
        snapshot: &EditorSnapshot,
        style: &EditorStyle,
        editor_width: Pixels,
        is_row_soft_wrapped: impl Copy + Fn(usize) -> bool,
        bg_segments_per_row: &[Vec<(Range<DisplayPoint>, Hsla)>],
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<LineWithInvisibles> {
        if rows.start >= rows.end {
            return Vec::new();
        }

        // Show the placeholder when the editor is empty
        if snapshot.is_empty() {
            let font_size = style.text.font_size.to_pixels(window.rem_size());
            let placeholder_color = cx.theme().colors().text_placeholder;
            let placeholder_text = snapshot.placeholder_text();

            let placeholder_lines = placeholder_text
                .as_ref()
                .map_or(Vec::new(), |text| text.split('\n').collect::<Vec<_>>());

            let placeholder_line_count = placeholder_lines.len();

            placeholder_lines
                .into_iter()
                .skip(rows.start.0 as usize)
                .chain(iter::repeat(""))
                .take(cmp::max(rows.len(), placeholder_line_count))
                .map(move |line| {
                    let run = TextRun {
                        len: line.len(),
                        font: style.text.font(),
                        color: placeholder_color,
                        ..Default::default()
                    };
                    let line = window.text_system().shape_line(
                        line.to_string().into(),
                        font_size,
                        &[run],
                        None,
                    );
                    LineWithInvisibles {
                        width: line.width,
                        len: line.len,
                        fragments: smallvec![LineFragment::Text(line)],
                        invisibles: Vec::new(),
                        font_size,
                    }
                })
                .collect()
        } else {
            let use_tree_sitter = !snapshot.semantic_tokens_enabled
                || snapshot.use_tree_sitter_for_syntax(rows.start, cx);
            let language_aware = LanguageAwareStyling {
                tree_sitter: use_tree_sitter,
                diagnostics: true,
            };
            let chunks = snapshot.highlighted_chunks(rows.clone(), language_aware, style);
            LineWithInvisibles::from_chunks(
                chunks,
                style,
                MAX_LINE_LEN,
                rows.len(),
                &snapshot.mode,
                editor_width,
                is_row_soft_wrapped,
                bg_segments_per_row,
                window,
                cx,
            )
        }
    }

    pub(super) fn prepaint_lines(
        &self,
        start_row: DisplayRow,
        line_layouts: &mut [LineWithInvisibles],
        line_height: Pixels,
        scroll_position: gpui::Point<ScrollOffset>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        content_origin: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> SmallVec<[AnyElement; 1]> {
        let mut line_elements = SmallVec::new();
        for (ix, line) in line_layouts.iter_mut().enumerate() {
            let row = start_row + DisplayRow(ix as u32);
            line.prepaint(
                line_height,
                scroll_position,
                scroll_pixel_position,
                row,
                content_origin,
                &mut line_elements,
                window,
                cx,
            );
        }
        line_elements
    }
}

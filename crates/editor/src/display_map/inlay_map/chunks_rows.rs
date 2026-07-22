use super::*;

impl InlayChunks<'_> {
    #[ztracing::instrument(skip_all)]
    pub fn seek(&mut self, new_range: Range<InlayOffset>) {
        self.transforms.seek(&new_range.start, Bias::Right);

        let buffer_range = self.snapshot.to_buffer_offset(new_range.start)
            ..self.snapshot.to_buffer_offset(new_range.end);
        self.buffer_chunks.seek(buffer_range);
        self.inlay_chunks = None;
        self.buffer_chunk = None;
        self.output_offset = new_range.start;
        self.max_output_offset = new_range.end;
    }

    pub fn offset(&self) -> InlayOffset {
        self.output_offset
    }
}

impl<'a> Iterator for InlayChunks<'a> {
    type Item = InlayChunk<'a>;

    #[ztracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.output_offset == self.max_output_offset {
            return None;
        }

        let chunk = match self.transforms.item()? {
            Transform::Isomorphic(_) => {
                let chunk = self
                    .buffer_chunk
                    .get_or_insert_with(|| self.buffer_chunks.next().unwrap());
                if chunk.text.is_empty() {
                    *chunk = self.buffer_chunks.next().unwrap();
                }

                let desired_bytes = self.transforms.end().0.0 - self.output_offset.0;

                // If we're already at the transform boundary, skip to the next transform
                if desired_bytes == 0 {
                    self.inlay_chunks = None;
                    self.transforms.next();
                    return self.next();
                }

                // Determine split index handling edge cases
                let split_index = if desired_bytes >= chunk.text.len() {
                    chunk.text.len()
                } else {
                    chunk.text.ceil_char_boundary(desired_bytes)
                };

                let (prefix, suffix) = chunk.text.split_at(split_index);
                self.output_offset.0 += prefix.len();

                let mask = 1u128.unbounded_shl(split_index as u32).wrapping_sub(1);
                let chars = chunk.chars & mask;
                let tabs = chunk.tabs & mask;
                let newlines = chunk.newlines & mask;

                chunk.chars = chunk.chars.unbounded_shr(split_index as u32);
                chunk.tabs = chunk.tabs.unbounded_shr(split_index as u32);
                chunk.newlines = chunk.newlines.unbounded_shr(split_index as u32);
                chunk.text = suffix;

                InlayChunk {
                    chunk: Chunk {
                        text: prefix,
                        chars,
                        tabs,
                        newlines,
                        ..chunk.clone()
                    },
                    renderer: None,
                }
            }
            Transform::Inlay(inlay) => {
                let mut inlay_style_and_highlight = None;
                if let Some(inlay_highlights) = self.highlights.inlay_highlights {
                    for (_, inlay_id_to_data) in inlay_highlights.iter() {
                        let style_and_highlight = inlay_id_to_data.get(&inlay.id);
                        if style_and_highlight.is_some() {
                            inlay_style_and_highlight = style_and_highlight;
                            break;
                        }
                    }
                }

                let mut renderer = None;
                let mut highlight_style = match inlay.id {
                    InlayId::EditPrediction(_) => self.highlight_styles.edit_prediction.map(|s| {
                        if inlay.text().chars().all(|c| c.is_whitespace()) {
                            s.whitespace
                        } else {
                            s.insertion
                        }
                    }),
                    InlayId::Hint(_) => self.highlight_styles.inlay_hint,
                    InlayId::DebuggerValue(_) => self.highlight_styles.inlay_hint,
                    InlayId::ReplResult(_) => {
                        let text = inlay.text().to_string();
                        renderer = Some(ChunkRenderer {
                            id: ChunkRendererId::Inlay(inlay.id),
                            render: Arc::new(move |cx| {
                                let colors = cx.theme().colors();
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .child(div().w_4())
                                    .child(
                                        div()
                                            .px_1()
                                            .rounded_sm()
                                            .bg(colors.surface_background)
                                            .text_color(colors.text_muted)
                                            .text_xs()
                                            .child(text.trim().to_string()),
                                    )
                                    .into_any_element()
                            }),
                            constrain_width: false,
                            measured_width: None,
                        });
                        self.highlight_styles.inlay_hint
                    }
                    InlayId::Color(_) => {
                        if let InlayContent::Color(color) = inlay.content {
                            renderer = Some(ChunkRenderer {
                                id: ChunkRendererId::Inlay(inlay.id),
                                render: Arc::new(move |cx| {
                                    div()
                                        .relative()
                                        .size_3p5()
                                        .child(
                                            div()
                                                .absolute()
                                                .right_1()
                                                .size_3()
                                                .border_1()
                                                .border_color(
                                                    if cx.theme().appearance().is_light() {
                                                        gpui::black().opacity(0.5)
                                                    } else {
                                                        gpui::white().opacity(0.5)
                                                    },
                                                )
                                                .bg(color),
                                        )
                                        .into_any_element()
                                }),
                                constrain_width: false,
                                measured_width: None,
                            });
                        }
                        self.highlight_styles.inlay_hint
                    }
                };
                let next_inlay_highlight_endpoint;
                let offset_in_inlay = self.output_offset - self.transforms.start().0;
                if let Some((style, highlight)) = inlay_style_and_highlight {
                    let range = &highlight.range;
                    if offset_in_inlay < range.start {
                        next_inlay_highlight_endpoint = range.start - offset_in_inlay;
                    } else if offset_in_inlay >= range.end {
                        next_inlay_highlight_endpoint = usize::MAX;
                    } else {
                        next_inlay_highlight_endpoint = range.end - offset_in_inlay;
                        highlight_style = highlight_style
                            .map(|highlight| highlight.highlight(*style))
                            .or_else(|| Some(*style));
                    }
                } else {
                    next_inlay_highlight_endpoint = usize::MAX;
                }

                let inlay_chunks = self.inlay_chunks.get_or_insert_with(|| {
                    let start = offset_in_inlay;
                    let end = cmp::min(self.max_output_offset, self.transforms.end().0)
                        - self.transforms.start().0;
                    let chunks = inlay.text().chunks_in_range(start..end);
                    text::ChunkWithBitmaps(chunks)
                });
                let ChunkBitmaps {
                    text: inlay_chunk,
                    chars,
                    tabs,
                    newlines,
                } = self
                    .inlay_chunk
                    .get_or_insert_with(|| inlay_chunks.next().unwrap());

                // Determine split index handling edge cases
                let split_index = if next_inlay_highlight_endpoint >= inlay_chunk.len() {
                    inlay_chunk.len()
                } else if next_inlay_highlight_endpoint == 0 {
                    // Need to take at least one character to make progress
                    inlay_chunk
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(1)
                } else {
                    inlay_chunk.ceil_char_boundary(next_inlay_highlight_endpoint)
                };

                let (chunk, remainder) = inlay_chunk.split_at(split_index);
                *inlay_chunk = remainder;

                let mask = 1u128.unbounded_shl(split_index as u32).wrapping_sub(1);
                let new_chars = *chars & mask;
                let new_tabs = *tabs & mask;
                let new_newlines = *newlines & mask;

                *chars = chars.unbounded_shr(split_index as u32);
                *tabs = tabs.unbounded_shr(split_index as u32);
                *newlines = newlines.unbounded_shr(split_index as u32);

                if inlay_chunk.is_empty() {
                    self.inlay_chunk = None;
                }

                self.output_offset.0 += chunk.len();

                InlayChunk {
                    chunk: Chunk {
                        text: chunk,
                        chars: new_chars,
                        tabs: new_tabs,
                        newlines: new_newlines,
                        highlight_style,
                        is_inlay: true,
                        ..Chunk::default()
                    },
                    renderer,
                }
            }
        };

        if self.output_offset >= self.transforms.end().0 {
            self.inlay_chunks = None;
            self.transforms.next();
        }

        Some(chunk)
    }
}

impl InlayBufferRows<'_> {
    #[ztracing::instrument(skip_all)]
    pub fn seek(&mut self, row: u32) {
        let inlay_point = InlayPoint::new(row, 0);
        self.transforms.seek(&inlay_point, Bias::Left);

        let mut buffer_point = self.transforms.start().1;
        let buffer_row = MultiBufferRow(if row == 0 {
            0
        } else {
            match self.transforms.item() {
                Some(Transform::Isomorphic(_)) => {
                    buffer_point += inlay_point.0 - self.transforms.start().0.0;
                    buffer_point.row
                }
                _ => cmp::min(buffer_point.row + 1, self.max_buffer_row.0),
            }
        });
        self.inlay_row = inlay_point.row();
        self.buffer_rows.seek(buffer_row);
    }
}

impl Iterator for InlayBufferRows<'_> {
    type Item = RowInfo;

    #[ztracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        let buffer_row = if self.inlay_row == 0 {
            self.buffer_rows.next().unwrap()
        } else {
            match self.transforms.item()? {
                Transform::Inlay(_) => Default::default(),
                Transform::Isomorphic(_) => self.buffer_rows.next().unwrap(),
            }
        };

        self.inlay_row += 1;
        self.transforms
            .seek_forward(&InlayPoint::new(self.inlay_row, 0), Bias::Left);

        Some(buffer_row)
    }
}

impl InlayPoint {
    pub fn new(row: u32, column: u32) -> Self {
        Self(Point::new(row, column))
    }

    pub fn row(self) -> u32 {
        self.0.row
    }
}

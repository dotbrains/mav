use super::*;

pub(super) struct RenderedLine {
    pub(super) layout: TextLayout,
    pub(super) source_mappings: Vec<SourceMapping>,
    pub(super) source_end: usize,
    pub(super) language: Option<Arc<Language>>,
    pub(super) text_align: TextAlign,
}

impl RenderedLine {
    pub(super) fn rendered_index_for_source_index(&self, source_index: usize) -> usize {
        if source_index >= self.source_end {
            return self.layout.len();
        }

        let mapping = match self
            .source_mappings
            .binary_search_by_key(&source_index, |probe| probe.source_index)
        {
            Ok(ix) => &self.source_mappings[ix],
            Err(ix) => &self.source_mappings[ix - 1],
        };
        (mapping.rendered_index + (source_index - mapping.source_index)).min(self.layout.len())
    }

    pub(super) fn source_index_for_rendered_index(&self, rendered_index: usize) -> usize {
        if rendered_index >= self.layout.len() {
            return self.source_end;
        }

        let mapping = match self
            .source_mappings
            .binary_search_by_key(&rendered_index, |probe| probe.rendered_index)
        {
            Ok(ix) => &self.source_mappings[ix],
            Err(ix) => &self.source_mappings[ix - 1],
        };
        mapping.source_index + (rendered_index - mapping.rendered_index)
    }

    /// Returns the source index for use as an exclusive range end at a word/selection boundary.
    /// When the rendered index is exactly at the start of a segment with a gap from the previous
    /// segment (e.g., after stripped markdown syntax like backticks), this returns the end of the
    /// previous segment rather than the start of the current one.
    pub(super) fn source_index_for_exclusive_rendered_end(&self, rendered_index: usize) -> usize {
        if rendered_index >= self.layout.len() {
            return self.source_end;
        }

        let ix = match self
            .source_mappings
            .binary_search_by_key(&rendered_index, |probe| probe.rendered_index)
        {
            Ok(ix) => ix,
            Err(ix) => {
                return self.source_mappings[ix - 1].source_index
                    + (rendered_index - self.source_mappings[ix - 1].rendered_index);
            }
        };

        // Exact match at the start of a segment. Check if there's a gap from the previous segment.
        if ix > 0 {
            let prev_mapping = &self.source_mappings[ix - 1];
            let mapping = &self.source_mappings[ix];
            let prev_segment_len = mapping.rendered_index - prev_mapping.rendered_index;
            let prev_source_end = prev_mapping.source_index + prev_segment_len;
            if prev_source_end < mapping.source_index {
                return prev_source_end;
            }
        }

        self.source_mappings[ix].source_index
    }

    pub(super) fn alignment_offset_for_segment(
        &self,
        available_width: Pixels,
        segment_start_x: Pixels,
        segment_end_x: Pixels,
    ) -> Pixels {
        let segment_width = segment_end_x - segment_start_x;
        match self.text_align {
            TextAlign::Left => px(0.),
            TextAlign::Center => ((available_width - segment_width) / 2.).max(px(0.)),
            TextAlign::Right => (available_width - segment_width).max(px(0.)),
        }
    }

    pub(super) fn source_index_for_position(
        &self,
        position: Point<Pixels>,
    ) -> Result<usize, usize> {
        let adjusted_position = maybe!({
            if self.text_align == TextAlign::Left {
                return None;
            }

            let Some(wrapped_line) = self.layout.line_layout_for_index(0) else {
                return None;
            };

            let bounds = self.layout.bounds();
            let line_height = self.layout.line_height();
            let relative_y = (position.y - bounds.top()).max(px(0.));
            let wrapped_row_ix = (relative_y / line_height) as usize;
            let boundaries = wrapped_line.wrap_boundaries();

            let segment_start_x = if wrapped_row_ix == 0 {
                px(0.)
            } else {
                boundaries
                    .get(wrapped_row_ix - 1)
                    .map(|b| {
                        wrapped_line.unwrapped_layout.runs[b.run_ix].glyphs[b.glyph_ix]
                            .position
                            .x
                    })
                    .unwrap_or(px(0.))
            };
            let segment_end_x = boundaries
                .get(wrapped_row_ix)
                .map(|b| {
                    wrapped_line.unwrapped_layout.runs[b.run_ix].glyphs[b.glyph_ix]
                        .position
                        .x
                })
                .unwrap_or(wrapped_line.unwrapped_layout.width);

            let alignment_offset = self.alignment_offset_for_segment(
                bounds.size.width,
                segment_start_x,
                segment_end_x,
            );
            Some(point(position.x - alignment_offset, position.y))
        })
        .unwrap_or(position);

        let line_rendered_index;
        let out_of_bounds;
        match self.layout.index_for_position(adjusted_position) {
            Ok(ix) => {
                line_rendered_index = ix;
                out_of_bounds = false;
            }
            Err(ix) => {
                line_rendered_index = ix;
                out_of_bounds = true;
            }
        };
        let source_index = self.source_index_for_rendered_index(line_rendered_index);
        if out_of_bounds {
            Err(source_index)
        } else {
            Ok(source_index)
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub(super) struct SourceMapping {
    pub(super) rendered_index: usize,
    pub(super) source_index: usize,
}

pub(super) fn source_range_for_rendered(
    mappings: &[SourceMapping],
    rendered: &Range<usize>,
) -> Option<Range<usize>> {
    if rendered.start >= rendered.end {
        return None;
    }
    let start = source_index_for_rendered(mappings, rendered.start)?;
    let end = source_index_for_rendered(mappings, rendered.end - 1)? + 1;
    Some(start..end)
}

fn source_index_for_rendered(mappings: &[SourceMapping], rendered_index: usize) -> Option<usize> {
    let mut last: Option<&SourceMapping> = None;
    for mapping in mappings {
        if mapping.rendered_index <= rendered_index {
            last = Some(mapping);
        } else {
            break;
        }
    }
    last.map(|m| m.source_index + (rendered_index - m.rendered_index))
}

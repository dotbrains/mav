use super::*;

impl LineWithInvisibles {
    /// Takes text runs and non-overlapping left-to-right background ranges with color.
    /// Returns new text runs with adjusted contrast as per background ranges.
    pub(super) fn split_runs_by_bg_segments(
        text_runs: &[TextRun],
        bg_segments: &[(Range<DisplayPoint>, Hsla)],
        min_contrast: f32,
        start_col_offset: usize,
    ) -> Vec<TextRun> {
        let mut output_runs: Vec<TextRun> = Vec::with_capacity(text_runs.len());
        let mut line_col = start_col_offset;
        let mut segment_ix = 0usize;

        for text_run in text_runs.iter() {
            let run_start_col = line_col;
            let run_end_col = run_start_col + text_run.len;
            while segment_ix < bg_segments.len()
                && (bg_segments[segment_ix].0.end.column() as usize) <= run_start_col
            {
                segment_ix += 1;
            }
            let mut cursor_col = run_start_col;
            let mut local_segment_ix = segment_ix;
            while local_segment_ix < bg_segments.len() {
                let (range, segment_color) = &bg_segments[local_segment_ix];
                let segment_start_col = range.start.column() as usize;
                let segment_end_col = range.end.column() as usize;
                if segment_start_col >= run_end_col {
                    break;
                }
                if segment_start_col > cursor_col {
                    let span_len = segment_start_col - cursor_col;
                    output_runs.push(TextRun {
                        len: span_len,
                        font: text_run.font.clone(),
                        color: text_run.color,
                        background_color: text_run.background_color,
                        underline: text_run.underline,
                        strikethrough: text_run.strikethrough,
                    });
                    cursor_col = segment_start_col;
                }
                let segment_slice_end_col = segment_end_col.min(run_end_col);
                if segment_slice_end_col > cursor_col {
                    let new_text_color =
                        ensure_minimum_contrast(text_run.color, *segment_color, min_contrast);
                    output_runs.push(TextRun {
                        len: segment_slice_end_col - cursor_col,
                        font: text_run.font.clone(),
                        color: new_text_color,
                        background_color: text_run.background_color,
                        underline: text_run.underline,
                        strikethrough: text_run.strikethrough,
                    });
                    cursor_col = segment_slice_end_col;
                }
                if segment_end_col >= run_end_col {
                    break;
                }
                local_segment_ix += 1;
            }
            if cursor_col < run_end_col {
                output_runs.push(TextRun {
                    len: run_end_col - cursor_col,
                    font: text_run.font.clone(),
                    color: text_run.color,
                    background_color: text_run.background_color,
                    underline: text_run.underline,
                    strikethrough: text_run.strikethrough,
                });
            }
            line_col = run_end_col;
            segment_ix = local_segment_ix;
        }
        output_runs
    }

    pub fn x_for_index(&self, index: usize) -> Pixels {
        let mut fragment_start_x = Pixels::ZERO;
        let mut fragment_start_index = 0;

        for fragment in &self.fragments {
            match fragment {
                LineFragment::Text(shaped_line) => {
                    let fragment_end_index = fragment_start_index + shaped_line.len;
                    if index < fragment_end_index {
                        return fragment_start_x
                            + shaped_line.x_for_index(index - fragment_start_index);
                    }
                    fragment_start_x += shaped_line.width;
                    fragment_start_index = fragment_end_index;
                }
                LineFragment::Element { len, size, .. } => {
                    let fragment_end_index = fragment_start_index + len;
                    if index < fragment_end_index {
                        return fragment_start_x;
                    }
                    fragment_start_x += size.width;
                    fragment_start_index = fragment_end_index;
                }
            }
        }

        fragment_start_x
    }

    pub fn index_for_x(&self, x: Pixels) -> Option<usize> {
        let mut fragment_start_x = Pixels::ZERO;
        let mut fragment_start_index = 0;

        for fragment in &self.fragments {
            match fragment {
                LineFragment::Text(shaped_line) => {
                    let fragment_end_x = fragment_start_x + shaped_line.width;
                    if x < fragment_end_x {
                        return Some(
                            fragment_start_index + shaped_line.index_for_x(x - fragment_start_x)?,
                        );
                    }
                    fragment_start_x = fragment_end_x;
                    fragment_start_index += shaped_line.len;
                }
                LineFragment::Element { len, size, .. } => {
                    let fragment_end_x = fragment_start_x + size.width;
                    if x < fragment_end_x {
                        return Some(fragment_start_index);
                    }
                    fragment_start_index += len;
                    fragment_start_x = fragment_end_x;
                }
            }
        }

        None
    }

    pub fn font_id_for_index(&self, index: usize) -> Option<FontId> {
        let mut fragment_start_index = 0;

        for fragment in &self.fragments {
            match fragment {
                LineFragment::Text(shaped_line) => {
                    let fragment_end_index = fragment_start_index + shaped_line.len;
                    if index < fragment_end_index {
                        return shaped_line.font_id_for_index(index - fragment_start_index);
                    }
                    fragment_start_index = fragment_end_index;
                }
                LineFragment::Element { len, .. } => {
                    let fragment_end_index = fragment_start_index + len;
                    if index < fragment_end_index {
                        return None;
                    }
                    fragment_start_index = fragment_end_index;
                }
            }
        }

        None
    }

    pub fn alignment_offset(&self, text_align: TextAlign, content_width: Pixels) -> Pixels {
        match text_align {
            TextAlign::Left => px(0.0),
            TextAlign::Center => (content_width - self.width) / 2.0,
            TextAlign::Right => content_width - self.width,
        }
    }
}

use super::*;
use gpui::{StyledText, TextStyle};
use smallvec::SmallVec;
use std::fmt::Write as _;

/// A runnable is a set of data about a region that could be resolved into a task
pub struct Runnable {
    pub tags: SmallVec<[RunnableTag; 1]>,
    pub language: Arc<Language>,
    pub buffer: BufferId,
}

#[derive(Default, Clone, Debug)]
pub struct HighlightedText {
    pub text: SharedString,
    pub highlights: Vec<(Range<usize>, HighlightStyle)>,
}

#[derive(Default, Debug)]
pub struct HighlightedTextBuilder {
    text: String,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
}

impl HighlightedText {
    pub fn from_buffer_range<T: ToOffset>(
        range: Range<T>,
        snapshot: &text::BufferSnapshot,
        syntax_snapshot: &SyntaxSnapshot,
        override_style: Option<HighlightStyle>,
        syntax_theme: &SyntaxTheme,
    ) -> Self {
        let mut highlighted_text = HighlightedTextBuilder::default();
        highlighted_text.add_text_from_buffer_range(
            range,
            snapshot,
            syntax_snapshot,
            override_style,
            syntax_theme,
        );
        highlighted_text.build()
    }

    pub fn to_styled_text(&self, default_style: &TextStyle) -> StyledText {
        gpui::StyledText::new(self.text.clone())
            .with_default_highlights(default_style, self.highlights.iter().cloned())
    }

    /// Returns the first line without leading whitespace unless highlighted
    /// and a boolean indicating if there are more lines after
    pub fn first_line_preview(self) -> (Self, bool) {
        let newline_ix = self.text.find('\n').unwrap_or(self.text.len());
        let first_line = &self.text[..newline_ix];

        // Trim leading whitespace, unless an edit starts prior to it.
        let mut preview_start_ix = first_line.len() - first_line.trim_start().len();
        if let Some((first_highlight_range, _)) = self.highlights.first() {
            preview_start_ix = preview_start_ix.min(first_highlight_range.start);
        }

        let preview_text = &first_line[preview_start_ix..];
        let preview_highlights = self
            .highlights
            .into_iter()
            .skip_while(|(range, _)| range.end <= preview_start_ix)
            .take_while(|(range, _)| range.start < newline_ix)
            .filter_map(|(mut range, highlight)| {
                range.start = range.start.saturating_sub(preview_start_ix);
                range.end = range.end.min(newline_ix).saturating_sub(preview_start_ix);
                if range.is_empty() {
                    None
                } else {
                    Some((range, highlight))
                }
            });

        let preview = Self {
            text: SharedString::new(preview_text),
            highlights: preview_highlights.collect(),
        };

        (preview, self.text.len() > newline_ix)
    }
}

impl HighlightedTextBuilder {
    pub fn build(self) -> HighlightedText {
        HighlightedText {
            text: self.text.into(),
            highlights: self.highlights,
        }
    }

    /// Append a displayable value to the text, highlighting its range with
    /// `style`.
    pub fn push_styled(&mut self, value: impl std::fmt::Display, style: HighlightStyle) {
        let start = self.text.len();
        let _ = write!(&mut self.text, "{value}");
        let end = self.text.len();
        if end > start {
            self.highlights.push((start..end, style));
        }
    }

    /// Append a displayable value to the text without any highlighting.
    pub fn push_plain(&mut self, value: impl std::fmt::Display) {
        let _ = write!(&mut self.text, "{value}");
    }

    pub fn add_text_from_buffer_range<T: ToOffset>(
        &mut self,
        range: Range<T>,
        snapshot: &text::BufferSnapshot,
        syntax_snapshot: &SyntaxSnapshot,
        override_style: Option<HighlightStyle>,
        syntax_theme: &SyntaxTheme,
    ) {
        let range = range.to_offset(snapshot);
        for chunk in Self::highlighted_chunks(range, snapshot, syntax_snapshot) {
            let start = self.text.len();
            self.text.push_str(chunk.text);
            let end = self.text.len();

            if let Some(highlight_style) = chunk
                .syntax_highlight_id
                .and_then(|id| syntax_theme.get(id).cloned())
            {
                let highlight_style = override_style.map_or(highlight_style, |override_style| {
                    highlight_style.highlight(override_style)
                });
                self.highlights.push((start..end, highlight_style));
            } else if let Some(override_style) = override_style {
                self.highlights.push((start..end, override_style));
            }
        }
    }

    fn highlighted_chunks<'a>(
        range: Range<usize>,
        snapshot: &'a text::BufferSnapshot,
        syntax_snapshot: &'a SyntaxSnapshot,
    ) -> BufferChunks<'a> {
        let captures = syntax_snapshot.captures(range.clone(), snapshot, |grammar| {
            grammar
                .highlights_config
                .as_ref()
                .map(|config| &config.query)
        });

        let highlight_maps = captures
            .grammars()
            .iter()
            .map(|grammar| grammar.highlight_map())
            .collect();

        BufferChunks::new(
            snapshot.as_rope(),
            range,
            Some((captures, highlight_maps)),
            false,
            None,
        )
    }
}

#[derive(Clone)]
pub struct EditPreview {
    old_snapshot: text::BufferSnapshot,
    applied_edits_snapshot: text::BufferSnapshot,
    syntax_snapshot: SyntaxSnapshot,
}

impl EditPreview {
    pub(crate) fn new(
        old_snapshot: text::BufferSnapshot,
        applied_edits_snapshot: text::BufferSnapshot,
        syntax_snapshot: SyntaxSnapshot,
    ) -> Self {
        Self {
            old_snapshot,
            applied_edits_snapshot,
            syntax_snapshot,
        }
    }

    pub fn unchanged(snapshot: &BufferSnapshot) -> Self {
        Self {
            old_snapshot: snapshot.text.clone(),
            applied_edits_snapshot: snapshot.text.clone(),
            syntax_snapshot: snapshot.syntax.clone(),
        }
    }

    pub fn as_unified_diff(
        &self,
        file: Option<&Arc<dyn File>>,
        edits: &[(Range<Anchor>, impl AsRef<str>)],
    ) -> Option<String> {
        let (first, _) = edits.first()?;
        let (last, _) = edits.last()?;

        let start = first.start.to_point(&self.old_snapshot);
        let old_end = last.end.to_point(&self.old_snapshot);
        let new_end = last
            .end
            .bias_right(&self.old_snapshot)
            .to_point(&self.applied_edits_snapshot);

        let start = Point::new(start.row.saturating_sub(3), 0);
        let old_end = Point::new(old_end.row + 4, 0).min(self.old_snapshot.max_point());
        let new_end = Point::new(new_end.row + 4, 0).min(self.applied_edits_snapshot.max_point());

        let diff_body = unified_diff_with_offsets(
            &self
                .old_snapshot
                .text_for_range(start..old_end)
                .collect::<String>(),
            &self
                .applied_edits_snapshot
                .text_for_range(start..new_end)
                .collect::<String>(),
            start.row,
            start.row,
        );

        let path = file.map(|f| f.path().as_unix_str());
        let header = match path {
            Some(p) => format!("--- a/{}\n+++ b/{}\n", p, p),
            None => String::new(),
        };

        Some(format!("{}{}", header, diff_body))
    }

    pub fn highlight_edits(
        &self,
        current_snapshot: &BufferSnapshot,
        edits: &[(Range<Anchor>, impl AsRef<str>)],
        include_deletions: bool,
        cx: &App,
    ) -> HighlightedText {
        let Some(visible_range_in_preview_snapshot) = self.compute_visible_range(edits) else {
            return HighlightedText::default();
        };

        let mut highlighted_text = HighlightedTextBuilder::default();

        let visible_range_in_preview_snapshot =
            visible_range_in_preview_snapshot.to_offset(&self.applied_edits_snapshot);
        let mut offset_in_preview_snapshot = visible_range_in_preview_snapshot.start;

        let insertion_highlight_style = HighlightStyle {
            background_color: Some(cx.theme().status().created_background),
            ..Default::default()
        };
        let deletion_highlight_style = HighlightStyle {
            background_color: Some(cx.theme().status().deleted_background),
            ..Default::default()
        };
        let syntax_theme = cx.theme().syntax();

        for (range, edit_text) in edits {
            let edit_new_end_in_preview_snapshot = range
                .end
                .bias_right(&self.old_snapshot)
                .to_offset(&self.applied_edits_snapshot);
            let edit_start_in_preview_snapshot =
                edit_new_end_in_preview_snapshot - edit_text.as_ref().len();

            let unchanged_range_in_preview_snapshot =
                offset_in_preview_snapshot..edit_start_in_preview_snapshot;
            if !unchanged_range_in_preview_snapshot.is_empty() {
                highlighted_text.add_text_from_buffer_range(
                    unchanged_range_in_preview_snapshot,
                    &self.applied_edits_snapshot,
                    &self.syntax_snapshot,
                    None,
                    syntax_theme,
                );
            }

            let range_in_current_snapshot = range.to_offset(current_snapshot);
            if include_deletions && !range_in_current_snapshot.is_empty() {
                highlighted_text.add_text_from_buffer_range(
                    range_in_current_snapshot,
                    &current_snapshot.text,
                    &current_snapshot.syntax,
                    Some(deletion_highlight_style),
                    syntax_theme,
                );
            }

            if !edit_text.as_ref().is_empty() {
                highlighted_text.add_text_from_buffer_range(
                    edit_start_in_preview_snapshot..edit_new_end_in_preview_snapshot,
                    &self.applied_edits_snapshot,
                    &self.syntax_snapshot,
                    Some(insertion_highlight_style),
                    syntax_theme,
                );
            }

            offset_in_preview_snapshot = edit_new_end_in_preview_snapshot;
        }

        highlighted_text.add_text_from_buffer_range(
            offset_in_preview_snapshot..visible_range_in_preview_snapshot.end,
            &self.applied_edits_snapshot,
            &self.syntax_snapshot,
            None,
            syntax_theme,
        );

        highlighted_text.build()
    }

    pub fn build_result_buffer(&self, cx: &mut App) -> Entity<Buffer> {
        cx.new(|cx| {
            let mut buffer = Buffer::local_normalized(
                self.applied_edits_snapshot.as_rope().clone(),
                self.applied_edits_snapshot.line_ending(),
                cx,
            );
            buffer.set_language_async(self.syntax_snapshot.root_language(), cx);
            buffer
        })
    }

    pub fn result_text_snapshot(&self) -> &text::BufferSnapshot {
        &self.applied_edits_snapshot
    }

    pub fn result_syntax_snapshot(&self) -> &SyntaxSnapshot {
        &self.syntax_snapshot
    }

    pub fn anchor_to_offset_in_result(&self, anchor: Anchor) -> usize {
        anchor
            .bias_right(&self.old_snapshot)
            .to_offset(&self.applied_edits_snapshot)
    }

    pub fn compute_visible_range<T>(&self, edits: &[(Range<Anchor>, T)]) -> Option<Range<Point>> {
        let (first, _) = edits.first()?;
        let (last, _) = edits.last()?;

        let start = first
            .start
            .bias_left(&self.old_snapshot)
            .to_point(&self.applied_edits_snapshot);
        let end = last
            .end
            .bias_right(&self.old_snapshot)
            .to_point(&self.applied_edits_snapshot);

        // Ensure that the first line of the first edit and the last line of the last edit are always fully visible
        let range = Point::new(start.row, 0)
            ..Point::new(end.row, self.applied_edits_snapshot.line_len(end.row));

        Some(range)
    }
}

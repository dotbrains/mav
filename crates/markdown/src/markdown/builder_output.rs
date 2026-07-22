use super::*;
use crate::rendered_line::source_range_for_rendered;

impl MarkdownElementBuilder {
    pub(super) fn source_range_for_rendered(
        &self,
        rendered: &Range<usize>,
    ) -> Option<Range<usize>> {
        source_range_for_rendered(&self.pending_line.source_mappings, rendered)
    }

    pub(super) fn render_source_anchor(&mut self, source_range: Range<usize>) -> AnyElement {
        let mut text_style = self.base_text_style.clone();
        text_style.color = Hsla::transparent_black();
        let text = "\u{200B}";
        let styled_text = StyledText::new(text).with_runs(vec![text_style.to_run(text.len())]);
        self.rendered_lines.push(RenderedLine {
            layout: styled_text.layout().clone(),
            source_mappings: vec![SourceMapping {
                rendered_index: 0,
                source_index: source_range.start,
            }],
            source_end: source_range.end,
            language: None,
            text_align: TextAlign::Left,
        });
        div()
            .absolute()
            .top_0()
            .left_0()
            .opacity(0.)
            .child(styled_text)
            .into_any_element()
    }

    pub(super) fn flush_text(&mut self) {
        let text_align = self.text_style().text_align;
        let line = mem::take(&mut self.pending_line);
        if line.text.is_empty() {
            return;
        }

        let text = StyledText::new(line.text).with_runs(line.runs);
        self.rendered_lines.push(RenderedLine {
            layout: text.layout().clone(),
            source_mappings: line.source_mappings,
            source_end: self.current_source_index,
            language: self.code_block_stack.last().cloned().flatten(),
            text_align,
        });
        self.append_child(text.into_any());
    }

    pub(super) fn build(mut self) -> RenderedMarkdown {
        debug_assert_eq!(self.div_stack.len(), 1);
        self.flush_text();
        RenderedMarkdown {
            element: self.div_stack.pop().unwrap().div.into_any_element(),
            text: RenderedText {
                lines: self.rendered_lines.into(),
                links: self.rendered_links.into(),
                footnote_refs: self.rendered_footnote_refs.into(),
            },
        }
    }
}

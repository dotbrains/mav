use std::ops::Range;

use gpui::{App, FontStyle, FontWeight, StrikethroughStyle, TextStyleRefinement, UnderlineStyle};
use ui::prelude::*;

use crate::html::html_parser::{
    HtmlHighlightStyle, HtmlImage, HtmlParagraph, HtmlParagraphChunk, ParsedHtmlBlock,
    ParsedHtmlElement, ParsedHtmlList, ParsedHtmlListItemType, ParsedHtmlText,
};
use crate::{MarkdownElement, MarkdownElementBuilder};

mod table;

pub(crate) struct HtmlSourceAllocator {
    source_range: Range<usize>,
    next_source_index: usize,
}

impl HtmlSourceAllocator {
    pub(crate) fn new(source_range: Range<usize>) -> Self {
        Self {
            next_source_index: source_range.start,
            source_range,
        }
    }

    pub(crate) fn allocate(&mut self, requested_len: usize) -> Range<usize> {
        let remaining = self.source_range.end.saturating_sub(self.next_source_index);
        let len = requested_len.min(remaining);
        let start = self.next_source_index;
        let end = start + len;
        self.next_source_index = end;
        start..end
    }
}

impl MarkdownElement {
    pub(crate) fn render_html_block(
        &self,
        block: &ParsedHtmlBlock,
        builder: &mut MarkdownElementBuilder,
        markdown_end: usize,
        cx: &mut App,
    ) {
        let mut source_allocator = HtmlSourceAllocator::new(block.source_range.clone());
        self.render_html_elements(
            &block.children,
            &mut source_allocator,
            builder,
            markdown_end,
            cx,
        );
    }

    fn render_html_elements(
        &self,
        elements: &[ParsedHtmlElement],
        source_allocator: &mut HtmlSourceAllocator,
        builder: &mut MarkdownElementBuilder,
        markdown_end: usize,
        cx: &mut App,
    ) {
        for element in elements {
            self.render_html_element(element, source_allocator, builder, markdown_end, cx);
        }
    }

    fn render_html_element(
        &self,
        element: &ParsedHtmlElement,
        source_allocator: &mut HtmlSourceAllocator,
        builder: &mut MarkdownElementBuilder,
        markdown_end: usize,
        cx: &mut App,
    ) {
        let Some(source_range) = element.source_range() else {
            return;
        };

        match element {
            ParsedHtmlElement::Paragraph(paragraph) => {
                self.push_markdown_paragraph(
                    builder,
                    &source_range,
                    markdown_end,
                    paragraph.text_align,
                );
                self.render_html_paragraph(
                    &paragraph.contents,
                    source_allocator,
                    builder,
                    cx,
                    markdown_end,
                );
                self.pop_markdown_paragraph(builder);
            }
            ParsedHtmlElement::Heading(heading) => {
                self.push_markdown_heading(
                    builder,
                    heading.level,
                    &heading.source_range,
                    markdown_end,
                    heading.text_align,
                );
                self.render_html_paragraph(
                    &heading.contents,
                    source_allocator,
                    builder,
                    cx,
                    markdown_end,
                );
                self.pop_markdown_heading(builder);
            }
            ParsedHtmlElement::List(list) => {
                self.render_html_list(list, source_allocator, builder, markdown_end, cx);
            }
            ParsedHtmlElement::BlockQuote(block_quote) => {
                self.push_markdown_block_quote(
                    builder,
                    None,
                    &block_quote.source_range,
                    markdown_end,
                );
                self.render_html_elements(
                    &block_quote.children,
                    source_allocator,
                    builder,
                    markdown_end,
                    cx,
                );
                self.pop_markdown_block_quote(builder);
            }
            ParsedHtmlElement::Table(table) => {
                self.render_html_table(table, source_allocator, builder, markdown_end, cx);
            }
            ParsedHtmlElement::Image(image) => {
                self.render_html_image(image, builder);
            }
        }
    }

    fn render_html_list(
        &self,
        list: &ParsedHtmlList,
        source_allocator: &mut HtmlSourceAllocator,
        builder: &mut MarkdownElementBuilder,
        markdown_end: usize,
        cx: &mut App,
    ) {
        builder.push_div(div().pl_2p5(), &list.source_range, markdown_end);

        for list_item in &list.items {
            let bullet = match list_item.item_type {
                ParsedHtmlListItemType::Ordered(order) => html_list_item_prefix(
                    order as usize,
                    list.ordered,
                    list.depth.saturating_sub(1) as usize,
                ),
                ParsedHtmlListItemType::Unordered => {
                    html_list_item_prefix(1, false, list.depth.saturating_sub(1) as usize)
                }
            };

            self.push_markdown_list_item(
                builder,
                div().child(bullet).into_any_element(),
                &list_item.source_range,
                markdown_end,
            );
            self.render_html_elements(
                &list_item.content,
                source_allocator,
                builder,
                markdown_end,
                cx,
            );
            self.pop_markdown_list_item(builder);
        }

        builder.pop_div();
    }

    fn render_html_paragraph(
        &self,
        paragraph: &HtmlParagraph,
        source_allocator: &mut HtmlSourceAllocator,
        builder: &mut MarkdownElementBuilder,
        cx: &mut App,
        _markdown_end: usize,
    ) {
        for chunk in paragraph {
            match chunk {
                HtmlParagraphChunk::Text(text) => {
                    self.render_html_text(text, source_allocator, builder, cx);
                }
                HtmlParagraphChunk::Image(image) => {
                    self.render_html_image(image, builder);
                }
            }
        }
    }

    fn render_html_text(
        &self,
        text: &ParsedHtmlText,
        source_allocator: &mut HtmlSourceAllocator,
        builder: &mut MarkdownElementBuilder,
        cx: &mut App,
    ) {
        let text_contents = text.contents.as_ref();
        if text_contents.is_empty() {
            return;
        }

        let allocated_range = source_allocator.allocate(text_contents.len());
        let allocated_len = allocated_range.end.saturating_sub(allocated_range.start);

        let mut boundaries = vec![0, text_contents.len()];
        for (range, _) in &text.highlights {
            boundaries.push(range.start);
            boundaries.push(range.end);
        }
        for (range, _) in &text.links {
            boundaries.push(range.start);
            boundaries.push(range.end);
        }
        boundaries.sort_unstable();
        boundaries.dedup();

        for segment in boundaries.windows(2) {
            let start = segment[0];
            let end = segment[1];
            if start >= end {
                continue;
            }

            let source_start = allocated_range.start + start.min(allocated_len);
            let source_end = allocated_range.start + end.min(allocated_len);
            if source_start >= source_end {
                continue;
            }

            let mut refinement = TextStyleRefinement::default();
            let mut has_refinement = false;

            for (highlight_range, style) in &text.highlights {
                if highlight_range.start < end && highlight_range.end > start {
                    apply_html_highlight_style(&mut refinement, style);
                    has_refinement = true;
                }
            }

            let link = text.links.iter().find_map(|(link_range, link)| {
                if link_range.start < end && link_range.end > start {
                    Some(link.clone())
                } else {
                    None
                }
            });

            if let Some(link) = link.as_ref() {
                builder.push_link(link.clone(), source_start..source_end);
                let link_style = self
                    .style
                    .link_callback
                    .as_ref()
                    .and_then(|callback| callback(link.as_ref(), cx))
                    .unwrap_or_else(|| self.style.link.clone());
                builder.push_text_style(link_style);
            }

            if has_refinement {
                builder.push_text_style(refinement);
            }

            builder.push_text(&text_contents[start..end], source_start..source_end);

            if has_refinement {
                builder.pop_text_style();
            }

            if link.is_some() {
                builder.pop_text_style();
            }
        }
    }

    fn render_html_image(&self, image: &HtmlImage, builder: &mut MarkdownElementBuilder) {
        let Some(source) = self
            .image_resolver
            .as_ref()
            .and_then(|resolve| resolve(image.dest_url.as_ref()))
        else {
            return;
        };

        self.push_markdown_image(
            builder,
            &image.source_range,
            source,
            image.dest_url.clone(),
            image.alt_text.clone(),
            image.width,
            image.height,
        );
    }
}

fn apply_html_highlight_style(refinement: &mut TextStyleRefinement, style: &HtmlHighlightStyle) {
    if style.weight != FontWeight::default() {
        refinement.font_weight = Some(style.weight);
    }

    if style.oblique {
        refinement.font_style = Some(FontStyle::Oblique);
    } else if style.italic {
        refinement.font_style = Some(FontStyle::Italic);
    }

    if style.underline {
        refinement.underline = Some(UnderlineStyle {
            thickness: px(1.),
            color: None,
            ..Default::default()
        });
    }

    if style.strikethrough {
        refinement.strikethrough = Some(StrikethroughStyle {
            thickness: px(1.),
            color: None,
        });
    }
}

fn html_list_item_prefix(order: usize, ordered: bool, depth: usize) -> String {
    let index = order.saturating_sub(1);
    const NUMBERED_PREFIXES_1: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const NUMBERED_PREFIXES_2: &str = "abcdefghijklmnopqrstuvwxyz";
    const BULLETS: [&str; 5] = ["•", "◦", "▪", "‣", "⁃"];

    if ordered {
        match depth {
            0 => format!("{}. ", order),
            1 => format!(
                "{}. ",
                NUMBERED_PREFIXES_1
                    .chars()
                    .nth(index % NUMBERED_PREFIXES_1.len())
                    .unwrap()
            ),
            _ => format!(
                "{}. ",
                NUMBERED_PREFIXES_2
                    .chars()
                    .nth(index % NUMBERED_PREFIXES_2.len())
                    .unwrap()
            ),
        }
    } else {
        let depth = depth.min(BULLETS.len() - 1);
        format!("{} ", BULLETS[depth])
    }
}

#[cfg(test)]
#[path = "html_rendering/tests.rs"]
mod tests;

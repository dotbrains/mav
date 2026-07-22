use super::super::*;
use super::MarkdownElement;
use super::layout::{apply_heading_style, image_fallback_element};

impl MarkdownElement {
    pub(super) fn push_markdown_code_span(
        &self,
        builder: &mut MarkdownElementBuilder,
        text: &str,
        range: Range<usize>,
        cx: &App,
    ) {
        let link_url = if builder.code_block_stack.is_empty()
            && builder.link_depth == 0
            && !self.style.prevent_mouse_interaction
        {
            self.code_span_link
                .as_ref()
                .and_then(|callback| callback(text, cx))
        } else {
            None
        };

        if let Some(url) = link_url {
            builder.push_link(url.clone(), range.clone());
            let link_style = self
                .style
                .link_callback
                .as_ref()
                .and_then(|callback| callback(url.as_ref(), cx))
                .unwrap_or_else(|| self.style.link.clone());
            builder.push_text_style(self.style.inline_code.clone());
            builder.push_text_style(link_style);
            builder.push_text(text, range);
            builder.pop_text_style();
            builder.pop_text_style();
        } else {
            let mut code_style = self.style.inline_code.clone();
            if builder.link_depth > 0 {
                code_style.color = self.style.link.color.or(code_style.color);
            }
            builder.push_text_style(code_style);
            builder.push_text(text, range);
            builder.pop_text_style();
        }
    }

    pub(super) fn push_markdown_image(
        &self,
        builder: &mut MarkdownElementBuilder,
        range: &Range<usize>,
        source: ImageSource,
        dest_url: SharedString,
        alt_text: Option<SharedString>,
        width: Option<DefiniteLength>,
        height: Option<DefiniteLength>,
    ) {
        let image_element = div().min_w_0().child(
            img(source)
                .id(("markdown-image", range.start))
                .min_w_0()
                .max_w_full()
                .rounded_md()
                .mr_1()
                .mb_1()
                .when_some(height, |this, height| this.h(height))
                .when_some(width, |this, width| this.w(width))
                .with_fallback(move || image_fallback_element(dest_url.clone(), alt_text.clone())),
        );

        builder.push_image_child(image_element);
    }

    pub(super) fn push_markdown_paragraph(
        &self,
        builder: &mut MarkdownElementBuilder,
        range: &Range<usize>,
        markdown_end: usize,
        text_align_override: Option<TextAlign>,
    ) {
        let align = text_align_override.unwrap_or(self.style.base_text_style.text_align);
        let mut paragraph = div().when(!self.style.height_is_multiple_of_line_height, |el| {
            el.mb_2().line_height(rems(1.3))
        });

        paragraph = match align {
            TextAlign::Center => paragraph.text_center(),
            TextAlign::Left => paragraph.text_left(),
            TextAlign::Right => paragraph.text_right(),
        };

        builder.push_text_style(TextStyleRefinement {
            text_align: Some(align),
            ..Default::default()
        });
        builder.push_div(paragraph, range, markdown_end);
    }

    pub(super) fn pop_markdown_paragraph(&self, builder: &mut MarkdownElementBuilder) {
        builder.pop_div();
        builder.pop_text_style();
    }

    pub(super) fn push_markdown_heading(
        &self,
        builder: &mut MarkdownElementBuilder,
        level: pulldown_cmark::HeadingLevel,
        range: &Range<usize>,
        markdown_end: usize,
        text_align_override: Option<TextAlign>,
    ) {
        let align = text_align_override.unwrap_or(self.style.base_text_style.text_align);
        let mut heading = div().mt_4().mb_2();
        heading = apply_heading_style(
            heading,
            level,
            self.style.heading_level_styles.as_ref(),
            self.style.heading_border_color,
        );

        heading = match align {
            TextAlign::Center => heading.text_center(),
            TextAlign::Left => heading.text_left(),
            TextAlign::Right => heading.text_right(),
        };

        let mut heading_style = self.style.heading.clone();
        let heading_text_style = heading_style.text_style().clone();
        heading.style().refine(&heading_style);

        builder.push_text_style(TextStyleRefinement {
            text_align: Some(align),
            ..heading_text_style
        });
        builder.push_div(heading, range, markdown_end);
    }

    pub(super) fn pop_markdown_heading(&self, builder: &mut MarkdownElementBuilder) {
        builder.pop_div();
        builder.pop_text_style();
    }

    pub(super) fn push_markdown_block_quote(
        &self,
        builder: &mut MarkdownElementBuilder,
        kind: Option<pulldown_cmark::BlockQuoteKind>,
        range: &Range<usize>,
        markdown_end: usize,
    ) {
        let border_color = self
            .style
            .block_quote_kind_colors
            .for_kind(kind, self.style.block_quote_border_color);

        let header = kind.map(|kind| {
            let (icon_name, label) = match kind {
                BlockQuoteKind::Note => (IconName::Info, "Note"),
                BlockQuoteKind::Tip => (IconName::Sparkle, "Tip"),
                BlockQuoteKind::Important => (IconName::Chat, "Important"),
                BlockQuoteKind::Warning => (IconName::Warning, "Warning"),
                BlockQuoteKind::Caution => (IconName::Stop, "Caution"),
            };
            h_flex()
                .gap_1()
                .items_center()
                .mb_1()
                .child(
                    Icon::new(icon_name)
                        .size(IconSize::Small)
                        .color(Color::Custom(border_color)),
                )
                .child(
                    Label::new(label)
                        .color(Color::Custom(border_color))
                        .weight(FontWeight::BOLD),
                )
                .into_any_element()
        });

        let block_div = div().pl_4().mb_2().border_l_4().border_color(border_color);
        let block_div = match header {
            Some(header) => block_div.child(header),
            None => block_div,
        };

        builder.push_text_style(self.style.block_quote.clone());
        builder.push_div(block_div, range, markdown_end);
    }

    pub(super) fn pop_markdown_block_quote(&self, builder: &mut MarkdownElementBuilder) {
        builder.pop_div();
        builder.pop_text_style();
    }

    pub(super) fn push_metadata_block(
        &self,
        builder: &mut MarkdownElementBuilder,
        source: &str,
        metadata_block: &ParsedMetadataBlock,
        markdown_end: usize,
        cx: &App,
    ) {
        let content_range = &metadata_block.content_range;
        if let Some(rows) = metadata_block.rows.as_deref() {
            builder.push_div(
                div()
                    .grid()
                    .grid_cols(2)
                    .w_full()
                    .mb_2()
                    .border_1()
                    .border_color(cx.theme().colors().border)
                    .rounded_sm()
                    .overflow_hidden(),
                content_range,
                markdown_end,
            );

            for (row_index, row) in rows.iter().enumerate() {
                self.push_metadata_cell(
                    builder,
                    source,
                    row.key.clone(),
                    content_range,
                    markdown_end,
                    MetadataCellStyle {
                        row_index,
                        is_key: true,
                    },
                    cx,
                );
                self.push_metadata_cell(
                    builder,
                    source,
                    row.value.clone(),
                    content_range,
                    markdown_end,
                    MetadataCellStyle {
                        row_index,
                        is_key: false,
                    },
                    cx,
                );
            }

            builder.pop_div();
        } else {
            let mut metadata_block = div().w_full().rounded_md();
            metadata_block.style().refine(&self.style.code_block);
            builder.push_text_style(self.style.code_block.text.to_owned());
            builder.push_code_block(None);
            builder.push_div(metadata_block, content_range, markdown_end);
            builder.push_text(&source[content_range.clone()], content_range.clone());
            builder.trim_trailing_newline();
            builder.pop_div();
            builder.pop_code_block();
            builder.pop_text_style();
        }
    }

    pub(super) fn push_metadata_cell(
        &self,
        builder: &mut MarkdownElementBuilder,
        source: &str,
        text_range: Range<usize>,
        block_range: &Range<usize>,
        markdown_end: usize,
        cell_style: MetadataCellStyle,
        cx: &App,
    ) {
        builder.push_div(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .px_2()
                .py_1()
                .border_color(cx.theme().colors().border)
                .when(cell_style.row_index > 0, |this| this.border_t_1())
                .when(!cell_style.is_key, |this| this.border_l_1())
                .when(cell_style.is_key, |this| {
                    this.bg(cx.theme().colors().panel_background)
                }),
            block_range,
            markdown_end,
        );

        let text_style = if cell_style.is_key {
            TextStyleRefinement {
                color: Some(cx.theme().colors().text_muted),
                font_weight: Some(FontWeight::SEMIBOLD),
                ..Default::default()
            }
        } else {
            TextStyleRefinement::default()
        };
        builder.push_text_style(text_style);
        builder.push_text(&source[text_range.clone()], text_range);
        builder.pop_text_style();
        builder.pop_div();
    }

    pub(super) fn push_markdown_list_item(
        &self,
        builder: &mut MarkdownElementBuilder,
        bullet: AnyElement,
        range: &Range<usize>,
        markdown_end: usize,
    ) {
        builder.push_div(
            div()
                .when(!self.style.height_is_multiple_of_line_height, |el| {
                    el.mb_1().gap_1().line_height(rems(1.3))
                })
                .h_flex()
                .items_start()
                .child(bullet),
            range,
            markdown_end,
        );
        // Without `w_0`, text doesn't wrap to the width of the container.
        builder.push_div(div().flex_1().w_0(), range, markdown_end);
    }

    pub(super) fn pop_markdown_list_item(&self, builder: &mut MarkdownElementBuilder) {
        builder.pop_div();
        builder.pop_div();
    }
}

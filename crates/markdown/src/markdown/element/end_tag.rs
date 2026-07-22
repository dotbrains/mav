use super::super::*;
use super::MarkdownElement;
use super::layout::{render_copy_code_block_button, render_wrap_code_block_button};

impl MarkdownElement {
    pub(super) fn render_end_tag(
        &self,
        tag: &MarkdownTagEnd,
        builder: &mut MarkdownElementBuilder,
        parsed_markdown: &ParsedMarkdown,
        range: &Range<usize>,
        current_img_block_range: &mut Option<Range<usize>>,
        cx: &mut App,
    ) {
        match tag {
            MarkdownTagEnd::Image => {
                current_img_block_range.take();
            }
            MarkdownTagEnd::Paragraph => {
                self.pop_markdown_paragraph(builder);
            }
            MarkdownTagEnd::Heading(_) => {
                self.pop_markdown_heading(builder);
            }
            MarkdownTagEnd::BlockQuote(_kind) => {
                self.pop_markdown_block_quote(builder);
            }
            MarkdownTagEnd::CodeBlock => {
                builder.trim_trailing_newline();

                builder.pop_div();
                builder.pop_code_block();
                builder.pop_text_style();

                if let CodeBlockRenderer::Default {
                    copy_button_visibility,
                    wrap_button_visibility,
                    ..
                } = &self.code_block_renderer
                    && (*copy_button_visibility != CopyButtonVisibility::Hidden
                        || *wrap_button_visibility != WrapButtonVisibility::Hidden)
                {
                    let copy_button_visibility = *copy_button_visibility;
                    let wrap_button_visibility = *wrap_button_visibility;
                    builder.modify_current_div(|el| {
                        let content_range = parser::extract_code_block_content_range(
                            &parsed_markdown.source()[range.clone()],
                        );
                        let content_range =
                            content_range.start + range.start..content_range.end + range.start;

                        let code = parsed_markdown.source()[content_range].to_string();

                        let any_hover = copy_button_visibility
                            == CopyButtonVisibility::VisibleOnHover
                            || wrap_button_visibility == WrapButtonVisibility::VisibleOnHover;
                        let any_always = copy_button_visibility
                            == CopyButtonVisibility::AlwaysVisible
                            || wrap_button_visibility == WrapButtonVisibility::AlwaysVisible;
                        let use_hover = any_hover && !any_always;

                        let button_row = h_flex()
                            .gap_0p5()
                            .absolute()
                            .bg(cx.theme().colors().editor_background)
                            .when_else(
                                use_hover,
                                |this| this.top_1().right_1().visible_on_hover("code_block"),
                                |this| this.top_1p5().right_1p5(),
                            )
                            .when(
                                wrap_button_visibility != WrapButtonVisibility::Hidden,
                                |this| {
                                    let is_wrapped =
                                        self.markdown.read(cx).is_code_block_wrapped(range.start);

                                    this.child(render_wrap_code_block_button(
                                        range.start,
                                        is_wrapped,
                                        self.markdown.clone(),
                                    ))
                                },
                            )
                            .when(
                                copy_button_visibility != CopyButtonVisibility::Hidden,
                                |this| {
                                    this.child(render_copy_code_block_button(
                                        range.end,
                                        code,
                                        self.markdown.clone(),
                                    ))
                                },
                            );

                        el.child(button_row)
                    });
                }

                // Pop the parent container.
                builder.pop_div();
            }
            MarkdownTagEnd::HtmlBlock => builder.pop_div(),
            MarkdownTagEnd::List(_) => {
                builder.pop_list();
                builder.pop_div();
            }
            MarkdownTagEnd::Item => {
                self.pop_markdown_list_item(builder);
            }
            MarkdownTagEnd::Emphasis => builder.pop_text_style(),
            MarkdownTagEnd::Strong => builder.pop_text_style(),
            MarkdownTagEnd::Strikethrough => builder.pop_text_style(),
            MarkdownTagEnd::Link => {
                if builder.code_block_stack.is_empty() {
                    builder.link_depth = builder.link_depth.saturating_sub(1);
                    builder.pop_text_style()
                }
            }
            MarkdownTagEnd::Table => {
                builder.pop_div();
                builder.table.end();
            }
            MarkdownTagEnd::TableHead => {
                builder.pop_text_style();
                builder.table.end_head();
            }
            MarkdownTagEnd::TableRow => {
                builder.table.end_row();
            }
            MarkdownTagEnd::TableCell => {
                builder.replace_pending_checkbox(self.on_checkbox_toggle.clone());
                builder.pop_div();
                builder.pop_div();
                builder.pop_text_style();
                builder.table.end_cell();
            }
            MarkdownTagEnd::FootnoteDefinition => {
                builder.pop_div();
                builder.pop_div();
            }
            MarkdownTagEnd::MetadataBlock(_) => {}
            _ => log::debug!("unsupported markdown tag end: {:?}", tag),
        }
    }
}

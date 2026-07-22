use super::super::*;
use super::MarkdownElement;
use super::layout::collect_image_alt_text;
use crate::builder::alignment_to_text_align;

impl MarkdownElement {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_start_tag(
        &self,
        tag: &MarkdownTag,
        builder: &mut MarkdownElementBuilder,
        parsed_markdown: &ParsedMarkdown,
        images: &HashMap<usize, Arc<Image>>,
        index: usize,
        range: &Range<usize>,
        markdown_end: usize,
        render_mermaid_diagrams: bool,
        mermaid_state: &MermaidState,
        current_img_block_range: &mut Option<Range<usize>>,
        handled_html_block: &mut bool,
        rendered_mermaid_block: &mut bool,
        rendered_metadata_block: &mut bool,
        code_block_ids: &mut HashSet<usize>,
        window: &mut Window,
        cx: &mut App,
    ) {
        match tag {
            MarkdownTag::Image { dest_url, .. } => {
                let alt_text = collect_image_alt_text(
                    &parsed_markdown.events[index..],
                    &parsed_markdown.source,
                );
                if let Some(image) = images.get(&range.start) {
                    *current_img_block_range = Some(range.clone());
                    self.push_markdown_image(
                        builder,
                        range,
                        image.clone().into(),
                        dest_url.clone(),
                        alt_text,
                        None,
                        None,
                    );
                } else if let Some(source) = self
                    .image_resolver
                    .as_ref()
                    .and_then(|resolve| resolve(dest_url.as_ref()))
                {
                    *current_img_block_range = Some(range.clone());
                    self.push_markdown_image(
                        builder,
                        range,
                        source,
                        dest_url.clone(),
                        alt_text,
                        None,
                        None,
                    );
                }
            }
            MarkdownTag::Paragraph => {
                let text_align_override = builder
                    .table
                    .current_cell_alignment()
                    .and_then(alignment_to_text_align);
                self.push_markdown_paragraph(builder, range, markdown_end, text_align_override);
            }
            MarkdownTag::Heading { level, .. } => {
                let text_align_override = builder
                    .table
                    .current_cell_alignment()
                    .and_then(alignment_to_text_align);
                self.push_markdown_heading(
                    builder,
                    *level,
                    range,
                    markdown_end,
                    text_align_override,
                );
            }
            MarkdownTag::BlockQuote(kind) => {
                self.push_markdown_block_quote(builder, *kind, range, markdown_end);
            }
            MarkdownTag::CodeBlock { kind, .. } => {
                if render_mermaid_diagrams
                    && let Some(mermaid_diagram) =
                        parsed_markdown.mermaid_diagrams.get(&range.start)
                {
                    let showing_code = self.markdown.read(cx).is_mermaid_showing_code(range.start);
                    let copy_button_visibility = match &self.code_block_renderer {
                        CodeBlockRenderer::Default {
                            copy_button_visibility,
                            ..
                        } => *copy_button_visibility,
                        _ => CopyButtonVisibility::VisibleOnHover,
                    };
                    builder.push_sourced_element(
                        mermaid_diagram.content_range.clone(),
                        render_mermaid_diagram(
                            mermaid_diagram,
                            &mermaid_state,
                            &self.style,
                            self.markdown.clone(),
                            range.start,
                            showing_code,
                            copy_button_visibility,
                        ),
                    );
                    *rendered_mermaid_block = true;
                    return;
                }

                let language = match kind {
                    CodeBlockKind::Fenced => None,
                    CodeBlockKind::FencedLang(language) => {
                        parsed_markdown.languages_by_name.get(language).cloned()
                    }
                    CodeBlockKind::FencedSrc(path_range) => parsed_markdown
                        .languages_by_path
                        .get(&path_range.path)
                        .cloned(),
                    _ => None,
                };

                let is_indented = matches!(kind, CodeBlockKind::Indented);
                let scroll_handle = if self.style.code_block_overflow_x_scroll {
                    self.markdown.update(cx, |markdown, _| {
                        markdown.code_block_scroll_handle(range.start)
                    })
                } else {
                    None
                };
                if scroll_handle.is_some() {
                    code_block_ids.insert(range.start);
                }

                match (&self.code_block_renderer, is_indented) {
                    (CodeBlockRenderer::Default { .. }, _) | (_, true) => {
                        // This is a parent container that we can position the copy button inside.
                        let parent_container = div().group("code_block").relative().w_full();

                        let mut parent_container: AnyDiv =
                            if let Some(scroll_handle) = scroll_handle.as_ref() {
                                let scrollbars = Scrollbars::new(ScrollAxes::Horizontal)
                                    .id(("markdown-code-block-scrollbar", range.start))
                                    .tracked_scroll_handle(scroll_handle)
                                    .with_track_along(
                                        ScrollAxes::Horizontal,
                                        cx.theme().colors().editor_background,
                                    )
                                    .notify_content();

                                parent_container
                                    .rounded_lg()
                                    .custom_scrollbars(scrollbars, window, cx)
                                    .into()
                            } else {
                                parent_container.into()
                            };

                        if let CodeBlockRenderer::Default { border: true, .. } =
                            &self.code_block_renderer
                        {
                            parent_container = parent_container
                                .rounded_md()
                                .border_1()
                                .border_color(cx.theme().colors().border_variant);
                        }

                        parent_container.style().refine(&self.style.code_block);
                        builder.push_div(parent_container, range, markdown_end);

                        let code_block = div().id(("code-block", range.start)).rounded_lg().map(
                            |mut code_block| {
                                if let Some(scroll_handle) = scroll_handle.as_ref() {
                                    code_block.style().restrict_scroll_to_axis = Some(true);
                                    code_block
                                        .flex()
                                        .overflow_x_scroll()
                                        .track_scroll(scroll_handle)
                                } else {
                                    code_block.w_full()
                                }
                            },
                        );

                        builder.push_text_style(self.style.code_block.text.to_owned());
                        builder.push_code_block(language);
                        builder.push_div(code_block, range, markdown_end);
                    }
                    (CodeBlockRenderer::Custom { .. }, _) => {}
                }
            }
            MarkdownTag::HtmlBlock => {
                builder.push_div(div(), range, markdown_end);
                if let Some(block) = parsed_markdown.html_blocks.get(&range.start) {
                    self.render_html_block(block, builder, markdown_end, cx);
                    *handled_html_block = true;
                }
            }
            MarkdownTag::List(bullet_index) => {
                builder.push_list(*bullet_index);
                builder.push_div(div().pl_2p5(), range, markdown_end);
            }
            MarkdownTag::Item => {
                let bullet = if let Some((task_range, MarkdownEvent::TaskListMarker(checked))) =
                    parsed_markdown.events.get(index.saturating_add(1))
                {
                    let source = &parsed_markdown.source()[range.clone()];
                    let checked = *checked;
                    let toggle_state = if checked {
                        ToggleState::Selected
                    } else {
                        ToggleState::Unselected
                    };

                    let checkbox =
                        Checkbox::new(ElementId::Name(source.to_string().into()), toggle_state)
                            .fill();

                    if let Some(on_toggle) = self.on_checkbox_toggle.clone() {
                        let task_source_range = task_range.clone();
                        checkbox
                            .on_click(move |_state, window, cx| {
                                on_toggle(task_source_range.clone(), !checked, window, cx);
                            })
                            .into_any_element()
                    } else {
                        checkbox.visualization_only(true).into_any_element()
                    }
                } else if let Some(bullet_index) = builder.next_bullet_index() {
                    div().child(format!("{}.", bullet_index)).into_any_element()
                } else {
                    div().child("•").into_any_element()
                };
                self.push_markdown_list_item(builder, bullet, range, markdown_end);
            }
            MarkdownTag::Emphasis => builder.push_text_style(TextStyleRefinement {
                font_style: Some(FontStyle::Italic),
                ..Default::default()
            }),
            MarkdownTag::Strong => builder.push_text_style(TextStyleRefinement {
                font_weight: Some(FontWeight::BOLD),
                color: Some(cx.theme().colors().text),
                ..Default::default()
            }),
            MarkdownTag::Strikethrough => builder.push_text_style(TextStyleRefinement {
                strikethrough: Some(StrikethroughStyle {
                    thickness: px(1.),
                    color: None,
                }),
                ..Default::default()
            }),
            MarkdownTag::Link { dest_url, .. } => {
                if builder.code_block_stack.is_empty() {
                    builder.link_depth += 1;
                    builder.push_link(dest_url.clone(), range.clone());
                    let style = self
                        .style
                        .link_callback
                        .as_ref()
                        .and_then(|callback| callback(dest_url, cx))
                        .unwrap_or_else(|| self.style.link.clone());
                    builder.push_text_style(style)
                }
            }
            MarkdownTag::FootnoteDefinition(label) => {
                if !builder.rendered_footnote_separator {
                    builder.rendered_footnote_separator = true;
                    builder.push_div(
                        div()
                            .border_t_1()
                            .mt_2()
                            .border_color(self.style.rule_color),
                        range,
                        markdown_end,
                    );
                    builder.pop_div();
                }
                builder.push_div(
                    div()
                        .pt_1()
                        .mb_1()
                        .line_height(rems(1.3))
                        .text_size(rems(0.85))
                        .h_flex()
                        .items_start()
                        .gap_2()
                        .child(div().text_size(rems(0.85)).child(format!("{}.", label))),
                    range,
                    markdown_end,
                );
                builder.push_div(div().flex_1().w_0(), range, markdown_end);
            }
            MarkdownTag::MetadataBlock(_) => {
                if let Some(metadata_block) = parsed_markdown.metadata_blocks.get(&range.start) {
                    self.push_metadata_block(
                        builder,
                        &parsed_markdown.source,
                        metadata_block,
                        markdown_end,
                        cx,
                    );
                    *rendered_metadata_block = true;
                }
            }
            MarkdownTag::Table(alignments) => {
                builder.table.start(alignments.clone());

                let column_count = alignments.len();
                builder.push_div(
                    div()
                        .id(("table", range.start))
                        .grid()
                        .grid_cols(column_count as u16)
                        .when(self.style.table_columns_min_size, |this| {
                            this.grid_cols_min_content(column_count as u16)
                        })
                        .when(!self.style.table_columns_min_size, |this| {
                            this.grid_cols(column_count as u16)
                        })
                        .w_full()
                        .mb_2()
                        .border(px(1.5))
                        .border_color(cx.theme().colors().border)
                        .rounded_sm()
                        .overflow_hidden(),
                    range,
                    markdown_end,
                );
            }
            MarkdownTag::TableHead => {
                builder.table.start_head();
                builder.push_text_style(TextStyleRefinement {
                    font_weight: Some(FontWeight::SEMIBOLD),
                    ..Default::default()
                });
            }
            MarkdownTag::TableRow => {
                builder.table.start_row();
            }
            MarkdownTag::TableCell => {
                let is_header = builder.table.in_head;
                let row_index = builder.table.row_index;
                let col_index = builder.table.col_index;
                let alignment = builder.table.current_cell_alignment();
                let text_align = alignment
                    .and_then(alignment_to_text_align)
                    .unwrap_or(self.style.base_text_style.text_align);

                let mut cell_div = div()
                    .flex()
                    .flex_col()
                    .h_full()
                    .when(col_index > 0, |this| this.border_l_1())
                    .when(row_index > 0, |this| this.border_t_1())
                    .border_color(cx.theme().colors().border)
                    .px_1()
                    .py_0p5()
                    .when(is_header, |this| {
                        this.bg(cx.theme().colors().title_bar_background)
                    })
                    .when(!is_header && row_index % 2 == 1, |this| {
                        this.bg(cx.theme().colors().panel_background)
                    });

                cell_div = match alignment {
                    Some(Alignment::Center) => cell_div.items_center(),
                    Some(Alignment::Right) => cell_div.items_end(),
                    _ => cell_div,
                };

                builder.push_text_style(TextStyleRefinement {
                    text_align: Some(text_align),
                    ..Default::default()
                });
                builder.push_div(cell_div, range, markdown_end);
                builder.push_div(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .w_full()
                        .justify_center()
                        .text_align(text_align),
                    range,
                    markdown_end,
                );
            }
            _ => log::debug!("unsupported markdown tag {:?}", tag),
        }
    }
}

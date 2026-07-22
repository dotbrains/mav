use super::super::*;
use super::MarkdownElement;
use super::layout::{
    collect_image_alt_text, render_copy_code_block_button, render_wrap_code_block_button,
};

impl Styled for MarkdownElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style.container_style
    }
}

impl Element for MarkdownElement {
    type RequestLayoutState = RenderedMarkdown;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let mut builder = MarkdownElementBuilder::new(
            &self.style.container_style,
            self.style.base_text_style.clone(),
            self.style.syntax.clone(),
        );
        let (parsed_markdown, images, active_root_block, render_mermaid_diagrams, mermaid_state) = {
            let markdown = self.markdown.read(cx);
            (
                markdown.parsed_markdown.clone(),
                markdown.images_by_source_offset.clone(),
                markdown.active_root_block,
                markdown.options.render_mermaid_diagrams,
                markdown.mermaid_state.clone(),
            )
        };
        let markdown_end = if let Some(last) = parsed_markdown.events.last() {
            last.0.end
        } else {
            0
        };
        let mut code_block_ids = HashSet::default();

        let mut current_img_block_range: Option<Range<usize>> = None;
        let mut handled_html_block = false;
        let mut rendered_mermaid_block = false;
        let mut rendered_metadata_block = false;
        for (index, (range, event)) in parsed_markdown.events.iter().enumerate() {
            // Skip alt text for images that rendered
            if let Some(current_img_block_range) = &current_img_block_range
                && current_img_block_range.end > range.end
            {
                continue;
            }

            if handled_html_block {
                if let MarkdownEvent::End(MarkdownTagEnd::HtmlBlock) = event {
                    handled_html_block = false;
                } else {
                    continue;
                }
            }

            if rendered_mermaid_block {
                if matches!(event, MarkdownEvent::End(MarkdownTagEnd::CodeBlock)) {
                    rendered_mermaid_block = false;
                }
                continue;
            }

            if rendered_metadata_block {
                if matches!(event, MarkdownEvent::End(MarkdownTagEnd::MetadataBlock(_))) {
                    rendered_metadata_block = false;
                }
                continue;
            }

            match event {
                MarkdownEvent::RootStart => {
                    if self.show_root_block_markers {
                        builder.push_root_block(range, markdown_end);
                    }
                }
                MarkdownEvent::RootEnd(root_block_index) => {
                    if self.show_root_block_markers {
                        builder.pop_root_block(
                            active_root_block == Some(*root_block_index),
                            cx.theme().colors().border,
                            cx.theme().colors().border_variant,
                        );
                    }
                }
                MarkdownEvent::Start(tag) => self.render_start_tag(
                    tag,
                    &mut builder,
                    &parsed_markdown,
                    &images,
                    index,
                    range,
                    markdown_end,
                    render_mermaid_diagrams,
                    &mermaid_state,
                    &mut current_img_block_range,
                    &mut handled_html_block,
                    &mut rendered_mermaid_block,
                    &mut rendered_metadata_block,
                    &mut code_block_ids,
                    window,
                    cx,
                ),
                MarkdownEvent::End(tag) => self.render_end_tag(
                    tag,
                    &mut builder,
                    &parsed_markdown,
                    range,
                    &mut current_img_block_range,
                    cx,
                ),
                MarkdownEvent::Text => {
                    builder.push_text(&parsed_markdown.source[range.clone()], range.clone());
                }
                MarkdownEvent::SubstitutedText(text) => {
                    builder.push_text(text, range.clone());
                }
                MarkdownEvent::Code => {
                    self.push_markdown_code_span(
                        &mut builder,
                        &parsed_markdown.source[range.clone()],
                        range.clone(),
                        cx,
                    );
                }
                MarkdownEvent::SubstitutedCode(text) => {
                    self.push_markdown_code_span(&mut builder, text, range.clone(), cx);
                }
                MarkdownEvent::Html => {
                    let html = &parsed_markdown.source[range.clone()];
                    if html.starts_with("<!--") {
                        builder.html_comment = true;
                    }
                    if html.trim_end().ends_with("-->") {
                        builder.html_comment = false;
                        continue;
                    }
                    if builder.html_comment {
                        continue;
                    }
                    builder.push_text(html, range.clone());
                }
                MarkdownEvent::InlineHtml => {
                    let html = &parsed_markdown.source[range.clone()];
                    if let Some(code) = html
                        .strip_prefix("<code>")
                        .and_then(|html| html.strip_suffix("</code>"))
                    {
                        let code_start = range.start + "<code>".len();
                        self.push_markdown_code_span(
                            &mut builder,
                            code,
                            code_start..code_start + code.len(),
                            cx,
                        );
                        continue;
                    }
                    if html.starts_with("<code>") {
                        builder.push_text_style(self.style.inline_code.clone());
                        continue;
                    }
                    if html.trim_end().starts_with("</code>") {
                        builder.pop_text_style();
                        continue;
                    }
                    builder.push_text(&parsed_markdown.source[range.clone()], range.clone());
                }
                MarkdownEvent::Rule => {
                    builder.push_div(
                        div()
                            .border_b_1()
                            .my_2()
                            .border_color(self.style.rule_color),
                        range,
                        markdown_end,
                    );
                    builder.pop_div()
                }
                MarkdownEvent::SoftBreak if !self.style.soft_break_as_hard_break => {
                    builder.push_soft_break(range.clone());
                }
                MarkdownEvent::SoftBreak | MarkdownEvent::HardBreak => {
                    builder.push_line_break(range.clone());
                }
                MarkdownEvent::TaskListMarker(_) => {
                    // handled inside the `MarkdownTag::Item` case
                }
                MarkdownEvent::FootnoteReference(label) => {
                    builder.push_footnote_ref(label.clone(), range.clone());
                    builder.push_text_style(self.style.link.clone());
                    builder.push_text(&format!("[{label}]"), range.clone());
                    builder.pop_text_style();
                }
            }
        }
        if self.style.code_block_overflow_x_scroll {
            let code_block_ids = code_block_ids;
            self.markdown.update(cx, move |markdown, _| {
                markdown.retain_code_block_scroll_handles(&code_block_ids);
            });
        } else {
            self.markdown
                .update(cx, |markdown, _| markdown.clear_code_block_scroll_handles());
        }
        let mut rendered_markdown = builder.build();
        let child_layout_id = rendered_markdown.element.request_layout(window, cx);
        let layout_id = window.request_layout(gpui::Style::default(), [child_layout_id], cx);
        (layout_id, rendered_markdown)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        rendered_markdown: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let focus_handle = self.markdown.read(cx).focus_handle.clone();
        window.set_focus_handle(&focus_handle, cx);
        window.set_view_id(self.markdown.entity_id());

        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        rendered_markdown.element.prepaint(window, cx);
        self.autoscroll(&rendered_markdown.text, window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        rendered_markdown: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut context = KeyContext::default();
        context.add("Markdown");
        window.set_key_context(context);
        window.on_action(std::any::TypeId::of::<crate::Copy>(), {
            let entity = self.markdown.clone();
            let text = rendered_markdown.text.clone();
            move |_, phase, window, cx| {
                let text = text.clone();
                if phase == DispatchPhase::Bubble {
                    entity.update(cx, move |this, cx| this.copy(&text, window, cx))
                }
            }
        });
        window.on_action(std::any::TypeId::of::<crate::CopyAsMarkdown>(), {
            let entity = self.markdown.clone();
            move |_, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    entity.update(cx, move |this, cx| this.copy_as_markdown(window, cx))
                }
            }
        });

        self.paint_mouse_listeners(hitbox, &rendered_markdown.text, window, cx);
        rendered_markdown.element.paint(window, cx);
        self.paint_search_highlights(&rendered_markdown.text, window, cx);
        self.paint_selection(&rendered_markdown.text, window, cx);
    }
}

pub(super) fn collect_image_alt_text(
    events_from_image_start: &[(Range<usize>, MarkdownEvent)],
    source: &str,
) -> Option<SharedString> {
    let mut alt_text = String::new();
    for (range, event) in events_from_image_start.iter().skip(1) {
        match event {
            MarkdownEvent::End(MarkdownTagEnd::Image) => break,
            MarkdownEvent::Text => alt_text.push_str(&source[range.clone()]),
            _ => {}
        }
    }
    if alt_text.is_empty() {
        None
    } else {
        Some(alt_text.into())
    }
}

pub(super) fn image_fallback_element(
    dest_url: SharedString,
    alt_text: Option<SharedString>,
) -> AnyElement {
    let link_label = alt_text
        .filter(|alt| !alt.is_empty())
        .unwrap_or_else(|| dest_url.clone());

    let label = format!("Failed to Load: {link_label}");

    div()
        .id("image-fallback")
        .cursor_pointer()
        .min_w_0()
        .child(Label::new(label).color(Color::Warning).underline())
        .tooltip(Tooltip::text(
            "Image failed to load. Open `mav: log` for more details.",
        ))
        .on_click(move |_, _, cx| cx.open_url(&dest_url))
        .into_any_element()
}

pub(super) fn apply_heading_style(
    mut heading: Div,
    level: pulldown_cmark::HeadingLevel,
    custom_styles: Option<&HeadingLevelStyles>,
    border_color: Option<Hsla>,
) -> Div {
    heading = match level {
        pulldown_cmark::HeadingLevel::H1 => heading.text_3xl(),
        pulldown_cmark::HeadingLevel::H2 => heading.text_2xl(),
        pulldown_cmark::HeadingLevel::H3 => heading.text_xl(),
        pulldown_cmark::HeadingLevel::H4 => heading.text_lg(),
        pulldown_cmark::HeadingLevel::H5 => heading.text_base(),
        pulldown_cmark::HeadingLevel::H6 => heading.text_sm(),
    };

    heading = match level {
        pulldown_cmark::HeadingLevel::H1 => heading,
        _ => heading.mt_6(),
    };

    if let Some(border_color) = border_color
        && matches!(
            level,
            pulldown_cmark::HeadingLevel::H1
                | pulldown_cmark::HeadingLevel::H2
                | pulldown_cmark::HeadingLevel::H3
        )
    {
        heading = heading.pb_1().border_b_1().border_color(border_color);
    }

    if let Some(styles) = custom_styles {
        let style_opt = match level {
            pulldown_cmark::HeadingLevel::H1 => &styles.h1,
            pulldown_cmark::HeadingLevel::H2 => &styles.h2,
            pulldown_cmark::HeadingLevel::H3 => &styles.h3,
            pulldown_cmark::HeadingLevel::H4 => &styles.h4,
            pulldown_cmark::HeadingLevel::H5 => &styles.h5,
            pulldown_cmark::HeadingLevel::H6 => &styles.h6,
        };

        if let Some(style) = style_opt {
            heading.style().text = style.clone();
        }
    }

    heading
}

pub(super) fn render_wrap_code_block_button(
    id: usize,
    is_wrapped: bool,
    markdown: Entity<Markdown>,
) -> impl IntoElement {
    let (icon, tooltip) = if is_wrapped {
        (IconName::TextUnwrap, "Unwrap Content")
    } else {
        (IconName::TextWrap, "Wrap Content")
    };
    let button_id = ElementId::NamedChild(
        Arc::new(ElementId::from(("wrap-code-block", markdown.entity_id()))),
        id.to_string().into(),
    );

    IconButton::new(button_id, icon)
        .icon_size(IconSize::Small)
        .icon_color(Color::Muted)
        .tooltip(Tooltip::text(tooltip))
        .on_click(move |_event, _window, cx| {
            markdown.update(cx, |markdown, cx| {
                markdown.toggle_code_block_wrap(id);
                cx.notify();
            });
        })
}

pub(super) fn render_copy_code_block_button(
    id: usize,
    code: String,
    markdown: Entity<Markdown>,
) -> impl IntoElement {
    let id = ElementId::NamedChild(
        Arc::new(ElementId::from((
            "copy-markdown-code",
            markdown.entity_id(),
        ))),
        id.to_string().into(),
    );

    CopyButton::new(id.clone(), code.clone()).custom_on_click({
        let markdown = markdown;
        move |_window, cx| {
            let id = id.clone();
            markdown.update(cx, |this, cx| {
                this.copied_code_blocks.insert(id.clone());

                cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));

                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_secs(2)).await;

                    cx.update(|cx| {
                        this.update(cx, |this, cx| {
                            this.copied_code_blocks.remove(&id);
                            cx.notify();
                        })
                    })
                    .ok();
                })
                .detach();
            });
        }
    })
}

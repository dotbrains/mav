use super::*;

impl ThreadView {
    pub(super) fn render_embedded_resource_output(
        &self,
        resource: &acp::EmbeddedResource,
        markdown: Option<Entity<Markdown>>,
        entry_ix: usize,
        context_ix: usize,
        tool_call: &ToolCall,
        card_layout: bool,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        if let Some(markdown) = markdown {
            return self.render_markdown_output(
                markdown,
                entry_ix,
                context_ix,
                tool_call,
                card_layout,
                window,
                cx,
            );
        }

        let uri = match &resource.resource {
            acp::EmbeddedResourceResource::BlobResourceContents(blob) => blob.uri.as_str(),
            acp::EmbeddedResourceResource::TextResourceContents(text) => text.uri.as_str(),
            _ => "",
        };

        v_flex()
            .gap_1()
            .map(|this| {
                if card_layout {
                    this.p_2().when(context_ix > 0, |this| {
                        this.border_t_1()
                            .border_color(self.tool_card_border_color(cx))
                    })
                } else {
                    this.ml(rems(0.4))
                        .px_3p5()
                        .border_l_1()
                        .border_color(self.tool_card_border_color(cx))
                }
            })
            .when(!uri.is_empty(), |this| {
                this.child(
                    Label::new(uri.to_string())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_resource_link(
        &self,
        resource_link: &acp::ResourceLink,
        cx: &Context<Self>,
    ) -> AnyElement {
        let uri: SharedString = resource_link.uri.clone().into();
        let is_file = resource_link.uri.strip_prefix("file://");

        let Some(project) = self.project.upgrade() else {
            return Empty.into_any_element();
        };

        let label: SharedString = if let Some(abs_path) = is_file {
            let (abs_path, fragment) = abs_path
                .split_once('#')
                .map_or((abs_path, None), |(path, fragment)| (path, Some(fragment)));

            let path_label = if let Some(project_path) = project
                .read(cx)
                .project_path_for_absolute_path(&Path::new(abs_path), cx)
                && let Some(worktree) = project
                    .read(cx)
                    .worktree_for_id(project_path.worktree_id, cx)
            {
                worktree
                    .read(cx)
                    .full_path(&project_path.path)
                    .to_string_lossy()
                    .to_string()
            } else {
                abs_path.to_string()
            };

            match fragment {
                Some(fragment) => format!("{path_label}#{fragment}").into(),
                None => path_label.into(),
            }
        } else {
            uri.clone()
        };

        let button_id = SharedString::from(format!("item-{}", uri));

        div()
            .ml(rems(0.4))
            .pl_2p5()
            .border_l_1()
            .border_color(self.tool_card_border_color(cx))
            .overflow_hidden()
            .child(
                Button::new(button_id, label)
                    .label_size(LabelSize::Small)
                    .color(Color::Muted)
                    .truncate(true)
                    .when(is_file.is_none(), |this| {
                        this.end_icon(
                            Icon::new(IconName::ArrowUpRight)
                                .size(IconSize::XSmall)
                                .color(Color::Muted),
                        )
                    })
                    .on_click(cx.listener({
                        let workspace = self.workspace.clone();
                        move |_, _, window, cx: &mut Context<Self>| {
                            open_link(uri.clone(), &workspace, window, cx);
                        }
                    })),
            )
            .into_any_element()
    }

    pub(super) fn render_diff_editor(
        &self,
        entry_ix: usize,
        diff: &Entity<acp_thread::Diff>,
        tool_call: &ToolCall,
        has_failed: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tool_progress = matches!(
            &tool_call.status,
            ToolCallStatus::InProgress | ToolCallStatus::Pending
        );

        let revealed_diff_editor = if let Some(entry) =
            self.entry_view_state.read(cx).entry(entry_ix)
            && let Some(editor) = entry.editor_for_diff(diff)
            && diff.read(cx).has_revealed_range(cx)
        {
            Some(editor)
        } else {
            None
        };

        let show_top_border = !has_failed || revealed_diff_editor.is_some();

        v_flex()
            .h_full()
            .when(show_top_border, |this| {
                this.border_t_1()
                    .when(has_failed, |this| this.border_dashed())
                    .border_color(self.tool_card_border_color(cx))
            })
            .child(if let Some(editor) = revealed_diff_editor {
                editor.into_any_element()
            } else if tool_progress && self.as_native_connection(cx).is_some() {
                self.render_diff_loading(cx)
            } else {
                Empty.into_any()
            })
            .into_any()
    }

    pub(super) fn render_markdown_output(
        &self,
        markdown: Entity<Markdown>,
        entry_ix: usize,
        context_ix: usize,
        tool_call: &ToolCall,
        card_layout: bool,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let markdown_style = MarkdownStyle::themed(MarkdownFont::Agent, window, cx);
        let output = self
            .render_numbered_read_file_output(
                markdown.clone(),
                entry_ix,
                context_ix,
                tool_call,
                markdown_style.clone(),
                cx,
            )
            .unwrap_or_else(|| {
                self.render_markdown(markdown, markdown_style, cx)
                    .into_any()
            });

        v_flex()
            .gap_2()
            .map(|this| {
                if card_layout {
                    this.p_2().when(context_ix > 0, |this| {
                        this.border_t_1()
                            .border_color(self.tool_card_border_color(cx))
                    })
                } else {
                    this.ml(rems(0.4))
                        .px_3p5()
                        .border_l_1()
                        .border_color(self.tool_card_border_color(cx))
                }
            })
            .text_xs()
            .text_color(cx.theme().colors().text_muted)
            .child(output)
            .into_any_element()
    }

    fn render_numbered_read_file_output(
        &self,
        markdown: Entity<Markdown>,
        entry_ix: usize,
        context_ix: usize,
        tool_call: &ToolCall,
        markdown_style: MarkdownStyle,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let is_read_file = tool_call
            .tool_name
            .as_ref()
            .is_some_and(|tool_name| tool_name.as_ref() == "read_file");
        if !is_read_file {
            return None;
        }

        let markdown = markdown.read(cx);
        let parsed = parse_cat_numbered_markdown_code_block(markdown.source())?;
        let language = markdown.first_code_block_language();
        Some(render_cat_numbered_code_block(
            parsed,
            language,
            markdown_style,
            format!("copy-read-file-output-{entry_ix}-{context_ix}"),
            cx,
        ))
    }

    pub(super) fn render_image_output(
        &self,
        entry_ix: usize,
        image: Arc<gpui::Image>,
        location: Option<acp::ToolCallLocation>,
        card_layout: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        v_flex()
            .gap_2()
            .map(|this| {
                if card_layout {
                    this
                } else {
                    this.ml(rems(0.4))
                        .px_3p5()
                        .border_l_1()
                        .border_color(self.tool_card_border_color(cx))
                }
            })
            .when_some(location, |this, _loc| {
                this.child(
                    h_flex().w_full().justify_end().child(
                        Button::new(("go-to-file", entry_ix), "Go to File")
                            .label_size(LabelSize::Small)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_tool_call_location(entry_ix, 0, window, cx);
                            })),
                    ),
                )
            })
            .child(
                img(image)
                    .max_w_96()
                    .max_h_96()
                    .object_fit(ObjectFit::ScaleDown),
            )
            .into_any_element()
    }
}

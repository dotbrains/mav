use super::*;
use crate::hover_popover::{hover_markdown_style, open_markdown_url};

impl CompletionsMenu {
    pub(super) fn render(
        &self,
        style: &EditorStyle,
        max_height_in_lines: u32,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> AnyElement {
        let show_completion_documentation = self.show_completion_documentation;
        let editor_settings = EditorSettings::get_global(cx);
        let completion_detail_alignment = editor_settings.completion_detail_alignment;
        let completion_menu_item_kind = editor_settings.completion_menu_item_kind;
        let widest_completion_ix = if self.display_options.dynamic_width {
            let completions = self.completions.borrow();
            let widest_completion_ix = self
                .entries
                .borrow()
                .iter()
                .enumerate()
                .filter_map(|(ix, entry)| entry.as_match().map(|m| (ix, m)))
                .max_by_key(|(_, mat)| {
                    let completion = &completions[mat.candidate_id];
                    let documentation = &completion.documentation;

                    let mut len = completion.label.text.chars().count();
                    if show_completion_documentation {
                        match documentation {
                            Some(CompletionDocumentation::SingleLine(text)) => {
                                len += text.chars().count();
                            }
                            Some(CompletionDocumentation::SingleLineAndMultiLinePlainText {
                                single_line,
                                ..
                            }) => {
                                len += single_line.chars().count();
                            }
                            _ => {}
                        }
                    }

                    len
                })
                .map(|(ix, _)| ix);
            drop(completions);
            widest_completion_ix
        } else {
            None
        };

        let selected_item = self.selected_item;
        let completions = self.completions.clone();
        let entries = self.entries.clone();
        let last_rendered_range = self.last_rendered_range.clone();
        let style = style.clone();
        let list = uniform_list(
            "completions",
            self.entries.borrow().len(),
            cx.processor(move |_editor, range: Range<usize>, _window, cx| {
                last_rendered_range.borrow_mut().replace(range.clone());
                let start_ix = range.start;
                let completions_guard = completions.borrow_mut();

                entries.borrow()[range]
                    .iter()
                    .enumerate()
                    .map(|(ix, entry)| {
                        let item_ix = start_ix + ix;

                        let Some(mat) = entry.as_match() else {
                            return match entry {
                                CompletionMenuEntry::GroupHeader(label) => div()
                                    .child(ListSubHeader::new(label.clone()).inset(true))
                                    .into_any_element(),
                                CompletionMenuEntry::Divider => h_flex()
                                    .flex_1()
                                    .size_full()
                                    .child(Divider::horizontal())
                                    .into_any_element(),
                                CompletionMenuEntry::Match(_) => unreachable!(),
                            };
                        };

                        let completion = &completions_guard[mat.candidate_id];
                        let documentation = if show_completion_documentation {
                            &completion.documentation
                        } else {
                            &None
                        };

                        let filter_start = completion.label.filter_range.start;

                        let highlights = gpui::combine_highlights(
                            mat.ranges().map(|range| {
                                (
                                    filter_start + range.start..filter_start + range.end,
                                    FontWeight::BOLD.into(),
                                )
                            }),
                            styled_runs_for_code_label(
                                &completion.label,
                                &style.syntax,
                                &style.local_player,
                            )
                            .map(|(range, mut highlight)| {
                                // Ignore font weight for syntax highlighting, as we'll use it
                                // for fuzzy matches.
                                highlight.font_weight = None;
                                if completion
                                    .source
                                    .lsp_completion(false)
                                    .and_then(|lsp_completion| {
                                        match (lsp_completion.deprecated, &lsp_completion.tags) {
                                            (Some(true), _) => Some(true),
                                            (_, Some(tags)) => {
                                                Some(tags.contains(&CompletionItemTag::DEPRECATED))
                                            }
                                            _ => None,
                                        }
                                    })
                                    .unwrap_or(false)
                                {
                                    highlight.strikethrough = Some(StrikethroughStyle {
                                        thickness: 1.0.into(),
                                        ..Default::default()
                                    });
                                    highlight.color = Some(cx.theme().colors().text_muted);
                                }

                                (range, highlight)
                            }),
                        );

                        let highlights: Vec<_> = highlights.collect();

                        let filter_range = &completion.label.filter_range;
                        let full_text = &completion.label.text;

                        let main_text: String = full_text[filter_range.clone()].to_string();
                        let main_highlights: Vec<_> = highlights
                            .iter()
                            .filter_map(|(range, highlight)| {
                                if range.end <= filter_range.start
                                    || range.start >= filter_range.end
                                {
                                    return None;
                                }
                                let clamped_start =
                                    range.start.max(filter_range.start) - filter_range.start;
                                let clamped_end =
                                    range.end.min(filter_range.end) - filter_range.start;
                                Some((clamped_start..clamped_end, (*highlight)))
                            })
                            .collect();
                        let main_label = StyledText::new(main_text)
                            .with_default_highlights(&style.text, main_highlights);

                        let suffix_text: String = full_text[filter_range.end..].to_string();
                        let suffix_highlights: Vec<_> = highlights
                            .iter()
                            .filter_map(|(range, highlight)| {
                                if range.end <= filter_range.end {
                                    return None;
                                }
                                let shifted_start = range.start.saturating_sub(filter_range.end);
                                let shifted_end = range.end - filter_range.end;
                                Some((shifted_start..shifted_end, (*highlight)))
                            })
                            .collect();
                        let suffix_label = if !suffix_text.is_empty() {
                            Some(
                                StyledText::new(suffix_text)
                                    .with_default_highlights(&style.text, suffix_highlights),
                            )
                        } else {
                            None
                        };

                        let left_aligned_suffix =
                            matches!(completion_detail_alignment, CompletionDetailAlignment::Left);

                        let right_aligned_suffix = matches!(
                            completion_detail_alignment,
                            CompletionDetailAlignment::Right,
                        );

                        let documentation_label = match documentation {
                            Some(CompletionDocumentation::SingleLine(text))
                            | Some(CompletionDocumentation::SingleLineAndMultiLinePlainText {
                                single_line: text,
                                ..
                            }) => {
                                if text.trim().is_empty() {
                                    None
                                } else {
                                    Some(
                                        Label::new(text.trim().to_string())
                                            .ml_4()
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    )
                                }
                            }
                            _ => None,
                        };

                        let icon_or_color_slot = completion
                            .color()
                            .map(|color| {
                                div()
                                    .flex_shrink_0()
                                    .size_3p5()
                                    .rounded_xs()
                                    .bg(color)
                                    .into_any_element()
                            })
                            .or_else(|| {
                                completion.icon_path.as_ref().map(|path| {
                                    Icon::from_path(path)
                                        .size(IconSize::XSmall)
                                        .color(
                                            completion
                                                .icon_color
                                                .map_or(Color::Muted, Color::Custom),
                                        )
                                        .into_any_element()
                                })
                            });

                        let kind_letter_slot = match completion_menu_item_kind {
                            CompletionMenuItemKind::Off => None,
                            CompletionMenuItemKind::Symbol => Some(render_completion_kind_letter(
                                completion.kind(),
                                item_ix,
                                &style,
                            )),
                        };

                        let start_slot = match (kind_letter_slot, icon_or_color_slot) {
                            (Some(letter), Some(icon_or_color)) => Some(
                                h_flex()
                                    .gap_0p5()
                                    .child(letter)
                                    .child(icon_or_color)
                                    .into_any_element(),
                            ),
                            (Some(letter), None) => Some(letter),
                            (None, slot) => slot,
                        };

                        div()
                            .min_w(COMPLETION_MENU_MIN_WIDTH)
                            .max_w(COMPLETION_MENU_MAX_WIDTH)
                            .child(
                                ListItem::new(mat.candidate_id)
                                    .inset(true)
                                    .toggle_state(item_ix == selected_item)
                                    .on_click(cx.listener(move |editor, _event, window, cx| {
                                        cx.stop_propagation();
                                        if let Some(task) = editor.confirm_completion(
                                            &ConfirmCompletion {
                                                item_ix: Some(item_ix),
                                            },
                                            window,
                                            cx,
                                        ) {
                                            task.detach_and_log_err(cx)
                                        }
                                    }))
                                    .start_slot::<AnyElement>(start_slot)
                                    .child(
                                        h_flex()
                                            .min_w_0()
                                            .w_full()
                                            .when(left_aligned_suffix, |this| this.justify_start())
                                            .when(right_aligned_suffix, |this| {
                                                this.justify_between()
                                            })
                                            .child(
                                                div()
                                                    .flex_none()
                                                    .whitespace_nowrap()
                                                    .child(main_label),
                                            )
                                            .when_some(suffix_label, |this, suffix| {
                                                this.child(div().truncate().child(suffix))
                                            }),
                                    )
                                    .end_slot::<Label>(documentation_label),
                            )
                            .into_any_element()
                    })
                    .collect()
            }),
        )
        .occlude()
        .max_h(max_height_in_lines as f32 * window.line_height())
        .track_scroll(&self.scroll_handle)
        .with_sizing_behavior(ListSizingBehavior::Infer)
        .map(|this| {
            if self.display_options.dynamic_width {
                this.with_width_from_item(widest_completion_ix)
            } else {
                this.w(rems(34.))
            }
        });

        Popover::new()
            .child(
                div().child(list).custom_scrollbars(
                    Scrollbars::for_settings::<CompletionMenuScrollBarSetting>()
                        .show_along(ScrollAxes::Vertical)
                        .tracked_scroll_handle(&self.scroll_handle),
                    window,
                    cx,
                ),
            )
            .into_any_element()
    }

    pub(super) fn render_aside(
        &mut self,
        max_size: Size<Pixels>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Option<AnyElement> {
        if !self.show_completion_documentation {
            return None;
        }

        let entries = self.entries.borrow();
        let Some(mat) = entries[self.selected_item].as_match() else {
            return None;
        };
        let completions = self.completions.borrow();
        let multiline_docs = match completions[mat.candidate_id].documentation.as_ref() {
            Some(CompletionDocumentation::MultiLinePlainText(text)) => div().child(text.clone()),
            Some(CompletionDocumentation::SingleLineAndMultiLinePlainText {
                plain_text: Some(text),
                ..
            }) => div().child(text.clone()),
            Some(CompletionDocumentation::MultiLineMarkdown(source)) if !source.is_empty() => {
                let Some((false, markdown)) = self.get_or_create_markdown(
                    mat.candidate_id,
                    Some(source),
                    true,
                    &completions,
                    cx,
                ) else {
                    return None;
                };
                Self::render_markdown(markdown, window, cx)
            }
            None => {
                // Handle the case where documentation hasn't yet been resolved but there's a
                // `new_text` match in the cache.
                //
                // TODO: It's inconsistent that documentation caching based on matching `new_text`
                // only works for markdown. Consider generally caching the results of resolving
                // completions.
                let Some((false, markdown)) =
                    self.get_or_create_markdown(mat.candidate_id, None, true, &completions, cx)
                else {
                    return None;
                };
                Self::render_markdown(markdown, window, cx)
            }
            Some(CompletionDocumentation::MultiLineMarkdown(_)) => return None,
            Some(CompletionDocumentation::SingleLine(_)) => return None,
            Some(CompletionDocumentation::Undocumented) => return None,
            Some(CompletionDocumentation::SingleLineAndMultiLinePlainText {
                plain_text: None,
                ..
            }) => {
                return None;
            }
        };

        Some(
            Popover::new()
                .child(
                    multiline_docs
                        .id("multiline_docs")
                        .px(MENU_ASIDE_X_PADDING / 2.)
                        .max_w(max_size.width)
                        .max_h(max_size.height)
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll_handle_aside)
                        .occlude(),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn render_markdown(
        markdown: Entity<Markdown>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Div {
        let editor = cx.weak_entity();
        div().child(
            MarkdownElement::new(markdown, hover_markdown_style(window, cx))
                .code_block_renderer(markdown::CodeBlockRenderer::Default {
                    copy_button_visibility: CopyButtonVisibility::Hidden,
                    wrap_button_visibility: markdown::WrapButtonVisibility::Hidden,
                    border: false,
                })
                .on_url_click(move |link, window, cx| {
                    open_markdown_url(
                        editor
                            .read_with(cx, |editor, _| editor.workspace())
                            .ok()
                            .flatten(),
                        link,
                        window,
                        cx,
                    )
                }),
        )
    }
}

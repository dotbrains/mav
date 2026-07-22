use super::*;

impl Editor {
    pub(super) fn dismiss_overlays_without_comments(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.buffer.read(cx).snapshot(cx);
        // First, compute which overlays have comments (to avoid borrow issues with retain)
        let overlays_with_comments: Vec<bool> = self
            .diff_review_overlays
            .iter()
            .map(|overlay| self.hunk_comment_count(&overlay.hunk_key, &snapshot) > 0)
            .collect();

        // Now collect block IDs to remove and retain overlays
        let mut block_ids_to_remove = HashSet::default();
        let mut index = 0;
        self.diff_review_overlays.retain(|overlay| {
            let has_comments = overlays_with_comments[index];
            index += 1;
            if !has_comments {
                block_ids_to_remove.insert(overlay.block_id);
            }
            has_comments
        });

        if !block_ids_to_remove.is_empty() {
            self.remove_blocks(block_ids_to_remove, None, cx);
            cx.notify();
        }
    }

    /// Refreshes the diff review overlay block to update its height and render function.
    /// Uses resize_blocks and replace_blocks to avoid visual flicker from remove+insert.
    pub(super) fn refresh_diff_review_overlay_height(
        &mut self,
        hunk_key: &DiffHunkKey,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Extract all needed data from overlay first to avoid borrow conflicts
        let snapshot = self.buffer.read(cx).snapshot(cx);
        let (comments_expanded, block_id, prompt_editor) = {
            let Some(overlay) = self
                .diff_review_overlays
                .iter()
                .find(|overlay| Self::hunk_keys_match(&overlay.hunk_key, hunk_key, &snapshot))
            else {
                return;
            };

            (
                overlay.comments_expanded,
                overlay.block_id,
                overlay.prompt_editor.clone(),
            )
        };

        // Calculate new height
        let snapshot = self.buffer.read(cx).snapshot(cx);
        let new_height = self.calculate_overlay_height(hunk_key, comments_expanded, &snapshot);

        // Update the block height using resize_blocks (avoids flicker)
        let mut heights = HashMap::default();
        heights.insert(block_id, new_height);
        self.resize_blocks(heights, None, cx);

        // Update the render function using replace_blocks (avoids flicker)
        let hunk_key_for_render = hunk_key.clone();
        let editor_handle = cx.entity().downgrade();
        let render: Arc<dyn Fn(&mut BlockContext) -> AnyElement + Send + Sync> =
            Arc::new(move |cx| {
                Self::render_diff_review_overlay(
                    &prompt_editor,
                    &hunk_key_for_render,
                    &editor_handle,
                    cx,
                )
            });

        let mut renderers = HashMap::default();
        renderers.insert(block_id, render);
        self.replace_blocks(renderers, None, cx);
    }

    /// Compares two DiffHunkKeys for equality by resolving their anchors.
    pub(super) fn hunk_keys_match(
        a: &DiffHunkKey,
        b: &DiffHunkKey,
        snapshot: &MultiBufferSnapshot,
    ) -> bool {
        a.file_path == b.file_path
            && a.hunk_start_anchor.to_point(snapshot) == b.hunk_start_anchor.to_point(snapshot)
    }

    pub(super) fn render_diff_review_overlay(
        prompt_editor: &Entity<Editor>,
        hunk_key: &DiffHunkKey,
        editor_handle: &WeakEntity<Editor>,
        cx: &mut BlockContext,
    ) -> AnyElement {
        fn format_line_ranges(ranges: &[(u32, u32)]) -> Option<String> {
            if ranges.is_empty() {
                return None;
            }
            let formatted: Vec<String> = ranges
                .iter()
                .map(|(start, end)| {
                    let start_line = start + 1;
                    let end_line = end + 1;
                    if start_line == end_line {
                        format!("Line {start_line}")
                    } else {
                        format!("Lines {start_line}-{end_line}")
                    }
                })
                .collect();
            // Don't show label for single line in single excerpt
            if ranges.len() == 1 && ranges[0].0 == ranges[0].1 {
                return None;
            }
            Some(formatted.join(" ⋯ "))
        }

        let theme = cx.theme();
        let colors = theme.colors();

        let (comments, comments_expanded, inline_editors, user_avatar_uri, line_ranges) =
            editor_handle
                .upgrade()
                .map(|editor| {
                    let editor = editor.read(cx);
                    let snapshot = editor.buffer().read(cx).snapshot(cx);
                    let comments = editor.comments_for_hunk(hunk_key, &snapshot).to_vec();
                    let (expanded, editors, avatar_uri, line_ranges) = editor
                        .diff_review_overlays
                        .iter()
                        .find(|overlay| {
                            Editor::hunk_keys_match(&overlay.hunk_key, hunk_key, &snapshot)
                        })
                        .map(|o| {
                            let start_point = o.anchor_range.start.to_point(&snapshot);
                            let end_point = o.anchor_range.end.to_point(&snapshot);
                            // Get line ranges per excerpt to detect discontinuities
                            let buffer_ranges =
                                snapshot.range_to_buffer_ranges(start_point..end_point);
                            let ranges: Vec<(u32, u32)> = buffer_ranges
                                .iter()
                                .map(|(buffer_snapshot, range, _)| {
                                    let start = buffer_snapshot.offset_to_point(range.start.0).row;
                                    let end = buffer_snapshot.offset_to_point(range.end.0).row;
                                    (start, end)
                                })
                                .collect();
                            (
                                o.comments_expanded,
                                o.inline_edit_editors.clone(),
                                o.user_avatar_uri.clone(),
                                if ranges.is_empty() {
                                    None
                                } else {
                                    Some(ranges)
                                },
                            )
                        })
                        .unwrap_or((true, HashMap::default(), None, None));
                    (comments, expanded, editors, avatar_uri, line_ranges)
                })
                .unwrap_or((Vec::new(), true, HashMap::default(), None, None));

        let comment_count = comments.len();
        let avatar_size = px(20.);
        let action_icon_size = IconSize::XSmall;

        v_flex()
            .w_full()
            .bg(colors.editor_background)
            .border_b_1()
            .border_color(colors.border)
            .px_2()
            .pb_2()
            .gap_2()
            // Line range indicator (only shown for multi-line selections or multiple excerpts)
            .when_some(line_ranges, |el, ranges| {
                let label = format_line_ranges(&ranges);
                if let Some(label) = label {
                    el.child(
                        h_flex()
                            .w_full()
                            .px_2()
                            .child(Label::new(label).size(LabelSize::Small).color(Color::Muted)),
                    )
                } else {
                    el
                }
            })
            // Top row: editable input with user's avatar
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .bg(colors.surface_background)
                    .child(
                        div()
                            .size(avatar_size)
                            .flex_shrink_0()
                            .rounded_full()
                            .overflow_hidden()
                            .child(if let Some(ref avatar_uri) = user_avatar_uri {
                                Avatar::new(avatar_uri.clone())
                                    .size(avatar_size)
                                    .into_any_element()
                            } else {
                                Icon::new(IconName::Person)
                                    .size(IconSize::Small)
                                    .color(ui::Color::Muted)
                                    .into_any_element()
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .border_1()
                            .border_color(colors.border)
                            .rounded_md()
                            .bg(colors.editor_background)
                            .px_2()
                            .py_1()
                            .child(prompt_editor.clone()),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_1()
                            .child(
                                IconButton::new("diff-review-close", IconName::Close)
                                    .icon_color(ui::Color::Muted)
                                    .icon_size(action_icon_size)
                                    .tooltip(Tooltip::text("Close"))
                                    .on_click(|_, window, cx| {
                                        window
                                            .dispatch_action(Box::new(crate::actions::Cancel), cx);
                                    }),
                            )
                            .child(
                                IconButton::new("diff-review-add", IconName::Return)
                                    .icon_color(ui::Color::Muted)
                                    .icon_size(action_icon_size)
                                    .tooltip(Tooltip::text("Add comment"))
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(
                                            Box::new(crate::actions::SubmitDiffReviewComment),
                                            cx,
                                        );
                                    }),
                            ),
                    ),
            )
            // Expandable comments section (only shown when there are comments)
            .when(comment_count > 0, |el| {
                el.child(Self::render_comments_section(
                    comments,
                    comments_expanded,
                    inline_editors,
                    user_avatar_uri,
                    avatar_size,
                    action_icon_size,
                    colors,
                ))
            })
            .into_any_element()
    }

    pub(super) fn render_comments_section(
        comments: Vec<StoredReviewComment>,
        expanded: bool,
        inline_editors: HashMap<usize, Entity<Editor>>,
        user_avatar_uri: Option<SharedUri>,
        avatar_size: Pixels,
        action_icon_size: IconSize,
        colors: &theme::ThemeColors,
    ) -> impl IntoElement {
        let comment_count = comments.len();

        v_flex()
            .w_full()
            .gap_1()
            // Header with expand/collapse toggle
            .child(
                h_flex()
                    .id("review-comments-header")
                    .w_full()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .rounded_md()
                    .hover(|style| style.bg(colors.ghost_element_hover))
                    .on_click(|_, window: &mut Window, cx| {
                        window.dispatch_action(
                            Box::new(crate::actions::ToggleReviewCommentsExpanded),
                            cx,
                        );
                    })
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(IconSize::Small)
                        .color(ui::Color::Muted),
                    )
                    .child(
                        Label::new(format!(
                            "{} Comment{}",
                            comment_count,
                            if comment_count == 1 { "" } else { "s" }
                        ))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                    ),
            )
            // Comments list (when expanded)
            .when(expanded, |el| {
                el.children(comments.into_iter().map(|comment| {
                    let inline_editor = inline_editors.get(&comment.id).cloned();
                    Self::render_comment_row(
                        comment,
                        inline_editor,
                        user_avatar_uri.clone(),
                        avatar_size,
                        action_icon_size,
                        colors,
                    )
                }))
            })
    }

    pub(super) fn render_comment_row(
        comment: StoredReviewComment,
        inline_editor: Option<Entity<Editor>>,
        user_avatar_uri: Option<SharedUri>,
        avatar_size: Pixels,
        action_icon_size: IconSize,
        colors: &theme::ThemeColors,
    ) -> impl IntoElement {
        let comment_id = comment.id;
        let is_editing = inline_editor.is_some();

        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded_md()
            .bg(colors.surface_background)
            .child(
                div()
                    .size(avatar_size)
                    .flex_shrink_0()
                    .rounded_full()
                    .overflow_hidden()
                    .child(if let Some(ref avatar_uri) = user_avatar_uri {
                        Avatar::new(avatar_uri.clone())
                            .size(avatar_size)
                            .into_any_element()
                    } else {
                        Icon::new(IconName::Person)
                            .size(IconSize::Small)
                            .color(ui::Color::Muted)
                            .into_any_element()
                    }),
            )
            .child(if let Some(editor) = inline_editor {
                // Inline edit mode: show an editable text field
                div()
                    .flex_1()
                    .border_1()
                    .border_color(colors.border)
                    .rounded_md()
                    .bg(colors.editor_background)
                    .px_2()
                    .py_1()
                    .child(editor)
                    .into_any_element()
            } else {
                // Display mode: show the comment text
                div()
                    .flex_1()
                    .text_sm()
                    .text_color(colors.text)
                    .child(comment.comment)
                    .into_any_element()
            })
            .child(if is_editing {
                // Editing mode: show close and confirm buttons
                h_flex()
                    .gap_1()
                    .child(
                        IconButton::new(
                            format!("diff-review-cancel-edit-{comment_id}"),
                            IconName::Close,
                        )
                        .icon_color(ui::Color::Muted)
                        .icon_size(action_icon_size)
                        .tooltip(Tooltip::text("Cancel"))
                        .on_click(move |_, window, cx| {
                            window.dispatch_action(
                                Box::new(crate::actions::CancelEditReviewComment {
                                    id: comment_id,
                                }),
                                cx,
                            );
                        }),
                    )
                    .child(
                        IconButton::new(
                            format!("diff-review-confirm-edit-{comment_id}"),
                            IconName::Return,
                        )
                        .icon_color(ui::Color::Muted)
                        .icon_size(action_icon_size)
                        .tooltip(Tooltip::text("Confirm"))
                        .on_click(move |_, window, cx| {
                            window.dispatch_action(
                                Box::new(crate::actions::ConfirmEditReviewComment {
                                    id: comment_id,
                                }),
                                cx,
                            );
                        }),
                    )
                    .into_any_element()
            } else {
                // Display mode: no action buttons for now (edit/delete not yet implemented)
                gpui::Empty.into_any_element()
            })
    }
}

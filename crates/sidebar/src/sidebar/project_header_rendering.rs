use super::*;

impl Sidebar {
    pub(super) fn render_list_entry(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = self.contents.entries.get(ix) else {
            return div().into_any_element();
        };
        let is_focused = self.focus_handle.is_focused(window);
        // is_selected means the keyboard selector is here.
        let is_selected = is_focused && self.selection == Some(ix);

        let is_group_header_after_first =
            ix > 0 && matches!(entry, ListEntry::ProjectHeader { .. });

        let is_active = self
            .active_entry
            .as_ref()
            .is_some_and(|active| active.matches_entry(entry));

        let rendered = match entry {
            ListEntry::ProjectHeader {
                key,
                label,
                highlight_positions,
                has_running_threads,
                waiting_thread_count,
                has_notifications,
                is_active: is_active_group,
                has_threads,
            } => {
                self.project_header_menu_handles.entry(ix).or_default();
                self.project_header_new_thread_menu_handles
                    .entry(ix)
                    .or_default();

                self.render_project_header(
                    ix,
                    false,
                    key,
                    label,
                    highlight_positions,
                    *has_running_threads,
                    *waiting_thread_count,
                    *has_notifications,
                    *is_active_group,
                    is_selected,
                    *has_threads,
                    // has_active_draft,
                    cx,
                )
            }
            ListEntry::Thread(thread) => {
                let is_active = self
                    .focused_thread_entry(window, cx)
                    .as_ref()
                    .is_some_and(|active| active.matches_entry(entry));
                self.render_thread(ix, thread, is_active, is_selected, cx)
            }
            ListEntry::Terminal(terminal) => {
                self.render_terminal(ix, terminal, is_active, is_selected, cx)
            }
        };

        if is_group_header_after_first {
            v_flex()
                .w_full()
                .border_t_1()
                .border_color(cx.theme().colors().border)
                .child(rendered)
                .into_any_element()
        } else {
            rendered
        }
    }

    pub(super) fn render_remote_project_icon(
        &self,
        ix: usize,
        host: Option<&RemoteConnectionOptions>,
    ) -> Option<AnyElement> {
        let remote_icon_per_type = match host? {
            RemoteConnectionOptions::Wsl(_) => IconName::Linux,
            RemoteConnectionOptions::Docker(_) => IconName::Box,
            _ => IconName::Server,
        };

        Some(
            div()
                .id(format!("remote-project-icon-{}", ix))
                .child(
                    Icon::new(remote_icon_per_type)
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                )
                .tooltip(Tooltip::text("Remote Project"))
                .into_any_element(),
        )
    }

    pub(super) fn render_project_header(
        &self,
        ix: usize,
        is_sticky: bool,
        key: &ProjectGroupKey,
        label: &SharedString,
        highlight_positions: &[usize],
        has_running_threads: bool,
        waiting_thread_count: usize,
        has_notifications: bool,
        is_active: bool,
        is_focused: bool,
        has_threads: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let host = key.host();

        let has_filter = self.has_filter_query(cx);

        let id_prefix = if is_sticky { "sticky-" } else { "" };
        let id = SharedString::from(format!("{id_prefix}project-header-{ix}"));
        let group_name = SharedString::from(format!("{id_prefix}header-group-{ix}"));

        let is_collapsed = self.is_group_collapsed(key, cx);
        let disclosure_icon = if is_collapsed {
            IconName::ChevronRight
        } else {
            IconName::ChevronDown
        };

        let key_for_toggle = key.clone();
        let key_for_focus = key.clone();

        // The fade gradient renders as a visible patch on transparent windows,
        // so truncate the label instead.
        let opaque_window =
            cx.theme().window_background_appearance() == WindowBackgroundAppearance::Opaque;

        let label = if highlight_positions.is_empty() {
            Label::new(label.clone())
                .when(!is_active, |this| this.color(Color::Muted))
                .when(!opaque_window, |this| this.truncate())
                .into_any_element()
        } else {
            HighlightedLabel::new(label.clone(), highlight_positions.to_vec())
                .when(!is_active, |this| this.color(Color::Muted))
                .when(!opaque_window, |this| this.truncate())
                .into_any_element()
        };

        let color = cx.theme().colors();
        let base_bg = color.editor_background;

        let hover_base = color
            .element_active
            .blend(color.element_background.opacity(0.2));
        let hover_solid = base_bg.blend(hover_base);

        let group_name_for_gradient = group_name.clone();
        let gradient_overlay = move || {
            GradientFade::new(base_bg, hover_solid, hover_solid)
                .width(px(92.0))
                .right(px(-2.0))
                .gradient_stop(0.7)
                .when(!has_filter, |this| {
                    this.group_name(group_name_for_gradient.clone())
                })
        };

        let header = h_flex()
            .id(id)
            .group(&group_name)
            .when(!has_filter, |this| this.cursor_pointer())
            .relative()
            .h(Tab::content_height(cx))
            .w_full()
            .pl_2()
            .pr_1p5()
            .justify_between()
            .border_1()
            .map(|this| {
                if is_focused {
                    this.border_color(color.border_focused)
                } else {
                    this.border_color(gpui::transparent_black())
                }
            })
            .when(!has_filter, |this| this.hover(|s| s.bg(hover_solid)))
            .child(
                h_flex()
                    .relative()
                    .min_w_0()
                    .w_full()
                    .gap_1()
                    .child(label)
                    .when_some(
                        self.render_remote_project_icon(ix, host.as_ref()),
                        |this, icon| this.child(icon),
                    )
                    .when(is_collapsed, |this| {
                        this.when(has_running_threads, |this| {
                            this.child(
                                Icon::new(IconName::LoadCircle)
                                    .size(IconSize::XSmall)
                                    .color(Color::Muted)
                                    .with_rotate_animation(2),
                            )
                        })
                        .when(waiting_thread_count > 0, |this| {
                            let tooltip_text = if waiting_thread_count == 1 {
                                "1 thread is waiting for confirmation".to_string()
                            } else {
                                format!(
                                    "{waiting_thread_count} threads are waiting for confirmation",
                                )
                            };
                            this.child(
                                div()
                                    .id(format!("{id_prefix}waiting-indicator-{ix}"))
                                    .child(
                                        Icon::new(IconName::Warning)
                                            .size(IconSize::XSmall)
                                            .color(Color::Warning),
                                    )
                                    .tooltip(Tooltip::text(tooltip_text)),
                            )
                        })
                        .when(
                            has_notifications && !has_running_threads && waiting_thread_count == 0,
                            |this| {
                                this.child(
                                    Icon::new(IconName::Circle)
                                        .size(IconSize::Small)
                                        .color(Color::Accent),
                                )
                            },
                        )
                    })
                    .when(!has_filter, |this| {
                        this.child(
                            div()
                                .when(!is_focused, |this| this.visible_on_hover(&group_name))
                                .child(
                                    Icon::new(disclosure_icon)
                                        .size(IconSize::Small)
                                        .color(Color::Muted),
                                ),
                        )
                    }),
            )
            .children(opaque_window.then(|| gradient_overlay()))
            .child(
                h_flex()
                    .gap_px()
                    .pr_1p5()
                    .children(opaque_window.then(|| gradient_overlay()))
                    .child(self.render_new_thread_button(ix, id_prefix, key, &group_name, cx))
                    .child(self.render_project_header_ellipsis_menu(
                        ix,
                        id_prefix,
                        key,
                        is_active,
                        has_threads,
                        &group_name,
                        cx,
                    ))
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    }),
            )
            .on_mouse_down(gpui::MouseButton::Right, {
                let menu_handle = self
                    .project_header_menu_handles
                    .get(&ix)
                    .cloned()
                    .unwrap_or_default();
                move |_, window, cx| {
                    cx.stop_propagation();
                    menu_handle.toggle(window, cx);
                }
            })
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if event.modifiers().secondary() {
                        this.activate_or_open_workspace_for_group(&key_for_focus, window, cx);
                    } else if !this.has_filter_query(cx) {
                        this.toggle_collapse(&key_for_toggle, window, cx);
                    }
                }),
            )
            .block_mouse_except_scroll();

        if !is_collapsed && !has_threads {
            v_flex()
                .w_full()
                .child(header)
                .child(
                    h_flex()
                        .px_2()
                        .pt_1()
                        .pb_2()
                        .gap(px(7.))
                        .child(Icon::new(IconName::Circle).size(IconSize::Small).color(
                            Color::Custom(cx.theme().colors().icon_placeholder.opacity(0.1)),
                        ))
                        .child(
                            Label::new("No threads yet")
                                .size(LabelSize::Small)
                                .color(Color::Placeholder),
                        ),
                )
                .into_any_element()
        } else {
            header.into_any_element()
        }
    }

    pub(super) fn render_sticky_header(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let scroll_top = self.list_state.logical_scroll_top();

        let &header_idx = self
            .contents
            .project_header_indices
            .iter()
            .rev()
            .find(|&&idx| idx <= scroll_top.item_ix)?;

        let needs_sticky = header_idx < scroll_top.item_ix
            || (header_idx == scroll_top.item_ix && scroll_top.offset_in_item > px(0.));

        if !needs_sticky {
            return None;
        }

        let ListEntry::ProjectHeader {
            key,
            label,
            highlight_positions,
            has_running_threads,
            waiting_thread_count,
            has_notifications,
            is_active,
            has_threads,
        } = self.contents.entries.get(header_idx)?
        else {
            return None;
        };

        let is_focused = self.focus_handle.is_focused(window);
        let is_selected = is_focused && self.selection == Some(header_idx);

        let header_element = self.render_project_header(
            header_idx,
            true,
            key,
            &label,
            &highlight_positions,
            *has_running_threads,
            *waiting_thread_count,
            *has_notifications,
            *is_active,
            is_selected,
            *has_threads,
            cx,
        );

        let top_offset = self
            .contents
            .project_header_indices
            .iter()
            .find(|&&idx| idx > header_idx)
            .and_then(|&next_idx| {
                let bounds = self.list_state.bounds_for_item(next_idx)?;
                let viewport = self.list_state.viewport_bounds();
                let y_in_viewport = bounds.origin.y - viewport.origin.y;
                let header_height = bounds.size.height;
                (y_in_viewport < header_height).then_some(y_in_viewport - header_height)
            })
            .unwrap_or(px(0.));

        let color = cx.theme().colors();
        let background = color.editor_background;

        let element = v_flex()
            .absolute()
            .top(top_offset)
            .left_0()
            .w_full()
            .bg(background)
            .border_b_1()
            .border_color(color.border.opacity(0.5))
            .child(header_element)
            .shadow_sm()
            .into_any_element();

        Some(element)
    }

    pub(super) fn toggle_collapse(
        &mut self,
        project_group_key: &ProjectGroupKey,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_collapsed = self.is_group_collapsed(project_group_key, cx);
        self.set_group_expanded(project_group_key, is_collapsed, cx);
        self.update_entries(cx);
    }
}

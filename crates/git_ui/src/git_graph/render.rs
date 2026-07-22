use super::*;

impl Render for GitGraph {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // This happens when we changed branches, we should refresh our search as well
        if let QueryState::Pending(query) = &mut self.search_state.state {
            let query = std::mem::take(query);
            self.search_state.state = QueryState::Empty;
            self.search(query, cx);
        }
        let (commit_count, is_loading) = self.commit_count_and_loading_state(cx);

        let error = self.get_repository(cx).and_then(|repo| {
            repo.read(cx)
                .get_graph_data(self.log_source.clone(), self.log_order)
                .and_then(|data| data.error.clone())
        });

        let content = if commit_count == 0 {
            let message = if let Some(error) = &error {
                format!("Error loading: {}", error)
            } else if is_loading {
                "Loading".to_string()
            } else {
                "No commits found".to_string()
            };
            let label = Label::new(message)
                .color(Color::Muted)
                .size(LabelSize::Large);
            div()
                .size_full()
                .h_flex()
                .gap_1()
                .items_center()
                .justify_center()
                .child(label)
                .when(is_loading && error.is_none(), |this| {
                    this.child(self.render_loading_spinner(cx))
                })
        } else {
            let is_path_history = matches!(self.log_source, LogSource::Path(_));
            let header_resize_info =
                HeaderResizeInfo::from_redistributable(&self.column_widths, cx);
            let header_context = TableRenderContext::for_column_widths(
                Some(self.column_widths.read(cx).widths_to_render()),
                true,
            );
            let [
                graph_fraction,
                description_fraction,
                date_fraction,
                author_fraction,
                commit_fraction,
            ] = self.preview_column_fractions(window, cx);
            let table_fraction =
                description_fraction + date_fraction + author_fraction + commit_fraction;
            let table_width_config = self.table_column_width_config(window, cx);

            h_flex()
                .size_full()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .size_full()
                        .flex()
                        .flex_col()
                        .child(render_table_header(
                            if !is_path_history {
                                TableRow::from_vec(
                                    vec![
                                        Label::new("Graph")
                                            .color(Color::Muted)
                                            .truncate()
                                            .into_any_element(),
                                        Label::new("Description")
                                            .color(Color::Muted)
                                            .into_any_element(),
                                        Label::new("Date").color(Color::Muted).into_any_element(),
                                        Label::new("Author").color(Color::Muted).into_any_element(),
                                        Label::new("Commit").color(Color::Muted).into_any_element(),
                                    ],
                                    5,
                                )
                            } else {
                                TableRow::from_vec(
                                    vec![
                                        Label::new("Description")
                                            .color(Color::Muted)
                                            .into_any_element(),
                                        Label::new("Date").color(Color::Muted).into_any_element(),
                                        Label::new("Author").color(Color::Muted).into_any_element(),
                                        Label::new("Commit").color(Color::Muted).into_any_element(),
                                    ],
                                    4,
                                )
                            },
                            header_context,
                            Some(header_resize_info),
                            Some(self.column_widths.entity_id()),
                            cx,
                        ))
                        .child({
                            let row_height = Self::row_height(window, cx);
                            let selected_entry_idx = self.selected_entry_idx;
                            let hovered_entry_idx = self.hovered_entry_idx;
                            let context_menu_entry_idx =
                                self.context_menu.as_ref().map(|menu| menu.entry_idx);
                            let weak_self = cx.weak_entity();
                            let focus_handle = self.focus_handle.clone();
                            let table_focus_handle =
                                self.table_interaction_state.read(cx).focus_handle.clone();

                            let graph_canvas = div()
                                .id("graph-canvas")
                                .size_full()
                                .overflow_hidden()
                                .cursor_pointer()
                                .child(
                                    div()
                                        .size_full()
                                        .child(self.render_graph_canvas(window, cx)),
                                )
                                .on_scroll_wheel(cx.listener(Self::handle_graph_scroll))
                                .on_mouse_move(cx.listener(Self::handle_graph_mouse_move))
                                .on_click(cx.listener(Self::handle_graph_click))
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(Self::handle_graph_secondary_mouse_down),
                                )
                                .on_hover(cx.listener(|this, &is_hovered: &bool, _, cx| {
                                    if !is_hovered && this.hovered_entry_idx.is_some() {
                                        this.hovered_entry_idx = None;
                                        cx.notify();
                                    }
                                }));

                            let commits_table = Table::new(4)
                                .interactable(&self.table_interaction_state)
                                .hide_row_borders()
                                .hide_row_hover()
                                .width_config(table_width_config)
                                .map_row(move |(index, row), window, cx| {
                                    let is_selected = selected_entry_idx == Some(index);
                                    let is_hovered = hovered_entry_idx == Some(index);
                                    let is_context_menu_target =
                                        context_menu_entry_idx == Some(index);
                                    let table_focus_handle = table_focus_handle.clone();
                                    let is_focused = focus_handle.is_focused(window)
                                        || table_focus_handle.is_focused(window);
                                    let weak = weak_self.clone();
                                    let weak_for_hover = weak.clone();
                                    let weak_for_context_menu = weak.clone();

                                    let hover_bg = cx.theme().colors().element_hover.opacity(0.6);
                                    let selected_bg = if is_focused {
                                        cx.theme().colors().element_selected
                                    } else {
                                        cx.theme().colors().element_hover
                                    };

                                    row.h(row_height)
                                        .cursor_pointer()
                                        .when(is_selected || is_context_menu_target, |row| {
                                            row.bg(selected_bg)
                                        })
                                        .when(
                                            is_hovered && !is_selected && !is_context_menu_target,
                                            |row| row.bg(hover_bg),
                                        )
                                        .on_hover(move |&is_hovered, _, cx| {
                                            weak_for_hover
                                                .update(cx, |this, cx| {
                                                    if is_hovered {
                                                        if this.hovered_entry_idx != Some(index) {
                                                            this.hovered_entry_idx = Some(index);
                                                            cx.notify();
                                                        }
                                                    } else if this.hovered_entry_idx == Some(index)
                                                    {
                                                        this.hovered_entry_idx = None;
                                                        cx.notify();
                                                    }
                                                })
                                                .ok();
                                        })
                                        .on_click(move |event, window, cx| {
                                            weak.update(cx, |this, cx| {
                                                this.handle_entry_click(
                                                    index,
                                                    event,
                                                    ScrollStrategy::Center,
                                                    Some(&table_focus_handle),
                                                    window,
                                                    cx,
                                                );
                                            })
                                            .ok();
                                        })
                                        .on_mouse_down(
                                            MouseButton::Right,
                                            move |event: &MouseDownEvent, window, cx| {
                                                weak_for_context_menu
                                                    .update(cx, |this, cx| {
                                                        this.handle_entry_secondary_mouse_down(
                                                            index, event, window, cx,
                                                        );
                                                    })
                                                    .ok();
                                            },
                                        )
                                        .into_any_element()
                                })
                                .uniform_list(
                                    "git-graph-commits",
                                    commit_count,
                                    cx.processor(Self::render_table_rows),
                                );

                            bind_redistributable_columns(
                                div()
                                    .relative()
                                    .flex_1()
                                    .w_full()
                                    .overflow_hidden()
                                    .child(
                                        h_flex()
                                            .size_full()
                                            .when(!is_path_history, |this| {
                                                this.child(
                                                    div()
                                                        .w(DefiniteLength::Fraction(graph_fraction))
                                                        .h_full()
                                                        .min_w_0()
                                                        .overflow_hidden()
                                                        .child(graph_canvas),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .tab_index(2)
                                                    .tab_group()
                                                    .tab_stop(false)
                                                    .w(DefiniteLength::Fraction(table_fraction))
                                                    .h_full()
                                                    .min_w_0()
                                                    .child(commits_table),
                                            ),
                                    )
                                    .child(render_redistributable_columns_resize_handles(
                                        &self.column_widths,
                                        window,
                                        cx,
                                    )),
                                self.column_widths.clone(),
                            )
                        }),
                )
                .on_drag_move::<DraggedSplitHandle>(cx.listener(|this, event, window, cx| {
                    this.commit_details_split_state.update(cx, |state, cx| {
                        state.on_drag_move(event, window, cx);
                    });
                }))
                .on_drop::<DraggedSplitHandle>(cx.listener(|this, _event, _window, cx| {
                    this.commit_details_split_state.update(cx, |state, _cx| {
                        state.commit_ratio();
                    });
                }))
                .when(self.selected_entry_idx.is_some(), |this| {
                    this.child(self.render_commit_view_resize_handle(window, cx))
                        .child(self.render_commit_detail_panel(window, cx))
                })
        };

        div()
            .key_context("GitGraph")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .on_action(cx.listener(|this, _: &OpenCommitView, window, cx| {
                this.open_selected_commit_view(window, cx);
            }))
            .on_action(cx.listener(Self::copy_selected_commit_sha))
            .on_action(cx.listener(Self::copy_selected_commit_tag))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search_state
                    .editor
                    .update(cx, |editor, cx| editor.focus_handle(cx).focus(window, cx));
                this.activate_search_editor_if_focused(window, cx);
            }))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::scroll_up))
            .on_action(cx.listener(Self::scroll_down))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::toggle_changed_files_view))
            .on_action(cx.listener(Self::focus_next_tab_stop))
            .on_action(cx.listener(Self::focus_previous_tab_stop))
            .on_action(cx.listener(|this, _: &SelectNextMatch, _window, cx| {
                this.select_next_match(cx);
            }))
            .on_action(cx.listener(|this, _: &SelectPreviousMatch, _window, cx| {
                this.select_previous_match(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleCaseSensitive, _window, cx| {
                this.search_state.case_sensitive = !this.search_state.case_sensitive;
                this.search_state.state.next_state();
                cx.emit(ItemEvent::Edit);
                cx.notify();
            }))
            .child(
                v_flex()
                    .size_full()
                    .child(self.render_search_bar(cx))
                    .child(div().flex_1().child(content)),
            )
            .children(self.context_menu.as_ref().map(|context_menu| {
                deferred(
                    anchored()
                        .position(context_menu.position)
                        .anchor(Anchor::TopLeft)
                        .child(context_menu.menu.clone()),
                )
                .with_priority(1)
            }))
            .on_action(cx.listener(|_, _: &buffer_search::Deploy, window, cx| {
                window.dispatch_action(Box::new(FocusSearch), cx);
                cx.stop_propagation();
            }))
    }
}

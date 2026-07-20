use super::tab_context_menu::{TabContextMenuParams, render_tab_context_menu};
use super::*;

impl Pane {
    pub(super) fn render_tab(
        &self,
        ix: usize,
        item: &dyn ItemHandle,
        detail: usize,
        focus_handle: &FocusHandle,
        window: &mut Window,
        cx: &mut Context<Pane>,
    ) -> impl IntoElement + use<> {
        let is_active = ix == self.active_item_index;
        let is_preview = self
            .preview_item_id
            .map(|id| id == item.item_id())
            .unwrap_or(false);

        let label = item.tab_content(
            TabContentParams {
                detail: Some(detail),
                selected: is_active,
                preview: is_preview,
                deemphasized: !self.has_focus(window, cx),
                max_title_len: None,
                truncate_title_middle: false,
            },
            window,
            cx,
        );

        let item_diagnostic = item
            .project_path(cx)
            .map_or(None, |project_path| self.diagnostics.get(&project_path));

        let tab_icon = item.tab_icon(window, cx);
        let decorated_icon = item_diagnostic.map_or(None, |diagnostic| {
            let icon = match tab_icon.clone() {
                Some(icon) => icon,
                None => return None,
            };

            let knockout_item_color = if is_active {
                cx.theme().colors().tab_active_background
            } else {
                cx.theme().colors().tab_bar_background
            };

            let (icon_decoration, icon_color) = if matches!(diagnostic, &DiagnosticSeverity::ERROR)
            {
                (IconDecorationKind::X, Color::Error)
            } else {
                (IconDecorationKind::Triangle, Color::Warning)
            };

            Some(DecoratedIcon::new(
                icon.size(IconSize::Small).color(Color::Muted),
                Some(
                    IconDecoration::new(icon_decoration, knockout_item_color, cx)
                        .color(icon_color.color(cx))
                        .position(Point {
                            x: px(-2.),
                            y: px(-2.),
                        }),
                ),
            ))
        });

        let icon = if decorated_icon.is_none() {
            match item_diagnostic {
                Some(&DiagnosticSeverity::ERROR) => None,
                Some(&DiagnosticSeverity::WARNING) => None,
                _ => item.tab_icon_element(window, cx).or_else(|| {
                    tab_icon.clone().map(|icon| {
                        icon.color(Color::Muted)
                            .size(IconSize::Small)
                            .into_any_element()
                    })
                }),
            }
        } else {
            None
        };

        let settings = ItemSettings::get_global(cx);
        let close_side = settings.close_position;
        let show_close_button = settings.show_close_button;
        let indicator = render_item_indicator(item.boxed_clone(), cx);
        let tab_tooltip_content = item.tab_tooltip_content(cx);
        let item_id = item.item_id();
        let is_first_item = ix == 0;
        let is_last_item = ix == self.items.len() - 1;
        let is_pinned = self.is_tab_pinned(ix);
        let position_relative_to_active_item = ix.cmp(&self.active_item_index);

        let read_only_toggle = |toggleable: bool| {
            IconButton::new("toggle_read_only", IconName::FileLock)
                .size(ButtonSize::None)
                .shape(IconButtonShape::Square)
                .icon_color(Color::Muted)
                .icon_size(IconSize::Small)
                .disabled(!toggleable)
                .tooltip(move |_, cx| {
                    if toggleable {
                        Tooltip::with_meta(
                            "Unlock File",
                            None,
                            "This will make this file editable",
                            cx,
                        )
                    } else {
                        Tooltip::with_meta("Locked File", None, "This file is read-only", cx)
                    }
                })
                .on_click(cx.listener(move |pane, _, window, cx| {
                    if let Some(item) = pane.item_for_index(ix) {
                        item.toggle_read_only(window, cx);
                    }
                }))
        };

        let has_file_icon = icon.is_some() | decorated_icon.is_some();

        let capability = item.capability(cx);
        let tab = Tab::new(ix)
            .position(if is_first_item {
                TabPosition::First
            } else if is_last_item {
                TabPosition::Last
            } else {
                TabPosition::Middle(position_relative_to_active_item)
            })
            .close_side(match close_side {
                ClosePosition::Left => ui::TabCloseSide::Start,
                ClosePosition::Right => ui::TabCloseSide::End,
            })
            .toggle_state(is_active)
            .on_click(cx.listener({
                let item_handle = item.boxed_clone();
                move |pane: &mut Self, event: &ClickEvent, window, cx| {
                    if event.click_count() > 1 {
                        pane.unpreview_item_if_preview(item_id);
                        let extra_actions = item_handle.tab_extra_context_menu_actions(window, cx);
                        if let Some((_, action)) = extra_actions
                            .into_iter()
                            .find(|(label, _)| label.as_ref() == "Rename")
                        {
                            // Dispatch action directly through the focus handle to avoid
                            // relay_action's intermediate focus step which can interfere
                            // with inline editors.
                            let focus_handle = item_handle.item_focus_handle(cx);
                            focus_handle.dispatch_action(&*action, window, cx);
                            return;
                        }
                    }
                    pane.activate_item(ix, true, true, window, cx)
                }
            }))
            .on_aux_click(
                cx.listener(move |pane: &mut Self, event: &ClickEvent, window, cx| {
                    if !event.is_middle_click() || is_pinned {
                        return;
                    }

                    pane.close_item_by_id(item_id, SaveIntent::Close, window, cx)
                        .detach_and_log_err(cx);
                    cx.stop_propagation();
                }),
            )
            .on_drag(
                DraggedTab {
                    item: item.boxed_clone(),
                    pane: cx.entity(),
                    detail,
                    is_active,
                    ix,
                },
                |tab, _, _, cx| cx.new(|_| tab.clone()),
            )
            .on_drag_move::<DraggedTab>(cx.listener(
                move |this, event: &DragMoveEvent<DraggedTab>, _, cx| {
                    this.handle_dragged_tab_over_tab(ix, event, cx);
                },
            ))
            .on_drag_move::<DraggedSelection>(cx.listener(
                move |this, event: &DragMoveEvent<DraggedSelection>, _, cx| {
                    this.handle_dragged_selection_over_tab(ix, event, cx);
                },
            ))
            .when_some(self.can_drop_predicate.clone(), |this, p| {
                this.can_drop(move |a, window, cx| p(a, window, cx))
            })
            .on_drop(
                cx.listener(move |this, dragged_tab: &DraggedTab, window, cx| {
                    this.clear_drag_drop_target(cx);
                    this.handle_tab_drop(dragged_tab, ix, false, window, cx)
                }),
            )
            .on_drop(
                cx.listener(move |this, selection: &DraggedSelection, window, cx| {
                    this.clear_drag_drop_target(cx);
                    this.handle_dragged_selection_drop(selection, Some(ix), window, cx)
                }),
            )
            .on_drop(cx.listener(move |this, paths, window, cx| {
                this.clear_drag_drop_target(cx);
                this.handle_external_paths_drop(paths, window, cx)
            }))
            .map(|tab| {
                if !cx.has_active_drag() {
                    return tab;
                }

                match self.drag_tab_insertion_target {
                    Some(TabInsertionTarget::Tab {
                        ix: target_ix,
                        side: TabInsertionSide::Left,
                    }) if target_ix == ix => tab.insertion_indicator_left(),
                    Some(TabInsertionTarget::Tab {
                        ix: target_ix,
                        side: TabInsertionSide::Right,
                    }) if target_ix == ix => tab.insertion_indicator_right(),
                    _ => tab,
                }
            })
            .map(|this| {
                let end_slot_action: &'static dyn Action;
                let end_slot_tooltip_text: &'static str;
                let end_slot_control = if is_pinned {
                    end_slot_action = &TogglePinTab;
                    end_slot_tooltip_text = "Unpin Tab";
                    Some(
                        IconButton::new("unpin tab", IconName::Pin)
                            .shape(IconButtonShape::Square)
                            .icon_color(Color::Muted)
                            .size(ButtonSize::None)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(move |pane, _, window, cx| {
                                pane.unpin_tab_at(ix, window, cx);
                            })),
                    )
                } else {
                    end_slot_action = &CloseActiveItem {
                        save_intent: None,
                        close_pinned: false,
                    };
                    end_slot_tooltip_text = item.tab_close_tooltip_text(cx);
                    let close_icon = item.tab_close_icon(cx);
                    match show_close_button {
                        ShowCloseButton::Always => Some(IconButton::new("close tab", close_icon)),
                        ShowCloseButton::Hover => Some(IconButton::new("close tab", close_icon)),
                        ShowCloseButton::Hidden => None,
                    }
                    .map(|button| {
                        button
                            .shape(IconButtonShape::Square)
                            .icon_color(Color::Muted)
                            .size(ButtonSize::None)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(move |pane, _, window, cx| {
                                pane.close_item_by_id(item_id, SaveIntent::Close, window, cx)
                                    .detach_and_log_err(cx);
                            }))
                    })
                };

                let Some(end_slot_control) = end_slot_control.map(|this| {
                    if is_active {
                        let focus_handle = focus_handle.clone();
                        this.tooltip(move |window, cx| {
                            Tooltip::for_action_in(
                                end_slot_tooltip_text,
                                end_slot_action,
                                &window.focused(cx).unwrap_or_else(|| focus_handle.clone()),
                                cx,
                            )
                        })
                    } else {
                        this.tooltip(Tooltip::text(end_slot_tooltip_text))
                    }
                }) else {
                    return if let Some(indicator) = indicator {
                        this.end_slot(indicator)
                    } else {
                        this
                    };
                };

                let show_control_on_hover =
                    show_close_button == ShowCloseButton::Hover || is_pinned && indicator.is_some();
                let end_slot = if show_control_on_hover {
                    if let Some(indicator) = indicator {
                        h_flex()
                            .relative()
                            .size_full()
                            .justify_center()
                            .child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .group_hover("", |this| this.invisible())
                                    .child(indicator),
                            )
                            .child(end_slot_control.visible_on_hover(""))
                            .into_any_element()
                    } else {
                        end_slot_control.visible_on_hover("").into_any_element()
                    }
                } else {
                    end_slot_control.into_any_element()
                };
                this.end_slot(end_slot)
            })
            .child(
                h_flex()
                    .id(("pane-tab-content", ix))
                    .gap_1()
                    .children(if let Some(decorated_icon) = decorated_icon {
                        Some(decorated_icon.into_any_element())
                    } else if let Some(icon) = icon {
                        Some(icon.into_any_element())
                    } else if !capability.editable() {
                        Some(read_only_toggle(capability == Capability::Read).into_any_element())
                    } else {
                        None
                    })
                    .child(label)
                    .map(|this| match tab_tooltip_content {
                        Some(TabTooltipContent::Text(text)) => {
                            if capability.editable() {
                                this.tooltip(Tooltip::text(text))
                            } else {
                                this.tooltip(move |_, cx| {
                                    let text = text.clone();
                                    Tooltip::with_meta(text, None, "Read-Only File", cx)
                                })
                            }
                        }
                        Some(TabTooltipContent::Custom(element_fn)) => {
                            this.tooltip(move |window, cx| element_fn(window, cx))
                        }
                        None => this,
                    })
                    .when(capability == Capability::Read && has_file_icon, |this| {
                        this.child(read_only_toggle(true))
                    }),
            );

        let single_entry_to_resolve = (self.items[ix].buffer_kind(cx) == ItemBufferKind::Singleton)
            .then(|| self.items[ix].project_entry_ids(cx).get(0).copied())
            .flatten();

        render_tab_context_menu(
            tab,
            TabContextMenuParams {
                pane: cx.entity().downgrade(),
                menu_context: item.item_focus_handle(cx),
                item_handle: item.boxed_clone(),
                item_id,
                ix,
                single_entry_to_resolve,
                total_items: self.items.len(),
                has_multibuffer_items: self
                    .items
                    .iter()
                    .any(|item| item.buffer_kind(cx) == ItemBufferKind::Multibuffer),
                has_items_to_left: ix > 0,
                has_items_to_right: ix < self.items.len() - 1,
                has_clean_items: self.items.iter().any(|item| !item.is_dirty(cx)),
                is_pinned: self.is_tab_pinned(ix),
                capability,
            },
        )
    }
}

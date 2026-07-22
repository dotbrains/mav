use super::*;

impl VariableList {
    pub(super) fn render_entries(
        &mut self,
        ix: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        ix.into_iter()
            .filter_map(|ix| {
                let (entry, state) = self
                    .entries
                    .get(ix)
                    .and_then(|entry| Some(entry).zip(self.entry_states.get(&entry.path)))?;

                match &entry.entry {
                    DapEntry::Watcher { .. } => {
                        Some(self.render_watcher(entry, *state, window, cx))
                    }
                    DapEntry::Variable(_) => Some(self.render_variable(entry, *state, window, cx)),
                    DapEntry::Scope(_) => Some(self.render_scope(entry, *state, cx)),
                }
            })
            .collect()
    }

    pub(super) fn variable_color(
        &self,
        presentation_hint: Option<&VariablePresentationHint>,
        cx: &Context<Self>,
    ) -> VariableColor {
        let syntax_color_for = |name| {
            cx.theme()
                .syntax()
                .style_for_name(name)
                .and_then(|style| style.color)
        };
        let name = if self.disabled {
            Some(Color::Disabled.color(cx))
        } else {
            match presentation_hint
                .as_ref()
                .and_then(|hint| hint.kind.as_ref())
                .unwrap_or(&VariablePresentationHintKind::Unknown)
            {
                VariablePresentationHintKind::Class
                | VariablePresentationHintKind::BaseClass
                | VariablePresentationHintKind::InnerClass
                | VariablePresentationHintKind::MostDerivedClass => syntax_color_for("type"),
                VariablePresentationHintKind::Data => syntax_color_for("variable"),
                VariablePresentationHintKind::Unknown | _ => syntax_color_for("variable"),
            }
        };
        let value = self
            .disabled
            .then(|| Color::Disabled.color(cx))
            .or_else(|| syntax_color_for("variable.special"));

        VariableColor { name, value }
    }

    pub(super) fn render_variable_value(
        &self,
        entry: &ListEntry,
        variable_color: &VariableColor,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !value.is_empty() {
            div()
                .w_full()
                .id(entry.item_value_id())
                .map(|this| {
                    if let Some((_, editor)) = self
                        .edited_path
                        .as_ref()
                        .filter(|(path, _)| path == &entry.path)
                    {
                        this.child(div().size_full().px_2().child(editor.clone()))
                    } else {
                        this.text_color(cx.theme().colors().text_muted)
                            .when(
                                !self.disabled
                                    && self
                                        .session
                                        .read(cx)
                                        .capabilities()
                                        .supports_set_variable
                                        .unwrap_or_default(),
                                |this| {
                                    let path = entry.path.clone();
                                    let variable_value = value.clone();
                                    this.on_click(cx.listener(
                                        move |this, click: &ClickEvent, window, cx| {
                                            if click.click_count() < 2 {
                                                return;
                                            }
                                            let editor = Self::create_variable_editor(
                                                &variable_value,
                                                window,
                                                cx,
                                            );
                                            this.edited_path = Some((path.clone(), editor));

                                            cx.notify();
                                        },
                                    ))
                                },
                            )
                            .child(
                                Label::new(format!("=  {}", &value))
                                    .single_line()
                                    .truncate()
                                    .size(LabelSize::Small)
                                    .color(Color::Muted)
                                    .when_some(variable_color.value, |this, color| {
                                        this.color(Color::from(color))
                                    }),
                            )
                            .tooltip(Tooltip::text(value))
                    }
                })
                .into_any_element()
        } else {
            Empty.into_any_element()
        }
    }

    pub(super) fn center_truncate_string(s: &str, mut max_chars: usize) -> String {
        const ELLIPSIS: &str = "...";
        const MIN_LENGTH: usize = 3;

        max_chars = max_chars.max(MIN_LENGTH);

        let char_count = s.chars().count();
        if char_count <= max_chars {
            return s.to_string();
        }

        if ELLIPSIS.len() + MIN_LENGTH > max_chars {
            return s.chars().take(MIN_LENGTH).collect();
        }

        let available_chars = max_chars - ELLIPSIS.len();

        let start_chars = available_chars / 2;
        let end_chars = available_chars - start_chars;
        let skip_chars = char_count - end_chars;

        let mut start_boundary = 0;
        let mut end_boundary = s.len();

        for (i, (byte_idx, _)) in s.char_indices().enumerate() {
            if i == start_chars {
                start_boundary = byte_idx.max(MIN_LENGTH);
            }

            if i == skip_chars {
                end_boundary = byte_idx;
            }
        }

        if start_boundary >= end_boundary {
            return s.chars().take(MIN_LENGTH).collect();
        }

        format!("{}{}{}", &s[..start_boundary], ELLIPSIS, &s[end_boundary..])
    }

    pub(super) fn render_watcher(
        &self,
        entry: &ListEntry,
        state: EntryState,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(watcher) = &entry.as_watcher() else {
            debug_panic!("Called render watcher on non watcher variable list entry variant");
            return div().into_any_element();
        };

        let variable_color = self.variable_color(watcher.presentation_hint.as_ref(), cx);

        let is_selected = self
            .selection
            .as_ref()
            .is_some_and(|selection| selection == &entry.path);
        let var_ref = watcher.variables_reference;

        let colors = get_entry_color(cx);
        let bg_hover_color = if !is_selected {
            colors.hover
        } else {
            colors.default
        };
        let border_color = if is_selected {
            colors.marked_active
        } else {
            colors.default
        };
        let path = entry.path.clone();

        let weak = cx.weak_entity();
        let focus_handle = self.focus_handle.clone();
        let watcher_len = (f32::from(self.list_handle.content_size().width / 12.0).floor()) - 3.0;
        let watcher_len = watcher_len as usize;

        div()
            .id(entry.item_id())
            .group("variable_list_entry")
            .pl_2()
            .border_1()
            .border_r_2()
            .border_color(border_color)
            .flex()
            .w_full()
            .h_full()
            .hover(|style| style.bg(bg_hover_color))
            .on_click(cx.listener({
                let path = path.clone();
                move |this, _, _window, cx| {
                    this.selection = Some(path.clone());
                    cx.notify();
                }
            }))
            .child(
                ListItem::new(SharedString::from(format!(
                    "watcher-{}",
                    watcher.expression
                )))
                .selectable(false)
                .disabled(self.disabled)
                .selectable(false)
                .indent_level(state.depth)
                .indent_step_size(INDENT_STEP_SIZE)
                .always_show_disclosure_icon(true)
                .when(var_ref > 0, |list_item| {
                    list_item.toggle(state.is_expanded).on_toggle(cx.listener({
                        let var_path = entry.path.clone();
                        move |this, _, _, cx| {
                            this.session.update(cx, |session, cx| {
                                session.variables(var_ref, cx);
                            });

                            this.toggle_entry(&var_path, cx);
                        }
                    }))
                })
                .on_secondary_mouse_down(cx.listener({
                    let path = path.clone();
                    let entry = entry.clone();
                    move |this, event: &MouseDownEvent, window, cx| {
                        this.selection = Some(path.clone());
                        this.deploy_list_entry_context_menu(
                            entry.clone(),
                            event.position,
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }))
                .child(
                    h_flex()
                        .gap_1()
                        .text_ui_sm(cx)
                        .w_full()
                        .child(
                            Label::new(&Self::center_truncate_string(
                                watcher.expression.as_ref(),
                                watcher_len,
                            ))
                            .when_some(variable_color.name, |this, color| {
                                this.color(Color::from(color))
                            }),
                        )
                        .child(self.render_variable_value(
                            entry,
                            &variable_color,
                            watcher.value.to_string(),
                            cx,
                        )),
                )
                .end_slot(
                    IconButton::new(
                        SharedString::from(format!("watcher-{}-remove-button", watcher.expression)),
                        IconName::Close,
                    )
                    .on_click({
                        move |_, window, cx| {
                            weak.update(cx, |variable_list, cx| {
                                variable_list.selection = Some(path.clone());
                                variable_list.remove_watcher(&RemoveWatch, window, cx);
                            })
                            .ok();
                        }
                    })
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action_in("Remove Watch", &RemoveWatch, &focus_handle, cx)
                    })
                    .icon_size(ui::IconSize::Indicator),
                ),
            )
            .into_any()
    }

    pub(super) fn render_scope(
        &self,
        entry: &ListEntry,
        state: EntryState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(scope) = entry.as_scope() else {
            debug_panic!("Called render scope on non scope variable list entry variant");
            return div().into_any_element();
        };

        let var_ref = scope.variables_reference;
        let is_selected = self
            .selection
            .as_ref()
            .is_some_and(|selection| selection == &entry.path);

        let colors = get_entry_color(cx);
        let bg_hover_color = if !is_selected {
            colors.hover
        } else {
            colors.default
        };
        let border_color = if is_selected {
            colors.marked_active
        } else {
            colors.default
        };
        let path = entry.path.clone();

        div()
            .id(var_ref as usize)
            .group("variable_list_entry")
            .pl_2()
            .border_1()
            .border_r_2()
            .border_color(border_color)
            .flex()
            .w_full()
            .h_full()
            .hover(|style| style.bg(bg_hover_color))
            .on_click(cx.listener({
                move |this, _, _window, cx| {
                    this.selection = Some(path.clone());
                    cx.notify();
                }
            }))
            .child(
                ListItem::new(SharedString::from(format!("scope-{}", var_ref)))
                    .selectable(false)
                    .disabled(self.disabled)
                    .indent_level(state.depth)
                    .indent_step_size(px(10.))
                    .always_show_disclosure_icon(true)
                    .toggle(state.is_expanded)
                    .on_toggle({
                        let var_path = entry.path.clone();
                        cx.listener(move |this, _, _, cx| this.toggle_entry(&var_path, cx))
                    })
                    .child(
                        div()
                            .text_ui(cx)
                            .w_full()
                            .truncate()
                            .when(self.disabled, |this| {
                                this.text_color(Color::Disabled.color(cx))
                            })
                            .child(scope.name.clone()),
                    ),
            )
            .into_any()
    }

    pub(super) fn render_variable(
        &self,
        variable: &ListEntry,
        state: EntryState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(dap) = &variable.as_variable() else {
            debug_panic!("Called render variable on non variable variable list entry variant");
            return div().into_any_element();
        };

        let variable_color = self.variable_color(dap.presentation_hint.as_ref(), cx);

        let var_ref = dap.variables_reference;
        let colors = get_entry_color(cx);
        let is_selected = self
            .selection
            .as_ref()
            .is_some_and(|selected_path| *selected_path == variable.path);

        let bg_hover_color = if !is_selected {
            colors.hover
        } else {
            colors.default
        };
        let border_color = if is_selected && self.focus_handle.contains_focused(window, cx) {
            colors.marked_active
        } else {
            colors.default
        };
        let path = variable.path.clone();
        div()
            .id(variable.item_id())
            .group("variable_list_entry")
            .pl_2()
            .border_1()
            .border_r_2()
            .border_color(border_color)
            .h_4()
            .size_full()
            .hover(|style| style.bg(bg_hover_color))
            .on_click(cx.listener({
                let path = path.clone();
                move |this, _, _window, cx| {
                    this.selection = Some(path.clone());
                    cx.notify();
                }
            }))
            .child(
                ListItem::new(SharedString::from(format!(
                    "variable-item-{}-{}",
                    dap.name, state.depth
                )))
                .disabled(self.disabled)
                .selectable(false)
                .indent_level(state.depth)
                .indent_step_size(INDENT_STEP_SIZE)
                .always_show_disclosure_icon(true)
                .when(var_ref > 0, |list_item| {
                    list_item.toggle(state.is_expanded).on_toggle(cx.listener({
                        let var_path = variable.path.clone();
                        move |this, _, _, cx| {
                            this.session.update(cx, |session, cx| {
                                session.variables(var_ref, cx);
                            });

                            this.toggle_entry(&var_path, cx);
                        }
                    }))
                })
                .on_secondary_mouse_down(cx.listener({
                    let entry = variable.clone();
                    move |this, event: &MouseDownEvent, window, cx| {
                        this.selection = Some(path.clone());
                        this.deploy_list_entry_context_menu(
                            entry.clone(),
                            event.position,
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }))
                .child(
                    h_flex()
                        .gap_1()
                        .text_ui_sm(cx)
                        .w_full()
                        .child(
                            Label::new(&dap.name).when_some(variable_color.name, |this, color| {
                                this.color(Color::from(color))
                            }),
                        )
                        .child(self.render_variable_value(
                            variable,
                            &variable_color,
                            dap.value.clone(),
                            cx,
                        )),
                ),
            )
            .into_any()
    }
}

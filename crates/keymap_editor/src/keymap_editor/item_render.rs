use super::*;

impl Item for KeymapEditor {
    type Event = ();

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> ui::SharedString {
        "Keymap Editor".into()
    }
}

impl Render for KeymapEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut ui::Context<Self>) -> impl ui::IntoElement {
        if let SearchMode::KeyStroke { exact_match } = self.search_mode {
            let button = IconButton::new("keystrokes-exact-match", IconName::CaseSensitive)
                .tooltip(move |_window, cx| {
                    Tooltip::for_action(
                        "Toggle Exact Match Mode",
                        &ToggleExactKeystrokeMatching,
                        cx,
                    )
                })
                .shape(IconButtonShape::Square)
                .toggle_state(exact_match)
                .on_click(cx.listener(|_, _, window, cx| {
                    window.dispatch_action(ToggleExactKeystrokeMatching.boxed_clone(), cx);
                }));

            self.keystroke_editor.update(cx, |editor, _| {
                editor.actions_slot = Some(button.into_any_element());
            });
        } else {
            self.keystroke_editor.update(cx, |editor, _| {
                editor.actions_slot = None;
            });
        }

        let row_count = self.matches.len();
        let focus_handle = &self.focus_handle;
        let theme = cx.theme();
        let search_mode = self.search_mode;

        v_flex()
            .id("keymap-editor")
            .track_focus(focus_handle)
            .key_context(self.key_context())
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::focus_search))
            .on_action(cx.listener(Self::edit_binding))
            .on_action(cx.listener(Self::create_binding))
            .on_action(cx.listener(Self::open_create_keybinding_modal))
            .on_action(cx.listener(Self::delete_binding))
            .on_action(cx.listener(Self::copy_action_to_clipboard))
            .on_action(cx.listener(Self::copy_context_to_clipboard))
            .on_action(cx.listener(Self::toggle_conflict_filter))
            .on_action(cx.listener(Self::toggle_no_action_bindings))
            .on_action(cx.listener(Self::toggle_keystroke_search))
            .on_action(cx.listener(Self::toggle_exact_keystroke_matching))
            .on_action(cx.listener(Self::show_matching_keystrokes))
            .on_mouse_move(cx.listener(|this, _, _window, _cx| {
                this.show_hover_menus = true;
            }))
            .size_full()
            .p_2()
            .gap_1()
            .bg(theme.colors().editor_background)
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                h_flex()
                                    .key_context({
                                        let mut context = KeyContext::new_with_defaults();
                                        context.add("BufferSearchBar");
                                        context
                                    })
                                    .flex_1()
                                    .min_w_0()
                                    .h_8()
                                    .pl_2()
                                    .pr_1()
                                    .py_1()
                                    .border_1()
                                    .border_color(theme.colors().border)
                                    .rounded_md()
                                    .child(self.filter_editor.clone()),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .flex_none()
                                    .items_center()
                                    .child(
                                        IconButton::new(
                                            "KeymapEditorKeystrokeSearchButton",
                                            IconName::Keyboard,
                                        )
                                        .icon_size(IconSize::Small)
                                        .toggle_state(matches!(
                                            search_mode,
                                            SearchMode::KeyStroke { .. }
                                        ))
                                        .tooltip({
                                            let focus_handle = focus_handle.clone();
                                            move |_window, cx| {
                                                Tooltip::for_action_in(
                                                    "Search by Keystrokes",
                                                    &ToggleKeystrokeSearch,
                                                    &focus_handle,
                                                    cx,
                                                )
                                            }
                                        })
                                        .on_click(cx.listener(|_, _, window, cx| {
                                            window.dispatch_action(
                                                ToggleKeystrokeSearch.boxed_clone(),
                                                cx,
                                            );
                                        })),
                                    )
                                    .child(
                                        self.render_filter_dropdown(focus_handle, cx)
                                    )
                                    .child(
                                        Button::new("edit-in-json", "Edit in JSON")
                                            .style(ButtonStyle::Subtle)
                                            .key_binding(
                                                ui::KeyBinding::for_action_in(&mav_actions::OpenKeymapFile, &focus_handle, cx)
                                                    .map(|kb| kb.size(rems_from_px(10.))),
                                            )
                                            .on_click(|_, window, cx| {
                                                window.dispatch_action(
                                                    mav_actions::OpenKeymapFile.boxed_clone(),
                                                    cx,
                                                );
                                            })
                                    )
                                    .child(
                                        Button::new("create", "Create Keybinding")
                                            .style(ButtonStyle::Outlined)
                                            .key_binding(
                                                ui::KeyBinding::for_action_in(&OpenCreateKeybindingModal, &focus_handle, cx)
                                                    .map(|kb| kb.size(rems_from_px(10.))),
                                            )
                                            .on_click(|_, window, cx| {
                                                window.dispatch_action(
                                                    OpenCreateKeybindingModal.boxed_clone(),
                                                    cx,
                                                );
                                            })
                                    )
                            ),
                    )
                    .when(
                        matches!(self.search_mode, SearchMode::KeyStroke { .. }),
                        |this| {
                            this.child(
                                h_flex()
                                    .gap_2()
                                    .child(self.keystroke_editor.clone())
                                    .child(div().min_w_96()), // Spacer div to align with the search input
                            )
                        },
                    ),
            )
            .child(
                Table::new(COLS)
                    .interactable(&self.table_interaction_state)
                    .striped()
                    .empty_table_callback({
                        let this = cx.entity();
                        move |window, cx| this.read(cx).render_no_matches_hint(window, cx)
                    })
                    .width_config(ColumnWidthConfig::redistributable(
                        self.current_widths.clone(),
                    ))
                    .header(vec!["", "Action", "Arguments", "Keystrokes", "Context", "Source"])
                    .uniform_list(
                        "keymap-editor-table",
                        row_count,
                        cx.processor(move |this, range: Range<usize>, _window, cx| {
                            let context_menu_deployed = this.context_menu_deployed();
                            range
                                .filter_map(|index| {
                                    let candidate_id = this.matches.get(index)?.candidate_id;
                                    let binding = &this.keybindings[candidate_id];
                                    let action_name = binding.action().name;
                                    let conflict = this.get_conflict(index);
                                    let is_unbound_by_unbind = binding.is_unbound_by_unbind();
                                    let is_overridden = conflict.is_some_and(|conflict| {
                                        !conflict.is_user_keybind_conflict()
                                    });
                                    let is_dimmed = is_overridden || is_unbound_by_unbind;

                                    let icon = this.create_row_button(
                                        index,
                                        conflict,
                                        is_unbound_by_unbind,
                                        cx,
                                    );

                                    let action = div()
                                        .id(("keymap action", index))
                                        .child({
                                            if action_name != gpui::NoAction.name() {
                                                binding
                                                    .action()
                                                    .humanized_name
                                                    .clone()
                                                    .into_any_element()
                                            } else {
                                                const NULL: SharedString =
                                                    SharedString::new_static("<null>");
                                                muted_styled_text(NULL, cx)
                                                    .into_any_element()
                                            }
                                        })
                                        .when(
                                            !context_menu_deployed
                                                && this.show_hover_menus
                                                && !is_dimmed,
                                            |this| {
                                                this.tooltip({
                                                    let action_name = binding.action().name;
                                                    let action_docs =
                                                        binding.action().documentation;
                                                    move |_, cx| {
                                                        let action_tooltip =
                                                            Tooltip::new(action_name);
                                                        let action_tooltip = match action_docs {
                                                            Some(docs) => action_tooltip.meta(docs),
                                                            None => action_tooltip,
                                                        };
                                                        cx.new(|_| action_tooltip).into()
                                                    }
                                                })
                                            },
                                        )
                                        .into_any_element();

                                    let keystrokes = binding.key_binding().map_or(
                                        binding
                                            .keystroke_text()
                                            .cloned()
                                            .unwrap_or_default()
                                            .into_any_element(),
                                        |binding| ui::KeyBinding::from_keystrokes(binding.keystrokes.clone(), binding.source == KeybindSource::Vim).into_any_element()
                                    );

                                    let action_arguments = match binding.action().arguments.clone()
                                    {
                                        Some(arguments) => arguments.into_any_element(),
                                        None => {
                                            if binding.action().has_schema {
                                                muted_styled_text(NO_ACTION_ARGUMENTS_TEXT, cx)
                                                    .into_any_element()
                                            } else {
                                                gpui::Empty.into_any_element()
                                            }
                                        }
                                    };

                                    let context = binding.context().cloned().map_or(
                                        gpui::Empty.into_any_element(),
                                        |context| {
                                            let is_local = context.local().is_some();

                                            div()
                                                .id(("keymap context", index))
                                                .child(context.clone())
                                                .when(
                                                    is_local
                                                        && !context_menu_deployed
                                                        && !is_dimmed
                                                        && this.show_hover_menus,
                                                    |this| {
                                                        this.tooltip(Tooltip::element({
                                                            move |_, _| {
                                                                context.clone().into_any_element()
                                                            }
                                                        }))
                                                    },
                                                )
                                                .into_any_element()
                                        },
                                    );

                                    let source = binding
                                        .keybind_source()
                                        .map(|source| source.name())
                                        .unwrap_or_default()
                                        .into_any_element();

                                    Some(vec![
                                        icon.into_any_element(),
                                        action,
                                        action_arguments,
                                        keystrokes,
                                        context,
                                        source,
                                    ])
                                })
                                .collect()
                        }),
                    )
                    .map_row(cx.processor(
                        |this, (row_index, row): (usize, Stateful<Div>), _window, cx| {
                        let conflict = this.get_conflict(row_index);
                            let candidate_id = this.matches.get(row_index).map(|candidate| candidate.candidate_id);
                            let is_unbound_by_unbind = candidate_id
                                .and_then(|candidate_id| this.keybindings.get(candidate_id))
                                .is_some_and(ProcessedBinding::is_unbound_by_unbind);
                            let is_selected = this.selected_index == Some(row_index);

                            let row_id = row_group_id(row_index);

                            div()
                                .id(("keymap-row-wrapper", row_index))
                                .child(
                                    row.id(row_id.clone())
                                        .when(!is_unbound_by_unbind, |row| {
                                            row.on_any_mouse_down(cx.listener(
                                                move |this,
                                                      mouse_down_event: &gpui::MouseDownEvent,
                                                      window,
                                                      cx| {
                                                    if mouse_down_event.button == MouseButton::Right {
                                                        this.select_index(
                                                            row_index, None, window, cx,
                                                        );
                                                        this.create_context_menu(
                                                            mouse_down_event.position,
                                                            window,
                                                            cx,
                                                        );
                                                    }
                                                },
                                            ))
                                        })
                                        .when(!is_unbound_by_unbind, |row| {
                                            row.on_click(cx.listener(
                                                move |this, event: &ClickEvent, window, cx| {
                                                    this.select_index(row_index, None, window, cx);
                                                    if event.click_count() == 2 {
                                                        this.open_edit_keybinding_modal(
                                                            false, window, cx,
                                                        );
                                                    }
                                                },
                                            ))
                                        })
                                        .group(row_id)
                                        .when(
                                            is_unbound_by_unbind
                                                || conflict.is_some_and(|conflict| {
                                                    !conflict.is_user_keybind_conflict()
                                                }),
                                            |row| {
                                                const OVERRIDDEN_OPACITY: f32 = 0.5;
                                                row.opacity(OVERRIDDEN_OPACITY)
                                            },
                                        )
                                        .when_some(
                                            conflict.filter(|conflict| {
                                                !is_unbound_by_unbind
                                                    && !this.context_menu_deployed() &&
                                                !conflict.is_user_keybind_conflict()
                                            }),
                                            |row, conflict| {
                                                let overriding_binding = this.keybindings.get(conflict.index);
                                                let context = overriding_binding.and_then(|binding| {
                                                    match conflict.override_source {
                                                        KeybindSource::User  => Some("your keymap"),
                                                        KeybindSource::Vim => Some("the vim keymap"),
                                                        KeybindSource::Base => Some("your base keymap"),
                                                        _ => {
                                                            log::error!("Unexpected override from the {} keymap", conflict.override_source.name());
                                                            None
                                                        }
                                                    }.map(|source| format!("This keybinding is overridden by the '{}' binding from {}.", binding.action().humanized_name, source))
                                                }).unwrap_or_else(|| "This binding is overridden.".to_string());

                                                row.tooltip(Tooltip::text(context))
                                            },
                                        )
                                        .when(is_unbound_by_unbind, |row| {
                                            row.tooltip(Tooltip::text("This action is unbound"))
                                        }),
                                )
                                .border_2()
                                .when(
                                    conflict.is_some_and(|conflict| {
                                        conflict.is_user_keybind_conflict()
                                    }),
                                    |row| row.bg(cx.theme().status().error_background),
                                )
                                .when(is_selected, |row| {
                                    row.border_color(cx.theme().colors().panel_focused_border)
                                })
                                .into_any_element()
                        }),
                    ),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                // This ensures that the menu is not dismissed in cases where scroll events
                // with a delta of zero are emitted
                if !event.delta.pixel_delta(px(1.)).y.is_zero() {
                    this.context_menu.take();
                    cx.notify();
                }
            }))
            .children(self.context_menu.as_ref().map(|(menu, position, _)| {
                deferred(
                    anchored()
                        .position(*position)
                        .anchor(gpui::Anchor::TopLeft)
                        .child(menu.clone()),
                )
                .with_priority(1)
            }))
    }
}

use super::*;

impl KeymapEditor {
    fn key_context(&self) -> KeyContext {
        let mut dispatch_context = KeyContext::new_with_defaults();
        dispatch_context.add("KeymapEditor");
        dispatch_context.add("menu");

        dispatch_context
    }

    fn scroll_to_item(&self, index: usize, strategy: ScrollStrategy, cx: &mut App) {
        let index = usize::min(index, self.matches.len().saturating_sub(1));
        self.table_interaction_state.update(cx, |this, _cx| {
            this.scroll_handle.scroll_to_item(index, strategy);
        });
    }

    fn focus_search(
        &mut self,
        _: &search::FocusSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .filter_editor
            .focus_handle(cx)
            .contains_focused(window, cx)
        {
            window.focus(&self.filter_editor.focus_handle(cx), cx);
        } else {
            self.filter_editor.update(cx, |editor, cx| {
                editor.select_all(&Default::default(), window, cx);
            });
        }
        self.selected_index.take();
    }

    fn selected_keybind_index(&self) -> Option<usize> {
        self.selected_index
            .and_then(|match_index| self.matches.get(match_index))
            .map(|r#match| r#match.candidate_id)
    }

    fn selected_keybind_and_index(&self) -> Option<(&ProcessedBinding, usize)> {
        self.selected_keybind_index()
            .map(|keybind_index| (&self.keybindings[keybind_index], keybind_index))
    }

    fn selected_binding(&self) -> Option<&ProcessedBinding> {
        self.selected_keybind_index()
            .and_then(|keybind_index| self.keybindings.get(keybind_index))
    }

    fn select_index(
        &mut self,
        index: usize,
        scroll: Option<ScrollStrategy>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_index != Some(index) {
            self.selected_index = Some(index);
            if let Some(scroll_strategy) = scroll {
                self.scroll_to_item(index, scroll_strategy, cx);
            }
            window.focus(&self.focus_handle, cx);
            cx.notify();
        }
    }

    fn create_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = self.selected_binding().map(|selected_binding| {
            let selected_binding_has_no_context = selected_binding
                .context()
                .and_then(KeybindContextString::local)
                .is_none();

            let selected_binding_is_unmapped = selected_binding.is_unbound();
            let selected_binding_is_suppressed = selected_binding.is_unbound_by_unbind();
            let selected_binding_is_non_interactable =
                selected_binding_is_unmapped || selected_binding_is_suppressed;

            let context_menu = ContextMenu::build(window, cx, |menu, _window, _cx| {
                menu.context(self.focus_handle.clone())
                    .when(selected_binding_is_unmapped, |this| {
                        this.action("Create", Box::new(CreateBinding))
                    })
                    .action_disabled_when(
                        selected_binding_is_non_interactable,
                        "Edit",
                        Box::new(EditBinding),
                    )
                    .action_disabled_when(
                        selected_binding_is_non_interactable,
                        "Delete",
                        Box::new(DeleteBinding),
                    )
                    .separator()
                    .action("Copy Action", Box::new(CopyAction))
                    .action_disabled_when(
                        selected_binding_has_no_context,
                        "Copy Context",
                        Box::new(CopyContext),
                    )
                    .separator()
                    .action_disabled_when(
                        selected_binding_has_no_context,
                        "Show Matching Keybindings",
                        Box::new(ShowMatchingKeybinds),
                    )
            });

            let context_menu_handle = context_menu.focus_handle(cx);
            window.defer(cx, move |window, cx| window.focus(&context_menu_handle, cx));
            let subscription = cx.subscribe_in(
                &context_menu,
                window,
                |this, _, _: &DismissEvent, window, cx| {
                    this.dismiss_context_menu(window, cx);
                },
            );
            (context_menu, position, subscription)
        });

        cx.notify();
    }

    fn dismiss_context_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.context_menu.take();
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn context_menu_deployed(&self) -> bool {
        self.context_menu.is_some()
    }

    fn create_row_button(
        &self,
        index: usize,
        conflict: Option<ConflictOrigin>,
        is_unbound_by_unbind: bool,
        cx: &mut Context<Self>,
    ) -> IconButton {
        if is_unbound_by_unbind {
            base_button_style(index, IconName::Warning)
                .icon_color(Color::Warning)
                .disabled(true)
                .tooltip(Tooltip::text("This action is unbound"))
        } else if self.filter_state != FilterState::Conflicts
            && let Some(conflict) = conflict
        {
            if conflict.is_user_keybind_conflict() {
                base_button_style(index, IconName::Warning)
                    .icon_color(Color::Warning)
                    .tooltip(|_window, cx| {
                        Tooltip::with_meta(
                            "View conflicts",
                            Some(&ToggleConflictFilter),
                            "Use alt+click to show all conflicts",
                            cx,
                        )
                    })
                    .on_click(cx.listener(move |this, click: &ClickEvent, window, cx| {
                        if click.modifiers().alt {
                            this.set_filter_state(FilterState::Conflicts, cx);
                        } else {
                            this.select_index(index, None, window, cx);
                            this.open_edit_keybinding_modal(false, window, cx);
                            cx.stop_propagation();
                        }
                    }))
            } else if self.search_mode.exact_match() {
                base_button_style(index, IconName::Info)
                    .tooltip(|_window, cx| {
                        Tooltip::with_meta(
                            "Edit this binding",
                            Some(&ShowMatchingKeybinds),
                            "This binding is overridden by other bindings.",
                            cx,
                        )
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.select_index(index, None, window, cx);
                        this.open_edit_keybinding_modal(false, window, cx);
                        cx.stop_propagation();
                    }))
            } else {
                base_button_style(index, IconName::Info)
                    .tooltip(|_window, cx|  {
                        Tooltip::with_meta(
                            "Show matching keybinds",
                            Some(&ShowMatchingKeybinds),
                            "This binding is overridden by other bindings.\nUse alt+click to edit this binding",
                            cx,
                        )
                    })
                    .on_click(cx.listener(move |this, click: &ClickEvent, window, cx| {
                        if click.modifiers().alt {
                            this.select_index(index, None, window, cx);
                            this.open_edit_keybinding_modal(false, window, cx);
                            cx.stop_propagation();
                        } else {
                            this.show_matching_keystrokes(&Default::default(), window, cx);
                        }
                    }))
            }
        } else {
            base_button_style(index, IconName::Pencil)
                .visible_on_hover(if self.selected_index == Some(index) {
                    "".into()
                } else if self.show_hover_menus {
                    row_group_id(index)
                } else {
                    "never-show".into()
                })
                .when(
                    self.show_hover_menus && !self.context_menu_deployed(),
                    |this| this.tooltip(Tooltip::for_action_title("Edit Keybinding", &EditBinding)),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_index(index, None, window, cx);
                    this.open_edit_keybinding_modal(false, window, cx);
                    cx.stop_propagation();
                }))
        }
    }

    fn render_no_matches_hint(&self, _window: &mut Window, _cx: &App) -> AnyElement {
        let hint = match (self.filter_state, &self.search_mode) {
            (FilterState::Conflicts, _) => {
                if self.keybinding_conflict_state.any_user_binding_conflicts() {
                    "No conflicting keybinds found that match the provided query"
                } else {
                    "No conflicting keybinds found"
                }
            }
            (FilterState::All, SearchMode::KeyStroke { .. }) => {
                "No keybinds found matching the entered keystrokes"
            }
            (FilterState::All, SearchMode::Normal) => "No matches found for the provided query",
        };

        Label::new(hint).color(Color::Muted).into_any_element()
    }

    fn select_next(&mut self, _: &menu::SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        self.show_hover_menus = false;
        if let Some(selected) = self.selected_index {
            let selected = selected + 1;
            if selected >= self.matches.len() {
                self.select_last(&Default::default(), window, cx);
            } else {
                self.select_index(selected, Some(ScrollStrategy::Center), window, cx);
            }
        } else {
            self.select_first(&Default::default(), window, cx);
        }
    }

    fn select_previous(
        &mut self,
        _: &menu::SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_hover_menus = false;
        if let Some(selected) = self.selected_index {
            if selected == 0 {
                return;
            }

            let selected = selected - 1;

            if selected >= self.matches.len() {
                self.select_last(&Default::default(), window, cx);
            } else {
                self.select_index(selected, Some(ScrollStrategy::Center), window, cx);
            }
        } else {
            self.select_last(&Default::default(), window, cx);
        }
    }

    fn select_first(&mut self, _: &menu::SelectFirst, window: &mut Window, cx: &mut Context<Self>) {
        self.show_hover_menus = false;
        if self.matches.get(0).is_some() {
            self.select_index(0, Some(ScrollStrategy::Center), window, cx);
        }
    }

    fn select_last(&mut self, _: &menu::SelectLast, window: &mut Window, cx: &mut Context<Self>) {
        self.show_hover_menus = false;
        if self.matches.last().is_some() {
            let index = self.matches.len() - 1;
            self.select_index(index, Some(ScrollStrategy::Center), window, cx);
        }
    }
}

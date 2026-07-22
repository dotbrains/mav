use super::*;

impl KeymapEditor {
    fn open_edit_keybinding_modal(
        &mut self,
        create: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_hover_menus = false;
        let Some((keybind, keybind_index)) = self.selected_keybind_and_index() else {
            return;
        };
        if !create && keybind.is_unbound_by_unbind() {
            return;
        }
        let keybind = keybind.clone();
        let keymap_editor = cx.entity();

        let keystroke = keybind.keystroke_text().cloned().unwrap_or_default();
        let arguments = keybind
            .action()
            .arguments
            .as_ref()
            .map(|arguments| arguments.text.clone());
        let context = keybind
            .context()
            .map(|context| context.local_str().unwrap_or("global"));
        let action = keybind.action().name;
        let source = keybind.keybind_source().map(|source| source.name());

        telemetry::event!(
            "Edit Keybinding Modal Opened",
            keystroke = keystroke,
            action = action,
            source = source,
            context = context,
            arguments = arguments,
        );

        let temp_dir = self.action_args_temp_dir.as_ref().map(|dir| dir.path());

        self.workspace
            .update(cx, |workspace, cx| {
                let fs = workspace.app_state().fs.clone();
                let workspace_weak = cx.weak_entity();
                workspace.toggle_modal(window, cx, |window, cx| {
                    let modal = KeybindingEditorModal::new(
                        create,
                        keybind,
                        keybind_index,
                        keymap_editor,
                        temp_dir,
                        workspace_weak,
                        fs,
                        window,
                        cx,
                    );
                    window.focus(&modal.focus_handle(cx), cx);
                    modal
                });
            })
            .log_err();
    }

    fn edit_binding(&mut self, _: &EditBinding, window: &mut Window, cx: &mut Context<Self>) {
        self.open_edit_keybinding_modal(false, window, cx);
    }

    fn create_binding(&mut self, _: &CreateBinding, window: &mut Window, cx: &mut Context<Self>) {
        self.open_edit_keybinding_modal(true, window, cx);
    }

    fn open_create_keybinding_modal(
        &mut self,
        _: &OpenCreateKeybindingModal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keymap_editor = cx.entity();

        let action_information = ActionInformation::new(
            gpui::NoAction.name(),
            None,
            &HashSet::default(),
            cx.action_documentation(),
            &self.humanized_action_names,
        );

        let dummy_binding = ProcessedBinding::Unmapped(action_information);
        let dummy_index = self.keybindings.len();

        let temp_dir = self.action_args_temp_dir.as_ref().map(|dir| dir.path());

        self.workspace
            .update(cx, |workspace, cx| {
                let fs = workspace.app_state().fs.clone();
                let workspace_weak = cx.weak_entity();
                workspace.toggle_modal(window, cx, |window, cx| {
                    let modal = KeybindingEditorModal::new(
                        true,
                        dummy_binding,
                        dummy_index,
                        keymap_editor,
                        temp_dir,
                        workspace_weak,
                        fs,
                        window,
                        cx,
                    );

                    window.focus(&modal.focus_handle(cx), cx);
                    modal
                });
            })
            .log_err();
    }

    fn delete_binding(&mut self, _: &DeleteBinding, window: &mut Window, cx: &mut Context<Self>) {
        let Some(to_remove) = self.selected_binding().cloned() else {
            return;
        };
        if to_remove.is_unbound_by_unbind() {
            return;
        }

        let std::result::Result::Ok(fs) = self
            .workspace
            .read_with(cx, |workspace, _| workspace.app_state().fs.clone())
        else {
            return;
        };
        self.previous_edit = Some(PreviousEdit::ScrollBarOffset(
            self.table_interaction_state.read(cx).scroll_offset(),
        ));
        let keyboard_mapper = cx.keyboard_mapper().clone();
        cx.spawn(async move |_, _| {
            remove_keybinding(to_remove, &fs, keyboard_mapper.as_ref()).await
        })
        .detach_and_notify_err(self.workspace.clone(), window, cx);
    }

    fn copy_context_to_clipboard(
        &mut self,
        _: &CopyContext,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let context = self
            .selected_binding()
            .and_then(|binding| binding.context())
            .and_then(KeybindContextString::local_str)
            .map(|context| context.to_string());
        let Some(context) = context else {
            return;
        };

        telemetry::event!("Keybinding Context Copied", context = context);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(context));
    }

    fn copy_action_to_clipboard(
        &mut self,
        _: &CopyAction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = self
            .selected_binding()
            .map(|binding| binding.action().name.to_string());
        let Some(action) = action else {
            return;
        };

        telemetry::event!("Keybinding Action Copied", action = action);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(action));
    }

    fn toggle_conflict_filter(
        &mut self,
        _: &ToggleConflictFilter,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_filter_state(self.filter_state.invert(), cx);
    }

    fn toggle_no_action_bindings(
        &mut self,
        _: &ToggleNoActionBindings,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_no_action_bindings = !self.show_no_action_bindings;
        self.on_query_changed(cx);
    }

    fn toggle_user_bindings_filter(&mut self, cx: &mut Context<Self>) {
        self.source_filters.user = !self.source_filters.user;
        self.on_query_changed(cx);
    }

    fn toggle_mav_defaults_filter(&mut self, cx: &mut Context<Self>) {
        self.source_filters.mav_defaults = !self.source_filters.mav_defaults;
        self.on_query_changed(cx);
    }

    fn toggle_vim_defaults_filter(&mut self, cx: &mut Context<Self>) {
        self.source_filters.vim_defaults = !self.source_filters.vim_defaults;
        self.on_query_changed(cx);
    }

    fn set_filter_state(&mut self, filter_state: FilterState, cx: &mut Context<Self>) {
        if self.filter_state != filter_state {
            self.filter_state = filter_state;
            self.on_query_changed(cx);
        }
    }

    fn toggle_keystroke_search(
        &mut self,
        _: &ToggleKeystrokeSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_mode = self.search_mode.invert();
        self.on_query_changed(cx);

        match self.search_mode {
            SearchMode::KeyStroke { .. } => {
                self.keystroke_editor.update(cx, |editor, cx| {
                    editor.start_recording(&StartRecording, window, cx);
                });
            }
            SearchMode::Normal => {
                self.keystroke_editor.update(cx, |editor, cx| {
                    editor.stop_recording(&StopRecording, window, cx);
                    editor.clear_keystrokes(&ClearKeystrokes, window, cx);
                });
                window.focus(&self.filter_editor.focus_handle(cx), cx);
            }
        }
    }

    fn toggle_exact_keystroke_matching(
        &mut self,
        _: &ToggleExactKeystrokeMatching,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let SearchMode::KeyStroke { exact_match } = &mut self.search_mode else {
            return;
        };

        *exact_match = !(*exact_match);
        self.on_query_changed(cx);
    }

    fn show_matching_keystrokes(
        &mut self,
        _: &ShowMatchingKeybinds,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected_binding) = self.selected_binding() else {
            return;
        };

        let keystrokes = selected_binding
            .keystrokes()
            .map(Vec::from)
            .unwrap_or_default();

        self.filter_state = FilterState::All;
        self.search_mode = SearchMode::KeyStroke { exact_match: true };

        self.keystroke_editor.update(cx, |editor, cx| {
            editor.set_keystrokes(keystrokes, cx);
        });
    }

    fn has_binding_for(&self, action_name: &str) -> bool {
        self.keybindings
            .iter()
            .filter(|kb| kb.keystrokes().is_some())
            .any(|kb| kb.action().name == action_name)
    }

    fn render_filter_dropdown(
        &self,
        focus_handle: &FocusHandle,
        cx: &mut Context<KeymapEditor>,
    ) -> impl IntoElement {
        let focus_handle = focus_handle.clone();
        let keymap_editor = cx.entity();
        return PopoverMenu::new("keymap-editor-filter-menu")
            .menu(move |window, cx| {
                Some(ContextMenu::build_persistent(window, cx, {
                    let focus_handle = focus_handle.clone();
                    let keymap_editor = keymap_editor.clone();
                    move |mut menu, _window, cx| {
                        let (filter_state, source_filters, show_no_action_bindings) = keymap_editor
                            .read_with(cx, |editor, _| {
                                (
                                    editor.filter_state,
                                    editor.source_filters,
                                    editor.show_no_action_bindings,
                                )
                            });

                        menu = menu
                            .context(focus_handle.clone())
                            .header("Filters")
                            .map(add_filter(
                                "Conflicts",
                                matches!(filter_state, FilterState::Conflicts),
                                Some(ToggleConflictFilter.boxed_clone()),
                                &focus_handle,
                                &keymap_editor,
                                None,
                            ))
                            .map(add_filter(
                                "No Action",
                                show_no_action_bindings,
                                Some(ToggleNoActionBindings.boxed_clone()),
                                &focus_handle,
                                &keymap_editor,
                                None,
                            ))
                            .separator()
                            .header("Categories")
                            .map(add_filter(
                                "User",
                                source_filters.user,
                                None,
                                &focus_handle,
                                &keymap_editor,
                                Some(|editor, cx| {
                                    editor.toggle_user_bindings_filter(cx);
                                }),
                            ))
                            .map(add_filter(
                                "Default",
                                source_filters.mav_defaults,
                                None,
                                &focus_handle,
                                &keymap_editor,
                                Some(|editor, cx| {
                                    editor.toggle_mav_defaults_filter(cx);
                                }),
                            ))
                            .map(add_filter(
                                "Vim",
                                source_filters.vim_defaults,
                                None,
                                &focus_handle,
                                &keymap_editor,
                                Some(|editor, cx| {
                                    editor.toggle_vim_defaults_filter(cx);
                                }),
                            ));
                        menu
                    }
                }))
            })
            .anchor(gpui::Anchor::TopRight)
            .offset(gpui::Point {
                x: px(0.0),
                y: px(2.0),
            })
            .trigger_with_tooltip(
                IconButton::new("KeymapEditorFilterMenuButton", IconName::Sliders)
                    .icon_size(IconSize::Small)
                    .when(
                        self.keybinding_conflict_state.any_user_binding_conflicts(),
                        |this| this.indicator(Indicator::dot().color(Color::Warning)),
                    ),
                Tooltip::text("Filters"),
            );

        fn add_filter(
            name: &'static str,
            toggled: bool,
            action: Option<Box<dyn Action>>,
            focus_handle: &FocusHandle,
            keymap_editor: &Entity<KeymapEditor>,
            cb: Option<fn(&mut KeymapEditor, &mut Context<KeymapEditor>)>,
        ) -> impl FnOnce(ContextMenu) -> ContextMenu {
            let focus_handle = focus_handle.clone();
            let keymap_editor = keymap_editor.clone();
            return move |menu: ContextMenu| {
                menu.toggleable_entry(
                    name,
                    toggled,
                    IconPosition::End,
                    action.as_ref().map(|a| a.boxed_clone()),
                    move |window, cx| {
                        window.focus(&focus_handle, cx);
                        if let Some(action) = &action {
                            window.dispatch_action(action.boxed_clone(), cx);
                        } else if let Some(cb) = cb {
                            keymap_editor.update(cx, cb);
                        }
                    },
                )
            };
        }
    }
}

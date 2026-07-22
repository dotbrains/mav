use super::*;

impl KeybindingEditorModal {
    fn save(&mut self, cx: &mut Context<Self>) -> Result<(), InputError> {
        let existing_keybind = self.editing_keybind.clone();
        let fs = self.fs.clone();

        let mut new_keystrokes = self.validate_keystrokes(cx).map_err(InputError::error)?;
        new_keystrokes
            .iter_mut()
            .for_each(|ks| ks.remove_key_char());

        let new_context = self.validate_context(cx).map_err(InputError::error)?;
        let new_action_args = self
            .validate_action_arguments(cx)
            .map_err(InputError::error)?;

        let action_mapping = ActionMapping {
            keystrokes: Rc::from(new_keystrokes.as_slice()),
            context: new_context.map(SharedString::from),
        };

        let conflicting_indices = self
            .keymap_editor
            .read(cx)
            .keybinding_conflict_state
            .conflicting_indices_for_mapping(
                &action_mapping,
                self.creating.not().then_some(self.editing_keybind_idx),
            );

        conflicting_indices.map(|KeybindConflict {
            first_conflict_index,
            remaining_conflict_amount,
        }|
        {
            let conflicting_action_name = self
                .keymap_editor
                .read(cx)
                .keybindings
                .get(first_conflict_index)
                .map(|keybind| keybind.action().name);

            let warning_message = match conflicting_action_name {
                Some(name) => {
                     if remaining_conflict_amount > 0 {
                        format!(
                            "Your keybind would conflict with the \"{}\" action and {} other bindings",
                            name, remaining_conflict_amount
                        )
                    } else {
                        format!("Your keybind would conflict with the \"{}\" action", name)
                    }
                }
                None => {
                    log::info!(
                        "Could not find action in keybindings with index {}",
                        first_conflict_index
                    );
                    "Your keybind would conflict with other actions".to_string()
                }
            };

            let warning = InputError::warning(warning_message);
            if self.error.as_ref().is_some_and(|old_error| *old_error == warning) {
                Ok(())
           } else {
                Err(warning)
            }
        }).unwrap_or(Ok(()))?;

        let create = self.creating;
        let keyboard_mapper = cx.keyboard_mapper().clone();

        let action_name = self
            .get_selected_action_name(cx)
            .map_err(InputError::error)?;

        let humanized_action_name: SharedString =
            command_palette::humanize_action_name(action_name).into();

        let action_information = ActionInformation::new(
            action_name,
            None,
            &HashSet::default(),
            cx.action_documentation(),
            &self.keymap_editor.read(cx).humanized_action_names,
        );

        let keybind_for_save = if create {
            ProcessedBinding::Unmapped(action_information)
        } else {
            existing_keybind
        };

        cx.spawn(async move |this, cx| {
            match save_keybinding_update(
                create,
                keybind_for_save,
                &action_mapping,
                new_action_args.as_deref(),
                &fs,
                keyboard_mapper.as_ref(),
            )
            .await
            {
                Ok(_) => {
                    this.update(cx, |this, cx| {
                        this.keymap_editor.update(cx, |keymap, cx| {
                            keymap.previous_edit = Some(PreviousEdit::Keybinding {
                                action_mapping,
                                action_name,
                                fallback: keymap.table_interaction_state.read(cx).scroll_offset(),
                            });
                            let status_toast = StatusToast::new(
                                format!("Saved edits to the {} action.", humanized_action_name),
                                cx,
                                move |this, _cx| {
                                    this.icon(
                                        Icon::new(IconName::Check)
                                            .size(IconSize::Small)
                                            .color(Color::Success),
                                    )
                                    .dismiss_button(true)
                                    // .action("Undo", f) todo: wire the undo functionality
                                },
                            );

                            this.workspace
                                .update(cx, |workspace, cx| {
                                    workspace.toggle_status_toast(status_toast, cx);
                                })
                                .log_err();
                        });
                        cx.emit(DismissEvent);
                    })
                    .ok();
                }
                Err(err) => {
                    this.update(cx, |this, cx| {
                        this.set_error(InputError::error(err), cx);
                    })
                    .log_err();
                }
            }
        })
        .detach();

        Ok(())
    }

    fn is_any_editor_showing_completions(&self, window: &Window, cx: &App) -> bool {
        let is_editor_showing_completions =
            |focus_handle: &FocusHandle, editor_entity: &Entity<Editor>| -> bool {
                focus_handle.contains_focused(window, cx)
                    && editor_entity.read_with(cx, |editor, _cx| {
                        editor
                            .context_menu()
                            .borrow()
                            .as_ref()
                            .is_some_and(|menu| menu.visible())
                    })
            };

        self.action_editor.as_ref().is_some_and(|action_editor| {
            let focus_handle = action_editor.read(cx).focus_handle(cx);
            let editor_entity = action_editor.read(cx).editor();
            let editor_entity = editor_entity
                .as_any()
                .downcast_ref::<Entity<Editor>>()
                .unwrap();
            is_editor_showing_completions(&focus_handle, editor_entity)
        }) || {
            let focus_handle = self.context_editor.read(cx).focus_handle(cx);
            let editor_entity = self.context_editor.read(cx).editor();
            let editor_entity = editor_entity
                .as_any()
                .downcast_ref::<Entity<Editor>>()
                .unwrap();
            is_editor_showing_completions(&focus_handle, editor_entity)
        } || self
            .action_arguments_editor
            .as_ref()
            .is_some_and(|args_editor| {
                let focus_handle = args_editor.read(cx).focus_handle(cx);
                let editor_entity = &args_editor.read(cx).editor;
                is_editor_showing_completions(&focus_handle, editor_entity)
            })
    }

    fn key_context(&self) -> KeyContext {
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("KeybindEditorModal");
        key_context
    }

    fn key_context_internal(&self, window: &Window, cx: &App) -> KeyContext {
        let mut key_context = self.key_context();

        if self.is_any_editor_showing_completions(window, cx) {
            key_context.add("showing_completions");
        }

        key_context
    }

    fn focus_next(&mut self, _: &menu::SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_any_editor_showing_completions(window, cx) {
            return;
        }
        self.focus_state.focus_next(window, cx);
    }

    fn focus_prev(
        &mut self,
        _: &menu::SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_any_editor_showing_completions(window, cx) {
            return;
        }
        self.focus_state.focus_previous(window, cx);
    }

    fn confirm(&mut self, _: &menu::Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        self.save_or_display_error(cx);
    }

    fn cancel(&mut self, _: &menu::Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn get_matching_bindings_count(&self, cx: &Context<Self>) -> usize {
        let current_keystrokes = self.keybind_editor.read(cx).keystrokes();

        if current_keystrokes.is_empty() {
            return 0;
        }

        self.keymap_editor
            .read(cx)
            .keybindings
            .iter()
            .enumerate()
            .filter(|(idx, binding)| {
                // Don't count the binding we're currently editing
                if !self.creating && *idx == self.editing_keybind_idx {
                    return false;
                }

                binding.keystrokes().is_some_and(|keystrokes| {
                    keystrokes_match_exactly(keystrokes, current_keystrokes)
                })
            })
            .count()
    }

    fn show_matching_bindings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keystrokes = self.keybind_editor.read(cx).keystrokes().to_vec();

        self.keymap_editor.update(cx, |keymap_editor, cx| {
            keymap_editor.clear_action_query(window, cx)
        });

        // Dismiss the modal
        cx.emit(DismissEvent);

        // Update the keymap editor to show matching keystrokes
        self.keymap_editor.update(cx, |editor, cx| {
            editor.filter_state = FilterState::All;
            editor.search_mode = SearchMode::KeyStroke { exact_match: true };
            editor.keystroke_editor.update(cx, |keystroke_editor, cx| {
                keystroke_editor.set_keystrokes(keystrokes, cx);
            });
        });
    }
}

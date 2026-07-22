use super::*;

impl KeybindingEditorModal {
    pub fn new(
        create: bool,
        editing_keybind: ProcessedBinding,
        editing_keybind_idx: usize,
        keymap_editor: Entity<KeymapEditor>,
        action_args_temp_dir: Option<&std::path::Path>,
        workspace: WeakEntity<Workspace>,
        fs: Arc<dyn Fs>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let keybind_editor = cx
            .new(|cx| KeystrokeInput::new(editing_keybind.keystrokes().map(Vec::from), window, cx));

        let context_editor: Entity<InputField> = cx.new(|cx| {
            let input = InputField::new(window, cx, "Keybinding Context")
                .label("Edit Context")
                .label_size(LabelSize::Default);

            if let Some(context) = editing_keybind
                .context()
                .and_then(KeybindContextString::local)
            {
                input.set_text(&context, window, cx);
            }

            let editor_entity = input.editor();
            let editor_entity = editor_entity
                .as_any()
                .downcast_ref::<Entity<Editor>>()
                .unwrap()
                .clone();
            let workspace = workspace.clone();
            cx.spawn(async move |_input_handle, cx| {
                let contexts = cx
                    .background_spawn(async { collect_contexts_from_assets() })
                    .await;

                let language = load_keybind_context_language(workspace, cx).await;
                editor_entity.update(cx, |editor, cx| {
                    if let Some(buffer) = editor.buffer().read(cx).as_singleton() {
                        buffer.update(cx, |buffer, cx| {
                            buffer.set_language(Some(language), cx);
                        });
                    }
                    editor.set_completion_provider(Some(std::rc::Rc::new(
                        KeyContextCompletionProvider { contexts },
                    )));
                });
            })
            .detach();

            input
        });

        let has_action_editor = create && editing_keybind.action().name == gpui::NoAction.name();

        let (action_editor, action_name_to_static) = if has_action_editor {
            let actions: Vec<&'static str> = cx.all_action_names().to_vec();

            let humanized_names: HashMap<&'static str, SharedString> = actions
                .iter()
                .map(|&name| (name, command_palette::humanize_action_name(name).into()))
                .collect();

            let action_name_to_static: HashMap<String, &'static str> = actions
                .iter()
                .map(|&name| (name.to_string(), name))
                .collect();

            let editor = cx.new(|cx| {
                let input = InputField::new(window, cx, "Type an action name")
                    .label("Action")
                    .label_size(LabelSize::Default);

                let editor_entity = input.editor();
                let editor_entity = editor_entity
                    .as_any()
                    .downcast_ref::<Entity<Editor>>()
                    .unwrap();
                editor_entity.update(cx, |editor, _cx| {
                    editor.set_completion_provider(Some(std::rc::Rc::new(
                        ActionCompletionProvider::new(actions, humanized_names),
                    )));
                });

                input
            });

            (Some(editor), action_name_to_static)
        } else {
            (None, HashMap::default())
        };

        let action_has_schema = editing_keybind.action().has_schema;
        let action_name_for_args = editing_keybind.action().name;
        let action_args = editing_keybind
            .action()
            .arguments
            .as_ref()
            .map(|args| args.text.clone());

        let action_arguments_editor = action_has_schema.then(|| {
            cx.new(|cx| {
                ActionArgumentsEditor::new(
                    action_name_for_args,
                    action_args.clone(),
                    action_args_temp_dir,
                    workspace.clone(),
                    window,
                    cx,
                )
            })
        });

        let focus_state = KeybindingEditorModalFocusState::new(
            action_editor.as_ref().map(|e| e.focus_handle(cx)),
            keybind_editor.focus_handle(cx),
            action_arguments_editor
                .as_ref()
                .map(|args_editor| args_editor.focus_handle(cx)),
            context_editor.focus_handle(cx),
        );

        Self {
            creating: create,
            editing_keybind,
            editing_keybind_idx,
            fs,
            keybind_editor,
            context_editor,
            action_editor,
            action_arguments_editor,
            action_name_to_static,
            selected_action_name: None,
            error: None,
            keymap_editor,
            workspace,
            focus_state,
        }
    }

    fn add_action_arguments_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action_editor) = &self.action_editor else {
            return;
        };

        let action_name_str = action_editor.read(cx).text(cx);
        let current_action = self.action_name_to_static.get(&action_name_str).copied();

        if current_action == self.selected_action_name {
            return;
        }

        self.selected_action_name = current_action;

        let Some(action_name) = current_action else {
            if self.action_arguments_editor.is_some() {
                self.action_arguments_editor = None;
                self.rebuild_focus_state(cx);
                cx.notify();
            }
            return;
        };

        let (action_has_schema, temp_dir) = {
            let keymap_editor = self.keymap_editor.read(cx);
            let has_schema = keymap_editor.actions_with_schemas.contains(action_name);
            let temp_dir = keymap_editor
                .action_args_temp_dir
                .as_ref()
                .map(|dir| dir.path().to_path_buf());
            (has_schema, temp_dir)
        };

        let currently_has_editor = self.action_arguments_editor.is_some();

        if action_has_schema && !currently_has_editor {
            let workspace = self.workspace.clone();

            let new_editor = cx.new(|cx| {
                ActionArgumentsEditor::new(
                    action_name,
                    None,
                    temp_dir.as_deref(),
                    workspace,
                    window,
                    cx,
                )
            });

            self.action_arguments_editor = Some(new_editor);
            self.rebuild_focus_state(cx);
            cx.notify();
        } else if !action_has_schema && currently_has_editor {
            self.action_arguments_editor = None;
            self.rebuild_focus_state(cx);
            cx.notify();
        }
    }

    fn rebuild_focus_state(&mut self, cx: &App) {
        self.focus_state = KeybindingEditorModalFocusState::new(
            self.action_editor.as_ref().map(|e| e.focus_handle(cx)),
            self.keybind_editor.focus_handle(cx),
            self.action_arguments_editor
                .as_ref()
                .map(|args_editor| args_editor.focus_handle(cx)),
            self.context_editor.focus_handle(cx),
        );
    }

    fn set_error(&mut self, error: InputError, cx: &mut Context<Self>) -> bool {
        if self
            .error
            .as_ref()
            .is_some_and(|old_error| old_error.severity == Severity::Warning && *old_error == error)
        {
            false
        } else {
            self.error = Some(error);
            cx.notify();
            true
        }
    }

    fn get_selected_action_name(&self, cx: &App) -> anyhow::Result<&'static str> {
        if let Some(selector) = self.action_editor.as_ref() {
            let action_name_str = selector.read(cx).text(cx);

            if action_name_str.is_empty() {
                anyhow::bail!("Action name is required");
            }

            self.action_name_to_static
                .get(&action_name_str)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("Action '{}' not found", action_name_str))
        } else {
            Ok(self.editing_keybind.action().name)
        }
    }

    fn validate_action_arguments(&self, cx: &App) -> anyhow::Result<Option<String>> {
        let action_name = self.get_selected_action_name(cx)?;
        let action_arguments = self
            .action_arguments_editor
            .as_ref()
            .map(|arguments_editor| arguments_editor.read(cx).editor.read(cx).text(cx))
            .filter(|args| !args.is_empty());

        let value = action_arguments
            .as_ref()
            .map(|args| {
                serde_json::from_str(args).context("Failed to parse action arguments as JSON")
            })
            .transpose()?;

        cx.build_action(action_name, value)
            .context("Failed to validate action arguments")?;
        Ok(action_arguments)
    }

    fn validate_keystrokes(&self, cx: &App) -> anyhow::Result<Vec<KeybindingKeystroke>> {
        let new_keystrokes = self
            .keybind_editor
            .read_with(cx, |editor, _| editor.keystrokes().to_vec());
        anyhow::ensure!(!new_keystrokes.is_empty(), "Keystrokes cannot be empty");
        Ok(new_keystrokes)
    }

    fn validate_context(&self, cx: &App) -> anyhow::Result<Option<String>> {
        let new_context = self
            .context_editor
            .read_with(cx, |input, cx| input.text(cx));
        let Some(context) = new_context.is_empty().not().then_some(new_context) else {
            return Ok(None);
        };
        gpui::KeyBindingContextPredicate::parse(&context).context("Failed to parse key context")?;

        Ok(Some(context))
    }

    fn save_or_display_error(&mut self, cx: &mut Context<Self>) {
        self.save(cx).map_err(|err| self.set_error(err, cx)).ok();
    }
}

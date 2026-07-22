use super::*;

impl KeymapFile {
    pub fn parse(content: &str) -> anyhow::Result<Self> {
        if content.trim().is_empty() {
            return Ok(Self(Vec::new()));
        }
        parse_json_with_comments::<Self>(content)
    }

    pub fn load_asset(
        asset_path: &str,
        source: Option<KeybindSource>,
        cx: &App,
    ) -> anyhow::Result<Vec<KeyBinding>> {
        match Self::load(asset_str::<SettingsAssets>(asset_path).as_ref(), cx) {
            KeymapFileLoadResult::Success { mut key_bindings } => match source {
                Some(source) => Ok({
                    for key_binding in &mut key_bindings {
                        key_binding.set_meta(source.meta());
                    }
                    key_bindings
                }),
                None => Ok(key_bindings),
            },
            KeymapFileLoadResult::SomeFailedToLoad { error_message, .. } => {
                anyhow::bail!("Error loading built-in keymap \"{asset_path}\": {error_message}",)
            }
            KeymapFileLoadResult::JsonParseFailure { error } => {
                anyhow::bail!("JSON parse error in built-in keymap \"{asset_path}\": {error}")
            }
        }
    }

    pub fn load_asset_allow_partial_failure(
        asset_path: &str,
        cx: &App,
    ) -> anyhow::Result<Vec<KeyBinding>> {
        match Self::load(asset_str::<SettingsAssets>(asset_path).as_ref(), cx) {
            KeymapFileLoadResult::SomeFailedToLoad {
                key_bindings,
                error_message,
                ..
            } if key_bindings.is_empty() => {
                anyhow::bail!("Error loading built-in keymap \"{asset_path}\": {error_message}",)
            }
            KeymapFileLoadResult::Success { key_bindings, .. }
            | KeymapFileLoadResult::SomeFailedToLoad { key_bindings, .. } => Ok(key_bindings),
            KeymapFileLoadResult::JsonParseFailure { error } => {
                anyhow::bail!("JSON parse error in built-in keymap \"{asset_path}\": {error}")
            }
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn load_asset_cached(asset_path: &str, cx: &App) -> anyhow::Result<Vec<KeyBinding>> {
        static CACHED: std::sync::OnceLock<KeymapFile> = std::sync::OnceLock::new();
        let keymap = CACHED
            .get_or_init(|| Self::parse(asset_str::<SettingsAssets>(asset_path).as_ref()).unwrap());
        match keymap.load_keymap(cx) {
            KeymapFileLoadResult::SomeFailedToLoad {
                key_bindings,
                error_message,
                ..
            } if key_bindings.is_empty() => {
                anyhow::bail!("Error loading built-in keymap \"{asset_path}\": {error_message}")
            }
            KeymapFileLoadResult::Success { key_bindings, .. }
            | KeymapFileLoadResult::SomeFailedToLoad { key_bindings, .. } => Ok(key_bindings),
            KeymapFileLoadResult::JsonParseFailure { error } => {
                anyhow::bail!("JSON parse error in built-in keymap \"{asset_path}\": {error}")
            }
        }
    }

    #[cfg(feature = "test-support")]
    pub fn load_panic_on_failure(content: &str, cx: &App) -> Vec<KeyBinding> {
        match Self::load(content, cx) {
            KeymapFileLoadResult::Success { key_bindings, .. } => key_bindings,
            KeymapFileLoadResult::SomeFailedToLoad { error_message, .. } => {
                panic!("{error_message}");
            }
            KeymapFileLoadResult::JsonParseFailure { error } => {
                panic!("JSON parse error: {error}");
            }
        }
    }

    pub fn load(content: &str, cx: &App) -> KeymapFileLoadResult {
        let keymap_file = match Self::parse(content) {
            Ok(keymap_file) => keymap_file,
            Err(error) => {
                return KeymapFileLoadResult::JsonParseFailure { error };
            }
        };
        keymap_file.load_keymap(cx)
    }

    pub fn load_keymap(&self, cx: &App) -> KeymapFileLoadResult {
        // Accumulate errors in order to support partial load of user keymap in the presence of
        // errors in context and binding parsing.
        let mut errors = Vec::new();
        let mut key_bindings = Vec::new();

        for KeymapSection {
            context,
            use_key_equivalents,
            unbind,
            bindings,
            unrecognized_fields,
        } in self.0.iter()
        {
            let context_predicate: Option<Rc<KeyBindingContextPredicate>> = if context.is_empty() {
                None
            } else {
                match KeyBindingContextPredicate::parse(context) {
                    Ok(context_predicate) => Some(context_predicate.into()),
                    Err(err) => {
                        // Leading space is to separate from the message indicating which section
                        // the error occurred in.
                        errors.push((
                            context.clone(),
                            format!(" Parse error in section `context` field: {}", err),
                        ));
                        continue;
                    }
                }
            };

            let mut section_errors = String::new();

            if !unrecognized_fields.is_empty() {
                write!(
                    section_errors,
                    "\n\n - Unrecognized fields: {}",
                    MarkdownInlineCode(&format!("{:?}", unrecognized_fields.keys()))
                )
                .unwrap();
            }

            if let Some(unbind) = unbind {
                for (keystrokes, action) in unbind {
                    let result = Self::load_unbinding(
                        keystrokes,
                        action,
                        context_predicate.clone(),
                        *use_key_equivalents,
                        cx,
                    );
                    match result {
                        Ok(key_binding) => {
                            key_bindings.push(key_binding);
                        }
                        Err(err) => {
                            let mut lines = err.lines();
                            let mut indented_err = lines.next().unwrap().to_string();
                            for line in lines {
                                indented_err.push_str("  ");
                                indented_err.push_str(line);
                                indented_err.push_str("\n");
                            }
                            write!(
                                section_errors,
                                "\n\n- In unbind {}, {indented_err}",
                                MarkdownInlineCode(&format!("\"{}\"", keystrokes))
                            )
                            .unwrap();
                        }
                    }
                }
            }

            if let Some(bindings) = bindings {
                for (keystrokes, action) in bindings {
                    let result = Self::load_keybinding(
                        keystrokes,
                        action,
                        context_predicate.clone(),
                        *use_key_equivalents,
                        cx,
                    );
                    match result {
                        Ok(key_binding) => {
                            key_bindings.push(key_binding);
                        }
                        Err(err) => {
                            let mut lines = err.lines();
                            let mut indented_err = lines.next().unwrap().to_string();
                            for line in lines {
                                indented_err.push_str("  ");
                                indented_err.push_str(line);
                                indented_err.push_str("\n");
                            }
                            write!(
                                section_errors,
                                "\n\n- In binding {}, {indented_err}",
                                MarkdownInlineCode(&format!("\"{}\"", keystrokes))
                            )
                            .unwrap();
                        }
                    }
                }
            }

            if !section_errors.is_empty() {
                errors.push((context.clone(), section_errors))
            }
        }

        if errors.is_empty() {
            KeymapFileLoadResult::Success { key_bindings }
        } else {
            let mut error_message = "Errors in user keymap file.".to_owned();

            for (context, section_errors) in errors {
                if context.is_empty() {
                    let _ = write!(error_message, "\nIn section without context predicate:");
                } else {
                    let _ = write!(
                        error_message,
                        "\nIn section with {}:",
                        MarkdownInlineCode(&format!("context = \"{}\"", context))
                    );
                }
                let _ = write!(error_message, "{section_errors}");
            }

            KeymapFileLoadResult::SomeFailedToLoad {
                key_bindings,
                error_message: MarkdownString(error_message),
            }
        }
    }

    fn load_keybinding(
        keystrokes: &str,
        action: &KeymapAction,
        context: Option<Rc<KeyBindingContextPredicate>>,
        use_key_equivalents: bool,
        cx: &App,
    ) -> std::result::Result<KeyBinding, String> {
        Self::load_keybinding_action_value(keystrokes, &action.0, context, use_key_equivalents, cx)
    }

    fn load_keybinding_action_value(
        keystrokes: &str,
        action: &Value,
        context: Option<Rc<KeyBindingContextPredicate>>,
        use_key_equivalents: bool,
        cx: &App,
    ) -> std::result::Result<KeyBinding, String> {
        let (action, action_input_string) = Self::build_keymap_action_value(action, cx)?;

        let key_binding = match KeyBinding::load(
            keystrokes,
            action,
            context,
            use_key_equivalents,
            action_input_string.map(SharedString::from),
            cx.keyboard_mapper().as_ref(),
        ) {
            Ok(key_binding) => key_binding,
            Err(InvalidKeystrokeError { keystroke }) => {
                return Err(format!(
                    "invalid keystroke {}. {}",
                    MarkdownInlineCode(&format!("\"{}\"", &keystroke)),
                    KEYSTROKE_PARSE_EXPECTED_MESSAGE
                ));
            }
        };

        if let Some(validator) = KEY_BINDING_VALIDATORS.get(&key_binding.action().type_id()) {
            match validator.validate(&key_binding) {
                Ok(()) => Ok(key_binding),
                Err(error) => Err(error.0),
            }
        } else {
            Ok(key_binding)
        }
    }

    fn load_unbinding(
        keystrokes: &str,
        action: &UnbindTargetAction,
        context: Option<Rc<KeyBindingContextPredicate>>,
        use_key_equivalents: bool,
        cx: &App,
    ) -> std::result::Result<KeyBinding, String> {
        let key_binding = Self::load_keybinding_action_value(
            keystrokes,
            &action.0,
            context,
            use_key_equivalents,
            cx,
        )?;

        if key_binding.action().partial_eq(&NoAction) {
            return Err("expected action name string or [name, input] array.".to_string());
        }

        if key_binding.action().name() == Unbind::name_for_type() {
            return Err(format!(
                "can't use {} as an unbind target.",
                MarkdownInlineCode(&format!("\"{}\"", Unbind::name_for_type()))
            ));
        }

        KeyBinding::load(
            keystrokes,
            Box::new(Unbind(key_binding.action().name().into())),
            key_binding.predicate(),
            use_key_equivalents,
            key_binding.action_input(),
            cx.keyboard_mapper().as_ref(),
        )
        .map_err(|InvalidKeystrokeError { keystroke }| {
            format!(
                "invalid keystroke {}. {}",
                MarkdownInlineCode(&format!("\"{}\"", &keystroke)),
                KEYSTROKE_PARSE_EXPECTED_MESSAGE
            )
        })
    }

    pub fn parse_action(
        action: &KeymapAction,
    ) -> Result<Option<(&String, Option<&Value>)>, String> {
        Self::parse_action_value(&action.0)
    }

    fn parse_action_value(action: &Value) -> Result<Option<(&String, Option<&Value>)>, String> {
        let name_and_input = match action {
            Value::Array(items) => {
                if items.len() != 2 {
                    return Err(format!(
                        "expected two-element array of `[name, input]`. \
                        Instead found {}.",
                        MarkdownInlineCode(&action.to_string())
                    ));
                }
                let serde_json::Value::String(ref name) = items[0] else {
                    return Err(format!(
                        "expected two-element array of `[name, input]`, \
                        but the first element is not a string in {}.",
                        MarkdownInlineCode(&action.to_string())
                    ));
                };
                Some((name, Some(&items[1])))
            }
            Value::String(name) => Some((name, None)),
            Value::Null => None,
            _ => {
                return Err(format!(
                    "expected two-element array of `[name, input]`. \
                    Instead found {}.",
                    MarkdownInlineCode(&action.to_string())
                ));
            }
        };
        Ok(name_and_input)
    }
}

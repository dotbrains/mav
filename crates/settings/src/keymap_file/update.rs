use super::*;

impl KeymapFile {
    pub async fn load_keymap_file(fs: &Arc<dyn Fs>) -> Result<String> {
        match fs.load(paths::keymap_file()).await {
            result @ Ok(_) => result,
            Err(err) => {
                if let Some(e) = err.downcast_ref::<std::io::Error>()
                    && e.kind() == std::io::ErrorKind::NotFound
                {
                    return Ok(crate::initial_keymap_content().to_string());
                }
                Err(err)
            }
        }
    }

    pub fn update_keybinding<'a>(
        mut operation: KeybindUpdateOperation<'a>,
        mut keymap_contents: String,
        tab_size: usize,
        keyboard_mapper: &dyn gpui::PlatformKeyboardMapper,
    ) -> Result<String> {
        // When replacing or removing a non-user binding, we may need to write an unbind entry
        // to suppress the original default binding.
        let mut suppression_unbind: Option<KeybindUpdateTarget<'_>> = None;

        match &operation {
            // if trying to replace a keybinding that is not user-defined, treat it as an add operation
            KeybindUpdateOperation::Replace {
                target_keybind_source: target_source,
                source,
                target,
            } if *target_source != KeybindSource::User => {
                if target.keystrokes_unparsed() != source.keystrokes_unparsed() {
                    suppression_unbind = Some(target.clone());
                }
                operation = KeybindUpdateOperation::Add {
                    source: source.clone(),
                    from: Some(target.clone()),
                };
            }
            // if trying to remove a keybinding that is not user-defined, treat it as creating an
            // unbind entry for the removed action
            KeybindUpdateOperation::Remove {
                target,
                target_keybind_source,
            } if *target_keybind_source != KeybindSource::User => {
                suppression_unbind = Some(target.clone());
            }
            _ => {}
        }

        // Sanity check that keymap contents are valid, even though we only use it for Replace.
        // We don't want to modify the file if it's invalid.
        let keymap = Self::parse(&keymap_contents).context("Failed to parse keymap")?;

        if let KeybindUpdateOperation::Remove {
            target,
            target_keybind_source,
        } = &operation
        {
            if *target_keybind_source == KeybindSource::User {
                let target_action_value = target
                    .action_value()
                    .context("Failed to generate target action JSON value")?;
                let Some(binding_location) =
                    find_binding(&keymap, target, &target_action_value, keyboard_mapper)
                else {
                    anyhow::bail!("Failed to find keybinding to remove");
                };
                let is_only_binding = binding_location.is_only_entry_in_section(&keymap);
                let key_path: &[&str] = if is_only_binding {
                    &[]
                } else {
                    &[
                        binding_location.kind.key_path(),
                        binding_location.keystrokes_str,
                    ]
                };
                let (replace_range, replace_value) = replace_top_level_array_value_in_json_text(
                    &keymap_contents,
                    key_path,
                    None,
                    None,
                    binding_location.index,
                    tab_size,
                );
                keymap_contents.replace_range(replace_range, &replace_value);

                return Ok(keymap_contents);
            }
        }

        if let KeybindUpdateOperation::Replace { source, target, .. } = operation {
            let target_action_value = target
                .action_value()
                .context("Failed to generate target action JSON value")?;
            let source_action_value = source
                .action_value()
                .context("Failed to generate source action JSON value")?;

            if let Some(binding_location) =
                find_binding(&keymap, &target, &target_action_value, keyboard_mapper)
            {
                if target.context == source.context {
                    // if we are only changing the keybinding (common case)
                    // not the context, etc. Then just update the binding in place

                    let (replace_range, replace_value) = replace_top_level_array_value_in_json_text(
                        &keymap_contents,
                        &[
                            binding_location.kind.key_path(),
                            binding_location.keystrokes_str,
                        ],
                        Some(&source_action_value),
                        Some(&source.keystrokes_unparsed()),
                        binding_location.index,
                        tab_size,
                    );
                    keymap_contents.replace_range(replace_range, &replace_value);

                    return Ok(keymap_contents);
                } else if binding_location.is_only_entry_in_section(&keymap) {
                    // if we are replacing the only binding in the section,
                    // just update the section in place, updating the context
                    // and the binding

                    let (replace_range, replace_value) = replace_top_level_array_value_in_json_text(
                        &keymap_contents,
                        &[
                            binding_location.kind.key_path(),
                            binding_location.keystrokes_str,
                        ],
                        Some(&source_action_value),
                        Some(&source.keystrokes_unparsed()),
                        binding_location.index,
                        tab_size,
                    );
                    keymap_contents.replace_range(replace_range, &replace_value);

                    let (replace_range, replace_value) = replace_top_level_array_value_in_json_text(
                        &keymap_contents,
                        &["context"],
                        source.context.map(Into::into).as_ref(),
                        None,
                        binding_location.index,
                        tab_size,
                    );
                    keymap_contents.replace_range(replace_range, &replace_value);
                    return Ok(keymap_contents);
                } else {
                    // if we are replacing one of multiple bindings in a section
                    // with a context change, remove the existing binding from the
                    // section, then treat this operation as an add operation of the
                    // new binding with the updated context.

                    let (replace_range, replace_value) = replace_top_level_array_value_in_json_text(
                        &keymap_contents,
                        &[
                            binding_location.kind.key_path(),
                            binding_location.keystrokes_str,
                        ],
                        None,
                        None,
                        binding_location.index,
                        tab_size,
                    );
                    keymap_contents.replace_range(replace_range, &replace_value);
                    operation = KeybindUpdateOperation::Add {
                        source,
                        from: Some(target),
                    };
                }
            } else {
                log::warn!(
                    "Failed to find keybinding to update `{:?} -> {}` creating new binding for `{:?} -> {}` instead",
                    target.keystrokes,
                    target_action_value,
                    source.keystrokes,
                    source_action_value,
                );
                operation = KeybindUpdateOperation::Add {
                    source,
                    from: Some(target),
                };
            }
        }

        if let KeybindUpdateOperation::Add {
            source: keybinding,
            from,
        } = operation
        {
            let mut value = serde_json::Map::with_capacity(4);
            if let Some(context) = keybinding.context {
                value.insert("context".to_string(), context.into());
            }
            let use_key_equivalents = from.and_then(|from| {
                let action_value = from.action_value().context("Failed to serialize action value. `use_key_equivalents` on new keybinding may be incorrect.").log_err()?;
                let binding_location =
                    find_binding(&keymap, &from, &action_value, keyboard_mapper)?;
                Some(keymap.0[binding_location.index].use_key_equivalents)
            }).unwrap_or(false);
            if use_key_equivalents {
                value.insert("use_key_equivalents".to_string(), true.into());
            }

            value.insert("bindings".to_string(), {
                let mut bindings = serde_json::Map::new();
                let action = keybinding.action_value()?;
                bindings.insert(keybinding.keystrokes_unparsed(), action);
                bindings.into()
            });

            let (replace_range, replace_value) = append_top_level_array_value_in_json_text(
                &keymap_contents,
                &value.into(),
                tab_size,
            );
            keymap_contents.replace_range(replace_range, &replace_value);
        }

        if let Some(suppression_unbind) = suppression_unbind {
            let mut value = serde_json::Map::with_capacity(2);
            if let Some(context) = suppression_unbind.context {
                value.insert("context".to_string(), context.into());
            }
            value.insert("unbind".to_string(), {
                let mut unbind = serde_json::Map::new();
                unbind.insert(
                    suppression_unbind.keystrokes_unparsed(),
                    suppression_unbind.action_value()?,
                );
                unbind.into()
            });
            let (replace_range, replace_value) = append_top_level_array_value_in_json_text(
                &keymap_contents,
                &value.into(),
                tab_size,
            );
            keymap_contents.replace_range(replace_range, &replace_value);
        }

        return Ok(keymap_contents);

        fn find_binding<'a, 'b>(
            keymap: &'b KeymapFile,
            target: &KeybindUpdateTarget<'a>,
            target_action_value: &Value,
            keyboard_mapper: &dyn gpui::PlatformKeyboardMapper,
        ) -> Option<BindingLocation<'b>> {
            let target_context_parsed =
                KeyBindingContextPredicate::parse(target.context.unwrap_or("")).ok();
            for (index, section) in keymap.sections().enumerate() {
                let section_context_parsed =
                    KeyBindingContextPredicate::parse(&section.context).ok();
                if section_context_parsed != target_context_parsed {
                    continue;
                }

                if let Some(binding_location) = find_binding_in_entries(
                    section.bindings.as_ref(),
                    BindingKind::Binding,
                    index,
                    target,
                    target_action_value,
                    keyboard_mapper,
                    |action| &action.0,
                ) {
                    return Some(binding_location);
                }

                if let Some(binding_location) = find_binding_in_entries(
                    section.unbind.as_ref(),
                    BindingKind::Unbind,
                    index,
                    target,
                    target_action_value,
                    keyboard_mapper,
                    |action| &action.0,
                ) {
                    return Some(binding_location);
                }
            }
            None
        }

        fn find_binding_in_entries<'a, 'b, T>(
            entries: Option<&'b IndexMap<String, T>>,
            kind: BindingKind,
            index: usize,
            target: &KeybindUpdateTarget<'a>,
            target_action_value: &Value,
            keyboard_mapper: &dyn gpui::PlatformKeyboardMapper,
            action_value: impl Fn(&T) -> &Value,
        ) -> Option<BindingLocation<'b>> {
            let entries = entries?;
            for (keystrokes_str, action) in entries {
                let Ok(keystrokes) = keystrokes_str
                    .split_whitespace()
                    .map(|source| {
                        let keystroke = Keystroke::parse(source)?;
                        Ok(KeybindingKeystroke::new_with_mapper(
                            keystroke,
                            false,
                            keyboard_mapper,
                        ))
                    })
                    .collect::<Result<Vec<_>, InvalidKeystrokeError>>()
                else {
                    continue;
                };
                if keystrokes.len() != target.keystrokes.len()
                    || !keystrokes
                        .iter()
                        .zip(target.keystrokes)
                        .all(|(a, b)| a.inner().should_match(b))
                {
                    continue;
                }
                if action_value(action) != target_action_value {
                    continue;
                }
                return Some(BindingLocation {
                    index,
                    kind,
                    keystrokes_str,
                });
            }
            None
        }

        #[derive(Copy, Clone)]
        enum BindingKind {
            Binding,
            Unbind,
        }

        impl BindingKind {
            fn key_path(self) -> &'static str {
                match self {
                    Self::Binding => "bindings",
                    Self::Unbind => "unbind",
                }
            }
        }

        struct BindingLocation<'a> {
            index: usize,
            kind: BindingKind,
            keystrokes_str: &'a str,
        }

        impl BindingLocation<'_> {
            fn is_only_entry_in_section(&self, keymap: &KeymapFile) -> bool {
                let section = &keymap.0[self.index];
                let binding_count = section.bindings.as_ref().map_or(0, IndexMap::len);
                let unbind_count = section.unbind.as_ref().map_or(0, IndexMap::len);
                binding_count + unbind_count == 1
            }
        }
    }
}

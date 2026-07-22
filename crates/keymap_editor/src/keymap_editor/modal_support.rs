use super::*;

pub(super) struct KeyContextCompletionProvider {
    pub(super) contexts: Vec<SharedString>,
}

impl CompletionProvider for KeyContextCompletionProvider {
    fn completions(
        &self,
        buffer: &Entity<language::Buffer>,
        buffer_position: language::Anchor,
        _trigger: editor::CompletionContext,
        _window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> gpui::Task<anyhow::Result<Vec<project::CompletionResponse>>> {
        let buffer = buffer.read(cx);
        let mut count_back = 0;
        for char in buffer.reversed_chars_at(buffer_position) {
            if char.is_ascii_alphanumeric() || char == '_' {
                count_back += 1;
            } else {
                break;
            }
        }
        let start_anchor =
            buffer.anchor_before(buffer_position.to_offset(buffer).saturating_sub(count_back));
        let replace_range = start_anchor..buffer_position;
        gpui::Task::ready(Ok(vec![project::CompletionResponse {
            completions: self
                .contexts
                .iter()
                .map(|context| project::Completion {
                    replace_range: replace_range.clone(),
                    label: language::CodeLabel::plain(context.to_string(), None),
                    new_text: context.to_string(),
                    documentation: None,
                    source: project::CompletionSource::Custom,
                    icon_path: None,
                    icon_color: None,
                    match_start: None,
                    snippet_deduplication_key: None,
                    insert_text_mode: None,
                    confirm: None,
                    group: None,
                })
                .collect(),
            display_options: CompletionDisplayOptions::default(),
            is_incomplete: false,
        }]))
    }

    fn is_completion_trigger(
        &self,
        _buffer: &Entity<language::Buffer>,
        _position: language::Anchor,
        text: &str,
        _trigger_in_words: bool,
        _cx: &mut Context<Editor>,
    ) -> bool {
        text.chars()
            .last()
            .is_some_and(|last_char| last_char.is_ascii_alphanumeric() || last_char == '_')
    }
}

pub(super) async fn load_json_language(
    workspace: WeakEntity<Workspace>,
    cx: &mut AsyncApp,
) -> Arc<Language> {
    let json_language_task = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .project()
                .read(cx)
                .languages()
                .language_for_name("JSON")
        })
        .context("Failed to load JSON language")
        .log_err();
    let json_language = match json_language_task {
        Some(task) => task.await.context("Failed to load JSON language").log_err(),
        None => None,
    };
    json_language.unwrap_or_else(|| {
        Arc::new(Language::new(
            LanguageConfig {
                name: "JSON".into(),
                ..Default::default()
            },
            Some(tree_sitter_json::LANGUAGE.into()),
        ))
    })
}

pub(super) async fn load_keybind_context_language(
    workspace: WeakEntity<Workspace>,
    cx: &mut AsyncApp,
) -> Arc<Language> {
    let language_task = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .project()
                .read(cx)
                .languages()
                .language_for_name("Mav Keybind Context")
        })
        .context("Failed to load Mav Keybind Context language")
        .log_err();
    let language = match language_task {
        Some(task) => task
            .await
            .context("Failed to load Mav Keybind Context language")
            .log_err(),
        None => None,
    };
    language.unwrap_or_else(|| {
        Arc::new(Language::new(
            LanguageConfig {
                name: "Mav Keybind Context".into(),
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        ))
    })
}

pub(super) async fn save_keybinding_update(
    create: bool,
    existing: ProcessedBinding,
    action_mapping: &ActionMapping,
    new_args: Option<&str>,
    fs: &Arc<dyn Fs>,
    keyboard_mapper: &dyn PlatformKeyboardMapper,
) -> anyhow::Result<()> {
    let keymap_contents = settings::KeymapFile::load_keymap_file(fs)
        .await
        .context("Failed to load keymap file")?;

    let tab_size = infer_json_indent_size(&keymap_contents);

    let existing_keystrokes = existing.keystrokes().unwrap_or_default();
    let existing_context = existing.context().and_then(KeybindContextString::local_str);
    let existing_args = existing
        .action()
        .arguments
        .as_ref()
        .map(|args| args.text.as_ref());

    let target = settings::KeybindUpdateTarget {
        context: existing_context,
        keystrokes: existing_keystrokes,
        action_name: existing.action().name,
        action_arguments: existing_args,
    };

    let source = settings::KeybindUpdateTarget {
        context: action_mapping.context.as_deref(),
        keystrokes: &action_mapping.keystrokes,
        action_name: existing.action().name,
        action_arguments: new_args,
    };

    let operation = if !create {
        settings::KeybindUpdateOperation::Replace {
            target,
            target_keybind_source: existing.keybind_source().unwrap_or(KeybindSource::User),
            source,
        }
    } else {
        settings::KeybindUpdateOperation::Add {
            source,
            from: Some(target),
        }
    };

    let (new_keybinding, removed_keybinding, source) = operation.generate_telemetry();

    let updated_keymap_contents = settings::KeymapFile::update_keybinding(
        operation,
        keymap_contents,
        tab_size,
        keyboard_mapper,
    )
    .map_err(|err| anyhow::anyhow!("Could not save updated keybinding: {}", err))?;
    fs.write(
        paths::keymap_file().as_path(),
        updated_keymap_contents.as_bytes(),
    )
    .await
    .context("Failed to write keymap file")?;

    telemetry::event!(
        "Keybinding Updated",
        new_keybinding = new_keybinding,
        removed_keybinding = removed_keybinding,
        source = source
    );
    Ok(())
}

pub(super) async fn remove_keybinding(
    existing: ProcessedBinding,
    fs: &Arc<dyn Fs>,
    keyboard_mapper: &dyn PlatformKeyboardMapper,
) -> anyhow::Result<()> {
    let Some(keystrokes) = existing.keystrokes() else {
        anyhow::bail!("Cannot remove a keybinding that does not exist");
    };
    let keymap_contents = settings::KeymapFile::load_keymap_file(fs)
        .await
        .context("Failed to load keymap file")?;
    let tab_size = infer_json_indent_size(&keymap_contents);

    let operation = settings::KeybindUpdateOperation::Remove {
        target: settings::KeybindUpdateTarget {
            context: existing.context().and_then(KeybindContextString::local_str),
            keystrokes,
            action_name: existing.action().name,
            action_arguments: existing
                .action()
                .arguments
                .as_ref()
                .map(|arguments| arguments.text.as_ref()),
        },
        target_keybind_source: existing.keybind_source().unwrap_or(KeybindSource::User),
    };

    let (new_keybinding, removed_keybinding, source) = operation.generate_telemetry();
    let updated_keymap_contents = settings::KeymapFile::update_keybinding(
        operation,
        keymap_contents,
        tab_size,
        keyboard_mapper,
    )
    .context("Failed to update keybinding")?;
    fs.write(
        paths::keymap_file().as_path(),
        updated_keymap_contents.as_bytes(),
    )
    .await
    .context("Failed to write keymap file")?;

    telemetry::event!(
        "Keybinding Removed",
        new_keybinding = new_keybinding,
        removed_keybinding = removed_keybinding,
        source = source
    );
    Ok(())
}

pub(super) fn collect_contexts_from_assets() -> Vec<SharedString> {
    let mut keymap_assets = vec![
        util::asset_str::<SettingsAssets>(settings::DEFAULT_KEYMAP_PATH),
        util::asset_str::<SettingsAssets>(settings::VIM_KEYMAP_PATH),
    ];
    keymap_assets.extend(
        BaseKeymap::OPTIONS
            .iter()
            .filter_map(|(_, base_keymap)| base_keymap.asset_path())
            .map(util::asset_str::<SettingsAssets>),
    );

    let mut contexts = HashSet::default();

    for keymap_asset in keymap_assets {
        let Ok(keymap) = KeymapFile::parse(&keymap_asset) else {
            continue;
        };

        for section in keymap.sections() {
            let context_expr = &section.context;
            let mut queue = Vec::new();
            let Ok(root_context) = gpui::KeyBindingContextPredicate::parse(context_expr) else {
                continue;
            };

            queue.push(root_context);
            while let Some(context) = queue.pop() {
                match context {
                    Identifier(ident) => {
                        contexts.insert(ident);
                    }
                    Equal(ident_a, ident_b) => {
                        contexts.insert(ident_a);
                        contexts.insert(ident_b);
                    }
                    NotEqual(ident_a, ident_b) => {
                        contexts.insert(ident_a);
                        contexts.insert(ident_b);
                    }
                    Descendant(ctx_a, ctx_b) => {
                        queue.push(*ctx_a);
                        queue.push(*ctx_b);
                    }
                    Not(ctx) => {
                        queue.push(*ctx);
                    }
                    And(ctx_a, ctx_b) => {
                        queue.push(*ctx_a);
                        queue.push(*ctx_b);
                    }
                    Or(ctx_a, ctx_b) => {
                        queue.push(*ctx_a);
                        queue.push(*ctx_b);
                    }
                }
            }
        }
    }

    let mut contexts = contexts.into_iter().collect::<Vec<_>>();
    contexts.sort();

    contexts
}

pub(super) fn normalized_ctx_eq(
    a: &gpui::KeyBindingContextPredicate,
    b: &gpui::KeyBindingContextPredicate,
) -> bool {
    use gpui::KeyBindingContextPredicate::*;
    return match (a, b) {
        (Identifier(_), Identifier(_)) => a == b,
        (Equal(a_left, a_right), Equal(b_left, b_right)) => {
            (a_left == b_left && a_right == b_right) || (a_left == b_right && a_right == b_left)
        }
        (NotEqual(a_left, a_right), NotEqual(b_left, b_right)) => {
            (a_left == b_left && a_right == b_right) || (a_left == b_right && a_right == b_left)
        }
        (Descendant(a_parent, a_child), Descendant(b_parent, b_child)) => {
            normalized_ctx_eq(a_parent, b_parent) && normalized_ctx_eq(a_child, b_child)
        }
        (Not(a_expr), Not(b_expr)) => normalized_ctx_eq(a_expr, b_expr),
        // Handle double negation: !(!a) == a
        (Not(a_expr), b) if matches!(a_expr.as_ref(), Not(_)) => {
            let Not(a_inner) = a_expr.as_ref() else {
                unreachable!();
            };
            normalized_ctx_eq(b, a_inner)
        }
        (a, Not(b_expr)) if matches!(b_expr.as_ref(), Not(_)) => {
            let Not(b_inner) = b_expr.as_ref() else {
                unreachable!();
            };
            normalized_ctx_eq(a, b_inner)
        }
        (And(a_left, a_right), And(b_left, b_right))
            if matches!(a_left.as_ref(), And(_, _))
                || matches!(a_right.as_ref(), And(_, _))
                || matches!(b_left.as_ref(), And(_, _))
                || matches!(b_right.as_ref(), And(_, _)) =>
        {
            let mut a_operands = Vec::new();
            flatten_and(a, &mut a_operands);
            let mut b_operands = Vec::new();
            flatten_and(b, &mut b_operands);
            compare_operand_sets(&a_operands, &b_operands)
        }
        (And(a_left, a_right), And(b_left, b_right)) => {
            (normalized_ctx_eq(a_left, b_left) && normalized_ctx_eq(a_right, b_right))
                || (normalized_ctx_eq(a_left, b_right) && normalized_ctx_eq(a_right, b_left))
        }
        (Or(a_left, a_right), Or(b_left, b_right))
            if matches!(a_left.as_ref(), Or(_, _))
                || matches!(a_right.as_ref(), Or(_, _))
                || matches!(b_left.as_ref(), Or(_, _))
                || matches!(b_right.as_ref(), Or(_, _)) =>
        {
            let mut a_operands = Vec::new();
            flatten_or(a, &mut a_operands);
            let mut b_operands = Vec::new();
            flatten_or(b, &mut b_operands);
            compare_operand_sets(&a_operands, &b_operands)
        }
        (Or(a_left, a_right), Or(b_left, b_right)) => {
            (normalized_ctx_eq(a_left, b_left) && normalized_ctx_eq(a_right, b_right))
                || (normalized_ctx_eq(a_left, b_right) && normalized_ctx_eq(a_right, b_left))
        }
        _ => false,
    };

    fn flatten_and<'a>(
        pred: &'a gpui::KeyBindingContextPredicate,
        operands: &mut Vec<&'a gpui::KeyBindingContextPredicate>,
    ) {
        use gpui::KeyBindingContextPredicate::*;
        match pred {
            And(left, right) => {
                flatten_and(left, operands);
                flatten_and(right, operands);
            }
            _ => operands.push(pred),
        }
    }

    fn flatten_or<'a>(
        pred: &'a gpui::KeyBindingContextPredicate,
        operands: &mut Vec<&'a gpui::KeyBindingContextPredicate>,
    ) {
        use gpui::KeyBindingContextPredicate::*;
        match pred {
            Or(left, right) => {
                flatten_or(left, operands);
                flatten_or(right, operands);
            }
            _ => operands.push(pred),
        }
    }

    fn compare_operand_sets(
        a: &[&gpui::KeyBindingContextPredicate],
        b: &[&gpui::KeyBindingContextPredicate],
    ) -> bool {
        if a.len() != b.len() {
            return false;
        }

        // For each operand in a, find a matching operand in b
        let mut b_matched = vec![false; b.len()];
        for a_operand in a {
            let mut found = false;
            for (b_idx, b_operand) in b.iter().enumerate() {
                if !b_matched[b_idx] && normalized_ctx_eq(a_operand, b_operand) {
                    b_matched[b_idx] = true;
                    found = true;
                    break;
                }
            }
            if !found {
                return false;
            }
        }

        true
    }
}

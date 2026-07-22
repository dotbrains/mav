use super::*;

impl KeymapEditor {
    fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _keymap_subscription =
            cx.observe_global_in::<KeymapEventChannel>(window, Self::on_keymap_changed);
        let table_interaction_state = cx.new(|cx| {
            TableInteractionState::new(cx).with_custom_scrollbar(ui::Scrollbars::for_settings::<
                editor::EditorSettingsScrollbarProxy,
            >())
        });

        let keystroke_editor = cx.new(|cx| {
            let mut keystroke_editor = KeystrokeInput::new(None, window, cx);
            keystroke_editor.set_search(true);
            keystroke_editor
        });

        let filter_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Filter action names…", window, cx);
            editor
        });

        cx.subscribe(&filter_editor, |this, _, e: &EditorEvent, cx| {
            if !matches!(e, EditorEvent::BufferEdited) {
                return;
            }

            this.on_query_changed(cx);
        })
        .detach();

        cx.subscribe(&keystroke_editor, |this, _, _, cx| {
            if matches!(this.search_mode, SearchMode::Normal) {
                return;
            }

            this.on_query_changed(cx);
        })
        .detach();

        cx.spawn({
            let workspace = workspace.clone();
            async move |this, cx| {
                let temp_dir = tempfile::tempdir_in(paths::temp_dir())?;
                let worktree = workspace
                    .update(cx, |ws, cx| {
                        ws.project()
                            .update(cx, |p, cx| p.create_worktree(temp_dir.path(), false, cx))
                    })?
                    .await?;
                this.update(cx, |this, _| {
                    this.action_args_temp_dir = Some(temp_dir);
                    this.action_args_temp_dir_worktree = Some(worktree);
                })
            }
        })
        .detach();

        let mut this = Self {
            workspace,
            keybindings: vec![],
            keybinding_conflict_state: ConflictState::default(),
            filter_state: FilterState::default(),
            source_filters: SourceFilters {
                user: true,
                mav_defaults: true,
                vim_defaults: true,
            },
            show_no_action_bindings: true,
            search_mode: SearchMode::default(),
            string_match_candidates: Arc::new(vec![]),
            matches: vec![],
            focus_handle: cx.focus_handle(),
            _keymap_subscription,
            table_interaction_state,
            filter_editor,
            keystroke_editor,
            selected_index: None,
            context_menu: None,
            previous_edit: None,
            search_query_debounce: None,
            humanized_action_names: HumanizedActionNameCache::new(cx),
            show_hover_menus: true,
            actions_with_schemas: HashSet::default(),
            action_args_temp_dir: None,
            action_args_temp_dir_worktree: None,
            current_widths: cx.new(|_cx| {
                RedistributableColumnsState::new(
                    COLS,
                    vec![
                        DefiniteLength::Absolute(AbsoluteLength::Pixels(px(36.))),
                        DefiniteLength::Fraction(0.25),
                        DefiniteLength::Fraction(0.20),
                        DefiniteLength::Fraction(0.14),
                        DefiniteLength::Fraction(0.45),
                        DefiniteLength::Fraction(0.08),
                    ],
                    vec![
                        TableResizeBehavior::None,
                        TableResizeBehavior::Resizable,
                        TableResizeBehavior::Resizable,
                        TableResizeBehavior::Resizable,
                        TableResizeBehavior::Resizable,
                        TableResizeBehavior::Resizable,
                    ],
                )
            }),
        };

        this.on_keymap_changed(window, cx);

        this
    }

    fn current_action_query(&self, cx: &App) -> String {
        self.filter_editor.read(cx).text(cx)
    }

    fn current_keystroke_query(&self, cx: &App) -> Vec<KeybindingKeystroke> {
        match self.search_mode {
            SearchMode::KeyStroke { .. } => self.keystroke_editor.read(cx).keystrokes().to_vec(),
            SearchMode::Normal => Default::default(),
        }
    }

    fn clear_action_query(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter_editor
            .update(cx, |editor, cx| editor.clear(window, cx))
    }

    fn on_query_changed(&mut self, cx: &mut Context<Self>) {
        let action_query = self.current_action_query(cx);
        let keystroke_query = self.current_keystroke_query(cx);
        let exact_match = self.search_mode.exact_match();

        let timer = cx.background_executor().timer(Duration::from_secs(1));
        self.search_query_debounce = Some(cx.background_spawn({
            let action_query = action_query.clone();
            let keystroke_query = keystroke_query.clone();
            async move {
                timer.await;

                let keystroke_query = keystroke_query
                    .into_iter()
                    .map(|keystroke| keystroke.inner().unparse())
                    .collect::<Vec<String>>()
                    .join(" ");

                telemetry::event!(
                    "Keystroke Search Completed",
                    action_query = action_query,
                    keystroke_query = keystroke_query,
                    keystroke_exact_match = exact_match
                )
            }
        }));
        cx.spawn(async move |this, cx| {
            Self::update_matches(this.clone(), action_query, keystroke_query, cx).await?;
            this.update(cx, |this, cx| {
                this.scroll_to_item(0, ScrollStrategy::Top, cx)
            })
        })
        .detach();
    }

    async fn update_matches(
        this: WeakEntity<Self>,
        action_query: String,
        keystroke_query: Vec<KeybindingKeystroke>,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<()> {
        let action_query = command_palette::normalize_action_query(&action_query);
        let (string_match_candidates, keybind_count) = this.read_with(cx, |this, _| {
            (this.string_match_candidates.clone(), this.keybindings.len())
        })?;
        let executor = cx.background_executor().clone();
        let mut matches = fuzzy::match_strings(
            &string_match_candidates,
            &action_query,
            true,
            true,
            keybind_count,
            &Default::default(),
            executor,
        )
        .await;
        this.update(cx, |this, cx| {
            matches.retain(|candidate| {
                this.source_filters
                    .allows(this.keybindings[candidate.candidate_id].keybind_source())
            });

            match this.filter_state {
                FilterState::Conflicts => {
                    matches.retain(|candidate| {
                        this.keybinding_conflict_state
                            .has_user_conflict(candidate.candidate_id)
                    });
                }
                FilterState::All => {}
            }

            match this.search_mode {
                SearchMode::KeyStroke { exact_match } => {
                    matches.retain(|item| {
                        this.keybindings[item.candidate_id]
                            .keystrokes()
                            .is_some_and(|keystrokes| {
                                if exact_match {
                                    keystrokes_match_exactly(&keystroke_query, keystrokes)
                                } else if keystroke_query.len() > keystrokes.len() {
                                    false
                                } else {
                                    for keystroke_offset in 0..keystrokes.len() {
                                        let mut found_count = 0;
                                        let mut query_cursor = 0;
                                        let mut keystroke_cursor = keystroke_offset;
                                        while query_cursor < keystroke_query.len()
                                            && keystroke_cursor < keystrokes.len()
                                        {
                                            let query = &keystroke_query[query_cursor];
                                            let keystroke = &keystrokes[keystroke_cursor];
                                            let matches = query
                                                .inner()
                                                .modifiers
                                                .is_subset_of(&keystroke.inner().modifiers)
                                                && ((query.inner().key.is_empty()
                                                    || query.inner().key == keystroke.inner().key)
                                                    && query.inner().key_char.as_ref().is_none_or(
                                                        |q_kc| q_kc == &keystroke.inner().key,
                                                    ));
                                            if matches {
                                                found_count += 1;
                                                query_cursor += 1;
                                            }
                                            keystroke_cursor += 1;
                                        }

                                        if found_count == keystroke_query.len() {
                                            return true;
                                        }
                                    }
                                    false
                                }
                            })
                    });
                }
                SearchMode::Normal => {}
            }

            if !this.show_no_action_bindings {
                matches.retain(|item| !this.keybindings[item.candidate_id].is_no_action());
            }

            if action_query.is_empty() {
                matches.sort_by(|item1, item2| {
                    let binding1 = &this.keybindings[item1.candidate_id];
                    let binding2 = &this.keybindings[item2.candidate_id];

                    binding1.cmp(binding2)
                });
            }
            this.selected_index.take();
            this.matches = matches;

            cx.notify();
        })
    }

    fn get_conflict(&self, row_index: usize) -> Option<ConflictOrigin> {
        self.matches.get(row_index).and_then(|candidate| {
            self.keybinding_conflict_state
                .conflict_for_idx(candidate.candidate_id)
        })
    }

    fn process_bindings(
        json_language: Arc<Language>,
        mav_keybind_context_language: Arc<Language>,
        humanized_action_names: &HumanizedActionNameCache,
        cx: &mut App,
    ) -> (
        Vec<ProcessedBinding>,
        Vec<StringMatchCandidate>,
        HashSet<&'static str>,
    ) {
        let key_bindings_ptr = cx.key_bindings();
        let lock = key_bindings_ptr.borrow();
        let key_bindings = lock.bindings().collect::<Vec<_>>();
        let mut unmapped_action_names = HashSet::from_iter(cx.all_action_names().iter().copied());
        let action_documentation = cx.action_documentation();
        let mut generator = KeymapFile::action_schema_generator();
        let actions_with_schemas = HashSet::from_iter(
            cx.action_schemas(&mut generator)
                .into_iter()
                .filter_map(|(name, schema)| schema.is_some().then_some(name)),
        );

        let mut processed_bindings = Vec::new();
        let mut string_match_candidates = Vec::new();

        for (binding_index, &key_binding) in key_bindings.iter().enumerate() {
            if gpui::is_unbind(key_binding.action()) {
                continue;
            }

            let source = key_binding
                .meta()
                .map(KeybindSource::from_meta)
                .unwrap_or(KeybindSource::Unknown);

            let keystroke_text = ui::text_for_keybinding_keystrokes(key_binding.keystrokes(), cx);
            let is_no_action = gpui::is_no_action(key_binding.action());
            let is_unbound_by_unbind =
                binding_is_unbound_by_unbind(key_binding, binding_index, &key_bindings);
            let binding = KeyBinding::new(key_binding, source);

            let context = key_binding
                .predicate()
                .map(|predicate| {
                    KeybindContextString::Local(
                        predicate.to_string().into(),
                        mav_keybind_context_language.clone(),
                    )
                })
                .unwrap_or(KeybindContextString::Global);

            let action_name = key_binding.action().name();
            unmapped_action_names.remove(&action_name);

            let action_arguments = key_binding
                .action_input()
                .map(|arguments| SyntaxHighlightedText::new(arguments, json_language.clone()));
            let action_information = ActionInformation::new(
                action_name,
                action_arguments,
                &actions_with_schemas,
                action_documentation,
                humanized_action_names,
            );

            let index = processed_bindings.len();
            let string_match_candidate =
                StringMatchCandidate::new(index, &action_information.humanized_name);
            processed_bindings.push(ProcessedBinding::new_mapped(
                keystroke_text,
                binding,
                context,
                source,
                is_no_action,
                is_unbound_by_unbind,
                action_information,
            ));
            string_match_candidates.push(string_match_candidate);
        }

        for action_name in unmapped_action_names.into_iter() {
            let index = processed_bindings.len();
            let action_information = ActionInformation::new(
                action_name,
                None,
                &actions_with_schemas,
                action_documentation,
                humanized_action_names,
            );
            let string_match_candidate =
                StringMatchCandidate::new(index, &action_information.humanized_name);

            processed_bindings.push(ProcessedBinding::Unmapped(action_information));
            string_match_candidates.push(string_match_candidate);
        }
        (
            processed_bindings,
            string_match_candidates,
            actions_with_schemas,
        )
    }

    fn on_keymap_changed(&mut self, window: &mut Window, cx: &mut Context<KeymapEditor>) {
        let workspace = self.workspace.clone();
        cx.spawn_in(window, async move |this, cx| {
            let json_language = load_json_language(workspace.clone(), cx).await;
            let mav_keybind_context_language =
                load_keybind_context_language(workspace.clone(), cx).await;

            let (action_query, keystroke_query) = this.update(cx, |this, cx| {
                let (key_bindings, string_match_candidates, actions_with_schemas) =
                    Self::process_bindings(
                        json_language,
                        mav_keybind_context_language,
                        &this.humanized_action_names,
                        cx,
                    );

                this.keybinding_conflict_state = ConflictState::new(&key_bindings);

                this.keybindings = key_bindings;
                this.actions_with_schemas = actions_with_schemas;
                this.string_match_candidates = Arc::new(string_match_candidates);
                this.matches = this
                    .string_match_candidates
                    .iter()
                    .enumerate()
                    .map(|(ix, candidate)| StringMatch {
                        candidate_id: ix,
                        score: 0.0,
                        positions: vec![],
                        string: candidate.string.clone(),
                    })
                    .collect();
                (
                    this.current_action_query(cx),
                    this.current_keystroke_query(cx),
                )
            })?;
            // calls cx.notify
            Self::update_matches(this.clone(), action_query, keystroke_query, cx).await?;
            this.update_in(cx, |this, window, cx| {
                if let Some(previous_edit) = this.previous_edit.take() {
                    match previous_edit {
                        // should remove scroll from process_query
                        PreviousEdit::ScrollBarOffset(offset) => {
                            this.table_interaction_state
                                .update(cx, |table, _| table.set_scroll_offset(offset))
                            // set selected index and scroll
                        }
                        PreviousEdit::Keybinding {
                            action_mapping,
                            action_name,
                            fallback,
                        } => {
                            let scroll_position =
                                this.matches.iter().enumerate().find_map(|(index, item)| {
                                    let binding = &this.keybindings[item.candidate_id];
                                    if binding.get_action_mapping().is_some_and(|binding_mapping| {
                                        binding_mapping == action_mapping
                                    }) && binding.action().name == action_name
                                    {
                                        Some(index)
                                    } else {
                                        None
                                    }
                                });

                            if let Some(scroll_position) = scroll_position {
                                this.select_index(
                                    scroll_position,
                                    Some(ScrollStrategy::Top),
                                    window,
                                    cx,
                                );
                            } else {
                                this.table_interaction_state
                                    .update(cx, |table, _| table.set_scroll_offset(fallback));
                            }
                            cx.notify();
                        }
                    }
                }
            })
        })
        .detach_and_log_err(cx);
    }
}

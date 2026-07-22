use super::*;

impl BufferSearchBar {
    pub fn query_editor_focused(&self) -> bool {
        self.query_editor_focused
    }

    pub fn register(registrar: &mut impl SearchActionsRegistrar) {
        registrar.register_handler(ForDeployed(|this, _: &FocusSearch, window, cx| {
            this.query_editor.focus_handle(cx).focus(window, cx);
            this.select_query(window, cx);
        }));
        registrar.register_handler(ForDeployed(
            |this, action: &ToggleCaseSensitive, window, cx| {
                if this.supported_options(cx).case {
                    this.toggle_case_sensitive(action, window, cx);
                }
            },
        ));
        registrar.register_handler(ForDeployed(|this, action: &ToggleWholeWord, window, cx| {
            if this.supported_options(cx).word {
                this.toggle_whole_word(action, window, cx);
            }
        }));
        registrar.register_handler(ForDeployed(|this, action: &ToggleRegex, window, cx| {
            if this.supported_options(cx).regex {
                this.toggle_regex(action, window, cx);
            }
        }));
        registrar.register_handler(ForDeployed(|this, action: &ToggleSelection, window, cx| {
            if this.supported_options(cx).selection {
                this.toggle_selection(action, window, cx);
            } else {
                cx.propagate();
            }
        }));
        registrar.register_handler(ForDeployed(|this, action: &ToggleReplace, window, cx| {
            if this.supported_options(cx).replacement {
                this.toggle_replace(action, window, cx);
            } else {
                cx.propagate();
            }
        }));
        registrar.register_handler(WithResultsOrExternalQuery(
            |this, action: &SelectNextMatch, window, cx| {
                if this.supported_options(cx).find_in_results {
                    cx.propagate();
                } else {
                    this.select_next_match(action, window, cx);
                }
            },
        ));
        registrar.register_handler(WithResultsOrExternalQuery(
            |this, action: &SelectPreviousMatch, window, cx| {
                if this.supported_options(cx).find_in_results {
                    cx.propagate();
                } else {
                    this.select_prev_match(action, window, cx);
                }
            },
        ));
        registrar.register_handler(WithResultsOrExternalQuery(
            |this, action: &SelectAllMatches, window, cx| {
                if this.supported_options(cx).find_in_results {
                    cx.propagate();
                } else {
                    this.select_all_matches(action, window, cx);
                }
            },
        ));
        registrar.register_handler(ForDeployed(
            |this, _: &editor::actions::Cancel, window, cx| {
                this.dismiss(&Dismiss, window, cx);
            },
        ));
        registrar.register_handler(ForDeployed(|this, _: &Dismiss, window, cx| {
            this.dismiss(&Dismiss, window, cx);
        }));

        // register deploy buffer search for both search bar states, since we want to focus into the search bar
        // when the deploy action is triggered in the buffer.
        registrar.register_handler(ForDeployed(|this, deploy, window, cx| {
            this.deploy(deploy, None, window, cx);
        }));
        registrar.register_handler(ForDismissed(|this, deploy, window, cx| {
            this.deploy(deploy, None, window, cx);
        }));
        registrar.register_handler(ForDeployed(|this, _: &DeployReplace, window, cx| {
            if this.supported_options(cx).find_in_results {
                cx.propagate();
            } else {
                this.deploy(&Deploy::replace(), None, window, cx);
            }
        }));
        registrar.register_handler(ForDismissed(|this, _: &DeployReplace, window, cx| {
            if this.supported_options(cx).find_in_results {
                cx.propagate();
            } else {
                this.deploy(&Deploy::replace(), None, window, cx);
            }
        }));
        registrar.register_handler(ForDeployed(
            |this, action: &UseSelectionForFind, window, cx| {
                this.use_selection_for_find(action, window, cx);
            },
        ));
        registrar.register_handler(ForDismissed(
            |this, action: &UseSelectionForFind, window, cx| {
                this.use_selection_for_find(action, window, cx);
            },
        ));
    }

    pub fn new(
        languages: Option<Arc<LanguageRegistry>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query_editor = cx.new(|cx| {
            let mut editor = Editor::auto_height(1, 4, window, cx);
            editor.set_use_autoclose(false);
            editor.set_use_selection_highlight(false);
            editor
        });
        cx.subscribe_in(&query_editor, window, Self::on_query_editor_event)
            .detach();
        let replacement_editor = cx.new(|cx| Editor::auto_height(1, 4, window, cx));
        cx.subscribe(&replacement_editor, Self::on_replacement_editor_event)
            .detach();

        let search_options = SearchOptions::from_settings(&EditorSettings::get_global(cx).search);
        if let Some(languages) = languages {
            let query_buffer = query_editor
                .read(cx)
                .buffer()
                .read(cx)
                .as_singleton()
                .expect("query editor should be backed by a singleton buffer");

            query_buffer
                .read(cx)
                .set_language_registry(languages.clone());

            cx.spawn(async move |buffer_search_bar, cx| {
                use anyhow::Context as _;

                let regex_language = languages
                    .language_for_name("regex")
                    .await
                    .context("loading regex language")?;

                buffer_search_bar
                    .update(cx, |buffer_search_bar, cx| {
                        buffer_search_bar.regex_language = Some(regex_language);
                        buffer_search_bar.adjust_query_regex_language(cx);
                    })
                    .ok();
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        }

        Self {
            query_editor,
            query_editor_focused: false,
            replacement_editor,
            replacement_editor_focused: false,
            active_searchable_item: None,
            active_searchable_item_subscriptions: None,
            #[cfg(target_os = "macos")]
            pending_external_query: None,
            active_match_index: None,
            searchable_items_with_matches: Default::default(),
            default_options: search_options,
            configured_options: search_options,
            search_options,
            pending_search: None,
            query_error: None,
            dismissed: true,
            search_history: SearchHistory::new(
                Some(MAX_BUFFER_SEARCH_HISTORY_SIZE),
                project::search_history::QueryInsertionBehavior::ReplacePreviousIfContains,
            ),
            search_history_cursor: Default::default(),
            active_search: None,
            replace_enabled: false,
            selection_search_enabled: None,
            scroll_handle: ScrollHandle::new(),
            regex_language: None,
            splittable_editor: None,
            _splittable_editor_subscription: None,
        }
    }

    pub fn is_dismissed(&self) -> bool {
        self.dismissed
    }

    pub fn dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        self.dismissed = true;
        cx.emit(Event::Dismissed);
        self.query_error = None;
        self.sync_select_next_case_sensitivity(cx);

        for searchable_item in self.searchable_items_with_matches.keys() {
            if let Some(searchable_item) =
                WeakSearchableItemHandle::upgrade(searchable_item.as_ref(), cx)
            {
                searchable_item.clear_matches(window, cx);
            }
        }

        let needs_collapse_expand = self.needs_expand_collapse_option(cx);

        if let Some(active_editor) = self.active_searchable_item.as_mut() {
            self.selection_search_enabled = None;
            self.replace_enabled = false;
            active_editor.search_bar_visibility_changed(false, window, cx);
            active_editor.toggle_filtered_search_ranges(None, window, cx);
            let handle = active_editor.item_focus_handle(cx);
            self.focus(&handle, window, cx);
        }

        if needs_collapse_expand {
            cx.emit(Event::UpdateLocation);
            cx.emit(ToolbarItemEvent::ChangeLocation(
                ToolbarItemLocation::PrimaryLeft,
            ));
            cx.notify();
            return;
        }
        cx.emit(Event::UpdateLocation);
        cx.emit(ToolbarItemEvent::ChangeLocation(
            ToolbarItemLocation::Hidden,
        ));
        cx.notify();
    }
}

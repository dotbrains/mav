use super::*;

impl OutlinePanel {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        let serialized_panel = match workspace
            .read_with(&cx, |workspace, _| {
                OutlinePanel::serialization_key(workspace)
            })
            .ok()
            .flatten()
        {
            Some(serialization_key) => {
                let kvp = cx.update(|_, cx| KeyValueStore::global(cx))?;
                cx.background_spawn(async move { kvp.read_kvp(&serialization_key) })
                    .await
                    .context("loading outline panel")
                    .log_err()
                    .flatten()
                    .map(|panel| serde_json::from_str::<SerializedOutlinePanel>(&panel))
                    .transpose()
                    .log_err()
                    .flatten()
            }
            None => None,
        };

        workspace.update_in(&mut cx, |workspace, window, cx| {
            let panel = Self::new(workspace, serialized_panel.as_ref(), window, cx);
            panel.update(cx, |_, cx| cx.notify());
            panel
        })
    }

    pub(super) fn new(
        workspace: &mut Workspace,
        serialized: Option<&SerializedOutlinePanel>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let project = workspace.project().clone();
        let workspace_handle = cx.entity().downgrade();

        cx.new(|cx| {
            let filter_editor = cx.new(|cx| {
                let mut editor = Editor::single_line(window, cx);
                editor.set_placeholder_text("Search buffer symbols…", window, cx);
                editor
            });
            let filter_update_subscription = cx.subscribe_in(
                &filter_editor,
                window,
                |outline_panel: &mut Self, _, event, window, cx| {
                    if let editor::EditorEvent::BufferEdited = event {
                        outline_panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
                    }
                },
            );

            let focus_handle = cx.focus_handle();
            let focus_subscription = cx.on_focus(&focus_handle, window, Self::focus_in);
            let workspace_subscription = cx.subscribe_in(
                &workspace
                    .weak_handle()
                    .upgrade()
                    .expect("have a &mut Workspace"),
                window,
                move |outline_panel, workspace, event, window, cx| {
                    if let workspace::Event::ActiveItemChanged = event {
                        if let Some((new_active_item, new_active_editor)) =
                            workspace_active_editor(workspace.read(cx), cx)
                        {
                            if outline_panel.should_replace_active_item(new_active_item.as_ref()) {
                                outline_panel.replace_active_editor(
                                    new_active_item,
                                    new_active_editor,
                                    window,
                                    cx,
                                );
                            }
                        } else {
                            outline_panel.clear_previous(window, cx);
                            cx.notify();
                        }
                    }
                },
            );

            let icons_subscription = cx.observe_global::<FileIcons>(|_, cx| {
                cx.notify();
            });

            let mut outline_panel_settings = *OutlinePanelSettings::get_global(cx);
            let mut current_theme = ThemeSettings::get_global(cx).clone();
            let mut document_symbols_by_buffer = HashMap::default();
            let settings_subscription =
                cx.observe_global_in::<SettingsStore>(window, move |outline_panel, window, cx| {
                    let new_settings = OutlinePanelSettings::get_global(cx);
                    let new_theme = ThemeSettings::get_global(cx);
                    let mut outlines_invalidated = false;
                    if &current_theme != new_theme {
                        outline_panel_settings = *new_settings;
                        current_theme = new_theme.clone();
                        for buffer in outline_panel.buffers.values_mut() {
                            buffer.invalidate_outlines();
                        }
                        outlines_invalidated = true;
                        let update_cached_items = outline_panel.update_non_fs_items(window, cx);
                        if update_cached_items {
                            outline_panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
                        }
                    } else if &outline_panel_settings != new_settings {
                        let old_expansion_depth = outline_panel_settings.expand_outlines_with_depth;
                        outline_panel_settings = *new_settings;

                        if old_expansion_depth != new_settings.expand_outlines_with_depth {
                            let old_collapsed_entries = outline_panel.collapsed_entries.clone();
                            outline_panel
                                .collapsed_entries
                                .retain(|entry| !matches!(entry, CollapsedEntry::Outline(..)));

                            let new_depth = new_settings.expand_outlines_with_depth;

                            for (buffer_id, buffer) in &outline_panel.buffers {
                                if let OutlineState::Outlines(outlines) = &buffer.outlines {
                                    for outline in outlines {
                                        if outline_panel
                                            .outline_children_cache
                                            .get(buffer_id)
                                            .and_then(|children_map| {
                                                let key = (outline.range.clone(), outline.depth);
                                                children_map.get(&key)
                                            })
                                            .copied()
                                            .unwrap_or(false)
                                            && (new_depth == 0 || outline.depth >= new_depth)
                                        {
                                            outline_panel.collapsed_entries.insert(
                                                CollapsedEntry::Outline(outline.range.clone()),
                                            );
                                        }
                                    }
                                }
                            }

                            if old_collapsed_entries != outline_panel.collapsed_entries {
                                outline_panel.update_cached_entries(
                                    Some(UPDATE_DEBOUNCE),
                                    window,
                                    cx,
                                );
                            }
                        } else {
                            cx.notify();
                        }
                    }

                    if !outlines_invalidated {
                        let new_document_symbols = outline_panel
                            .buffers
                            .keys()
                            .filter_map(|buffer_id| {
                                let buffer = outline_panel
                                    .project
                                    .read(cx)
                                    .buffer_for_id(*buffer_id, cx)?;
                                let buffer = buffer.read(cx);
                                let doc_symbols =
                                    LanguageSettings::for_buffer(buffer, cx).document_symbols;
                                Some((*buffer_id, doc_symbols))
                            })
                            .collect();
                        if new_document_symbols != document_symbols_by_buffer {
                            document_symbols_by_buffer = new_document_symbols;
                            for buffer in outline_panel.buffers.values_mut() {
                                buffer.invalidate_outlines();
                            }
                            let update_cached_items = outline_panel.update_non_fs_items(window, cx);
                            if update_cached_items {
                                outline_panel.update_cached_entries(
                                    Some(UPDATE_DEBOUNCE),
                                    window,
                                    cx,
                                );
                            }
                        }
                    }
                });

            let scroll_handle = UniformListScrollHandle::new();

            let mut outline_panel = Self {
                mode: ItemsDisplayMode::Outline,
                active: serialized.and_then(|s| s.active).unwrap_or(false),
                pinned: false,
                workspace: workspace_handle,
                project,
                fs: workspace.app_state().fs.clone(),
                max_width_item_index: None,
                scroll_handle,
                rendered_entries_len: 0,
                focus_handle,
                filter_editor,
                fs_entries: Vec::new(),
                fs_entries_depth: HashMap::default(),
                fs_children_count: HashMap::default(),
                collapsed_entries: HashSet::default(),
                unfolded_dirs: HashMap::default(),
                selected_entry: SelectedEntry::None,
                context_menu: None,
                active_item: None,
                pending_serialization: Task::ready(None),
                new_entries_for_fs_update: HashSet::default(),
                preserve_selection_on_buffer_fold_toggles: HashSet::default(),
                pending_default_expansion_depth: None,
                fs_entries_update_task: Task::ready(()),
                fs_entries_update_pending: false,
                cached_entries_update_task: Task::ready(()),
                cached_entries_update_pending: false,
                reveal_selection_task: Task::ready(Ok(())),
                outline_fetch_tasks: HashMap::default(),
                buffers: HashMap::default(),
                cached_entries: Vec::new(),
                _subscriptions: vec![
                    settings_subscription,
                    icons_subscription,
                    focus_subscription,
                    workspace_subscription,
                    filter_update_subscription,
                ],
                outline_children_cache: HashMap::default(),
            };
            if let Some((item, editor)) = workspace_active_editor(workspace, cx) {
                outline_panel.replace_active_editor(item, editor, window, cx);
            }
            outline_panel
        })
    }

    pub(super) fn serialization_key(workspace: &Workspace) -> Option<String> {
        workspace
            .database_id()
            .map(|id| i64::from(id).to_string())
            .or(workspace.session_id())
            .map(|id| format!("{}-{:?}", OUTLINE_PANEL_KEY, id))
    }

    pub(super) fn serialize(&mut self, cx: &mut Context<Self>) {
        let Some(serialization_key) = self
            .workspace
            .read_with(cx, |workspace, _| {
                OutlinePanel::serialization_key(workspace)
            })
            .ok()
            .flatten()
        else {
            return;
        };
        let active = self.active.then_some(true);
        let kvp = KeyValueStore::global(cx);
        self.pending_serialization = cx.background_spawn(
            async move {
                kvp.write_kvp(
                    serialization_key,
                    serde_json::to_string(&SerializedOutlinePanel { active })?,
                )
                .await?;
                anyhow::Ok(())
            }
            .log_err(),
        );
    }
}

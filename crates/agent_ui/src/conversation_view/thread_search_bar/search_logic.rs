use super::*;

impl ThreadSearchBar {
    pub(in crate::conversation_view) fn update_matches(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_active_match_ix = self.active_match;
        let previous_active_key = previous_active_match_ix
            .and_then(|ix| self.matches.get(ix))
            .map(ThreadMatch::key);

        let (query, err_msg) = self.build_query(cx);
        self.query_error = !self.current_query(cx).is_empty() && query.is_none();
        self.query_error_message = err_msg;

        let Some(query) = query else {
            self.clear_results(cx);
            cx.notify();
            return;
        };

        let mut targets: Vec<SearchTarget> = Vec::new();
        let thread = self.thread.read(cx);
        let entry_view_state = self.entry_view_state.read(cx);
        for (entry_ix, entry) in thread.entries().iter().enumerate() {
            match entry {
                AgentThreadEntry::UserMessage(_) => {
                    let editor = entry_view_state
                        .entry(entry_ix)
                        .and_then(|view_entry| view_entry.message_editor())
                        .map(|message_editor| message_editor.read(cx).editor().clone());
                    let Some(editor) = editor else {
                        continue;
                    };
                    let snapshot = editor.read(cx).buffer().read(cx).snapshot(cx);
                    targets.push(SearchTarget::Editor {
                        entry_ix,
                        editor,
                        snapshot,
                    });
                }
                _ => {
                    for markdown in collect_markdowns(entry_ix, entry, &entry_view_state, cx) {
                        let source = markdown.read(cx).source().clone();
                        targets.push(SearchTarget::Markdown {
                            entry_ix,
                            markdown,
                            source,
                        });
                    }
                }
            }
        }

        if targets.is_empty() {
            self.clear_results(cx);
            cx.notify();
            return;
        }

        self._search_task = Some(cx.spawn_in(window, async move |this, cx| {
            let scanned = cx
                .background_spawn(async move {
                    targets
                        .into_iter()
                        .filter_map(|target| match target {
                            SearchTarget::Editor {
                                entry_ix,
                                editor,
                                snapshot,
                            } => {
                                let ranges = query.search_str(&snapshot.text());
                                if ranges.is_empty() {
                                    return None;
                                }
                                let anchor_ranges = ranges
                                    .iter()
                                    .map(|range| {
                                        snapshot.anchor_before(MultiBufferOffset(range.start))
                                            ..snapshot.anchor_after(MultiBufferOffset(range.end))
                                    })
                                    .collect();
                                Some(ScannedTarget::Editor {
                                    entry_ix,
                                    editor,
                                    ranges,
                                    anchor_ranges,
                                })
                            }
                            SearchTarget::Markdown {
                                entry_ix,
                                markdown,
                                source,
                            } => {
                                let ranges = query.search_str(&source);
                                if ranges.is_empty() {
                                    return None;
                                }
                                Some(ScannedTarget::Markdown {
                                    entry_ix,
                                    markdown,
                                    ranges,
                                })
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.apply_search_results(
                    scanned,
                    previous_active_key,
                    previous_active_match_ix,
                    window,
                    cx,
                );
            })
            .ok();
        }));
    }

    fn apply_search_results(
        &mut self,
        scanned: Vec<ScannedTarget>,
        previous_active_key: Option<MatchKey>,
        previous_active_match_ix: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_match_highlights(cx);
        self.matches.clear();
        self.active_match = None;

        for target in scanned {
            match target {
                ScannedTarget::Editor {
                    entry_ix,
                    editor,
                    ranges,
                    anchor_ranges,
                } => {
                    let weak_editor = editor.downgrade();
                    for (ix, (range, anchor_range)) in ranges.iter().zip(&anchor_ranges).enumerate()
                    {
                        self.matches.push(ThreadMatch {
                            entry_ix,
                            target: MatchTarget::Editor {
                                editor: weak_editor.clone(),
                                anchor_range: anchor_range.clone(),
                                editor_match_ix: ix,
                            },
                            source_range: range.clone(),
                        });
                    }
                    self.highlighted_editors.push(weak_editor);
                }
                ScannedTarget::Markdown {
                    entry_ix,
                    markdown,
                    ranges,
                } => {
                    let weak = markdown.downgrade();
                    for (ix, range) in ranges.iter().enumerate() {
                        self.matches.push(ThreadMatch {
                            entry_ix,
                            target: MatchTarget::Markdown {
                                markdown: weak.clone(),
                                markdown_match_ix: ix,
                            },
                            source_range: range.clone(),
                        });
                    }
                    self.highlighted_markdowns.push(weak);
                    markdown.update(cx, |md, cx| {
                        md.set_search_highlights(ranges, None, cx);
                    });
                }
            }
        }

        if !self.matches.is_empty() {
            let preserved_ix = previous_active_key
                .as_ref()
                .and_then(|key| self.matches.iter().position(|m| &m.key() == key));
            let active_match_ix = preserved_ix
                .or_else(|| previous_active_match_ix.filter(|ix| *ix < self.matches.len()))
                .unwrap_or(0);
            let scroll_to_match = preserved_ix.is_none();
            self.activate_match(active_match_ix, scroll_to_match, window, cx);
        } else {
            cx.notify();
        }
    }

    pub(super) fn activate_match(
        &mut self,
        ix: usize,
        scroll_to_match: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(m) = self.matches.get(ix) else {
            return;
        };
        let entry_ix = m.entry_ix;
        let source_index = m.source_range.start;
        let target = m.target.clone();
        let target_entity_id = target.entity_id();
        let target_match_ix = target.match_ix();

        for weak in &self.highlighted_markdowns {
            if let Some(markdown) = weak.upgrade() {
                let active = (weak.entity_id() == target_entity_id).then_some(target_match_ix);
                markdown.update(cx, |markdown, cx| {
                    markdown.set_active_search_highlight(active, cx);
                    if active.is_some() && scroll_to_match {
                        markdown.request_autoscroll_to_source_index(source_index, cx);
                    }
                });
            }
        }

        let mut per_editor: HashMap<EntityId, (WeakEntity<Editor>, Vec<Range<Anchor>>)> =
            HashMap::default();
        for mat in &self.matches {
            if let MatchTarget::Editor {
                editor,
                anchor_range,
                ..
            } = &mat.target
            {
                let entry = per_editor
                    .entry(editor.entity_id())
                    .or_insert_with(|| (editor.clone(), Vec::new()));
                entry.1.push(anchor_range.clone());
            }
        }
        for (editor_id, (weak_editor, ranges)) in per_editor {
            let Some(editor) = weak_editor.upgrade() else {
                continue;
            };
            let active_ix = (editor_id == target_entity_id).then_some(target_match_ix);
            editor.update(cx, |editor, cx| {
                editor.highlight_background(
                    HighlightKey::BufferSearchHighlights,
                    &ranges,
                    move |index, theme| {
                        if active_ix == Some(*index) {
                            theme.colors().search_active_match_background
                        } else {
                            theme.colors().search_match_background
                        }
                    },
                    cx,
                );
            });
        }

        if scroll_to_match
            && let MatchTarget::Editor {
                editor,
                anchor_range,
                ..
            } = &target
            && let Some(editor) = editor.upgrade()
        {
            let anchor_range = anchor_range.clone();
            editor.update(cx, |editor, cx| {
                editor.change_selections(
                    SelectionEffects::scroll(Autoscroll::fit()).from_search(true),
                    window,
                    cx,
                    |selections| selections.select_anchor_ranges([anchor_range]),
                );
            });
        }

        self.active_match = Some(ix);
        if scroll_to_match {
            (self.on_activate_match)(entry_ix, window, cx);
        }
        cx.notify();
    }
}

use super::completion_kinds::exact_case_match_count;
use super::*;

impl CompletionsMenu {
    pub fn filter(
        &mut self,
        query: Arc<String>,
        query_end: text::Anchor,
        buffer: &Entity<Buffer>,
        provider: Option<Rc<dyn CompletionProvider>>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        self.cancel_filter.store(true, Ordering::Relaxed);
        self.cancel_filter = Arc::new(AtomicBool::new(false));
        let matches = self.do_async_filtering(query, query_end, buffer, cx);
        let id = self.id;
        self.filter_task = cx.spawn_in(window, async move |editor, cx| {
            let matches = matches.await;
            editor
                .update_in(cx, |editor, window, cx| {
                    editor.with_completions_menu_matching_id(id, |this| {
                        if let Some(this) = this {
                            this.set_filter_results(matches, provider, window, cx);
                        }
                    });
                })
                .ok();
        });
    }

    pub fn do_async_filtering(
        &self,
        query: Arc<String>,
        query_end: text::Anchor,
        buffer: &Entity<Buffer>,
        cx: &Context<Editor>,
    ) -> Task<Vec<StringMatch>> {
        let buffer_snapshot = buffer.read(cx).snapshot();
        let background_executor = cx.background_executor().clone();
        let match_candidates = self.match_candidates.clone();
        let cancel_filter = self.cancel_filter.clone();
        let default_query = query.clone();

        let matches_task = cx.background_spawn(async move {
            let queries_and_candidates = match_candidates
                .iter()
                .map(|(query_start, candidates)| {
                    let query_for_batch = match query_start {
                        Some(start) => {
                            Arc::new(buffer_snapshot.text_for_range(*start..query_end).collect())
                        }
                        None => default_query.clone(),
                    };
                    (query_for_batch, candidates)
                })
                .collect_vec();

            let mut results = vec![];
            for (query, match_candidates) in queries_and_candidates {
                results.extend(
                    fuzzy::match_strings(
                        &match_candidates,
                        &query,
                        query.chars().any(|c| c.is_uppercase()),
                        false,
                        1000,
                        &cancel_filter,
                        background_executor.clone(),
                    )
                    .await,
                );
            }
            results
        });

        let completions = self.completions.clone();
        let sort_completions = self.sort_completions;
        let snippet_sort_order = self.snippet_sort_order;
        cx.foreground_executor().spawn(async move {
            let mut matches = matches_task.await;

            let completions_ref = completions.borrow();

            if sort_completions {
                matches = Self::sort_string_matches(
                    matches,
                    Some(&query), // used for non-snippets only
                    snippet_sort_order,
                    &completions_ref,
                );
            }

            // Remove duplicate snippet prefixes (e.g., "cool code" will match
            // the text "c c" in two places; we should only show the longer one)
            let mut snippets_seen = HashSet::<(usize, usize)>::default();
            matches.retain(|result| {
                match completions_ref[result.candidate_id].snippet_deduplication_key {
                    Some(key) => snippets_seen.insert(key),
                    None => true,
                }
            });

            matches
        })
    }

    pub fn set_filter_results(
        &mut self,
        matches: Vec<StringMatch>,
        provider: Option<Rc<dyn CompletionProvider>>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let completions = self.completions.borrow();
        let mut entries: Vec<CompletionMenuEntry> = Vec::with_capacity(matches.len());
        let mut last_group: Option<&CompletionGroup> = None;
        for mat in matches {
            let group = completions[mat.candidate_id].group.as_ref();
            if group != last_group {
                if group.is_some() || last_group.is_some() {
                    if !entries.is_empty() {
                        entries.push(CompletionMenuEntry::Divider);
                    }
                    if let Some(label) = group.and_then(|g| g.label.as_ref()) {
                        entries.push(CompletionMenuEntry::GroupHeader(label.clone()));
                    }
                }
                last_group = group;
            }
            entries.push(CompletionMenuEntry::Match(mat));
        }
        drop(completions);
        *self.entries.borrow_mut() = entries.into_boxed_slice();
        self.selected_item = self.find_selectable_entry(0, true).unwrap_or(0);
        self.handle_selection_changed(provider.as_deref(), window, cx);
    }

    pub(super) fn sort_string_matches(
        matches: Vec<StringMatch>,
        query: Option<&str>,
        snippet_sort_order: SnippetSortOrder,
        completions: &[Completion],
    ) -> Vec<StringMatch> {
        let mut matches = matches;

        #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
        enum MatchTier<'a> {
            WordStartMatch {
                sort_exact: Reverse<i32>,
                sort_snippet: Reverse<i32>,
                sort_score: Reverse<OrderedFloat<f64>>,
                sort_positions: Vec<usize>,
                sort_exact_case_matches: Reverse<usize>,
                sort_text: Option<&'a str>,
                sort_kind: usize,
                sort_label: &'a str,
            },
            OtherMatch {
                sort_score: Reverse<OrderedFloat<f64>>,
            },
        }

        let query_start_lower = query
            .as_ref()
            .and_then(|q| q.chars().next())
            .and_then(|c| c.to_lowercase().next());

        if snippet_sort_order == SnippetSortOrder::None {
            matches
                .retain(|string_match| !completions[string_match.candidate_id].is_snippet_kind());
        }

        matches.sort_unstable_by_key(|string_match| {
            let completion = &completions[string_match.candidate_id];

            let sort_text = match &completion.source {
                CompletionSource::Lsp { lsp_completion, .. } => lsp_completion.sort_text.as_deref(),
                CompletionSource::Dap { sort_text } => Some(sort_text.as_str()),
                _ => None,
            };

            let (sort_kind, sort_label) = completion.sort_key();

            let score = string_match.score;
            let sort_score = Reverse(OrderedFloat(score));

            // Snippets do their own first-letter matching logic elsewhere.
            let is_snippet = completion.is_snippet_kind();
            let query_start_doesnt_match_split_words = !is_snippet
                && query_start_lower
                    .map(|query_char| {
                        !split_words(&string_match.string).any(|word| {
                            word.chars().next().and_then(|c| c.to_lowercase().next())
                                == Some(query_char)
                        })
                    })
                    .unwrap_or(false);

            if query_start_doesnt_match_split_words {
                MatchTier::OtherMatch { sort_score }
            } else {
                let sort_snippet = match snippet_sort_order {
                    SnippetSortOrder::Top => Reverse(if is_snippet { 1 } else { 0 }),
                    SnippetSortOrder::Bottom => Reverse(if is_snippet { 0 } else { 1 }),
                    SnippetSortOrder::Inline => Reverse(0),
                    SnippetSortOrder::None => Reverse(0),
                };
                let sort_positions = string_match.positions.clone();
                let sort_exact_case_matches = Reverse(exact_case_match_count(
                    query.unwrap_or_default(),
                    string_match,
                ));
                // This exact matching won't work for multi-word snippets, but it's fine
                let sort_exact = Reverse(if Some(completion.label.filter_text()) == query {
                    1
                } else {
                    0
                });

                MatchTier::WordStartMatch {
                    sort_exact,
                    sort_snippet,
                    sort_score,
                    sort_positions,
                    sort_exact_case_matches,
                    sort_text,
                    sort_kind,
                    sort_label,
                }
            }
        });

        matches
    }

    pub fn preserve_markdown_cache(&mut self, prev_menu: CompletionsMenu) {
        self.markdown_cache = prev_menu.markdown_cache.clone();

        // Convert ForCandidate cache keys to ForCompletionMatch keys.
        let prev_completions = prev_menu.completions.borrow();
        self.markdown_cache
            .borrow_mut()
            .retain_mut(|(key, _markdown)| match key {
                MarkdownCacheKey::ForCompletionMatch { .. } => true,
                MarkdownCacheKey::ForCandidate { candidate_id } => {
                    if let Some(completion) = prev_completions.get(*candidate_id) {
                        match &completion.documentation {
                            Some(CompletionDocumentation::MultiLineMarkdown(source)) => {
                                *key = MarkdownCacheKey::ForCompletionMatch {
                                    new_text: completion.new_text.clone(),
                                    markdown_source: source.clone(),
                                };
                                true
                            }
                            _ => false,
                        }
                    } else {
                        false
                    }
                }
            });
    }

    pub fn scroll_aside(
        &mut self,
        amount: ScrollAmount,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let mut offset = self.scroll_handle_aside.offset();

        offset.y -= amount.pixels(
            window.line_height(),
            self.scroll_handle_aside.bounds().size.height - px(16.),
        ) / 2.0;

        cx.notify();
        self.scroll_handle_aside.set_offset(offset);
    }
}

use super::*;

impl CompletionsMenu {
    pub fn resolve_visible_completions(
        &mut self,
        provider: Option<&dyn CompletionProvider>,
        cx: &mut Context<Editor>,
    ) {
        if !self.resolve_completions {
            return;
        }
        let Some(provider) = provider else {
            return;
        };

        let entries = self.entries.borrow();
        if entries.is_empty() {
            return;
        }
        if self.selected_item >= entries.len() {
            log::error!(
                "bug: completion selected_item >= entries.len(): {} >= {}",
                self.selected_item,
                entries.len()
            );
            self.selected_item = entries.len() - 1;
        }

        // Attempt to resolve completions for every item that will be displayed. This matters
        // because single line documentation may be displayed inline with the completion.
        //
        // When navigating to the very beginning or end of completions, `last_rendered_range` may
        // have no overlap with the completions that will be displayed, so instead use a range based
        // on the last rendered count.
        const APPROXIMATE_VISIBLE_COUNT: usize = 12;
        let last_rendered_range = self.last_rendered_range.borrow().clone();
        let visible_count = last_rendered_range
            .clone()
            .map_or(APPROXIMATE_VISIBLE_COUNT, |range| range.count());
        let entry_range = if self.selected_item == 0 {
            0..min(visible_count, entries.len())
        } else if self.selected_item == entries.len() - 1 {
            entries.len().saturating_sub(visible_count)..entries.len()
        } else {
            last_rendered_range.map_or(0..0, |range| {
                min(range.start, entries.len())..min(range.end, entries.len())
            })
        };

        // Expand the range to resolve more completions than are predicted to be visible, to reduce
        // jank on navigation.
        let entry_indices = util::expanded_and_wrapped_usize_range(
            entry_range,
            RESOLVE_BEFORE_ITEMS,
            RESOLVE_AFTER_ITEMS,
            entries.len(),
        );

        // Avoid work by sometimes filtering out completions that already have documentation.
        // This filtering doesn't happen if the completions are currently being updated.
        let completions = self.completions.borrow();
        let candidate_ids = entry_indices
            .filter_map(|i| entries[i].as_match().map(|m| m.candidate_id))
            .filter(|i| completions[*i].documentation.is_none());

        // Current selection is always resolved even if it already has documentation, to handle
        // out-of-spec language servers that return more results later.
        let Some(selected_candidate_id) = entries[self.selected_item]
            .as_match()
            .map(|m| m.candidate_id)
        else {
            drop(entries);
            drop(completions);
            return;
        };
        let candidate_ids = iter::once(selected_candidate_id)
            .chain(candidate_ids.filter(|id| *id != selected_candidate_id))
            .collect::<Vec<usize>>();
        drop(entries);

        if candidate_ids.is_empty() {
            return;
        }

        let resolve_task = provider.resolve_completions(
            self.buffer.clone(),
            candidate_ids,
            self.completions.clone(),
            cx,
        );

        let completion_id = self.id;
        cx.spawn(async move |editor, cx| {
            if let Some(true) = resolve_task.await.log_err() {
                editor
                    .update(cx, |editor, cx| {
                        // `resolve_completions` modified state affecting display.
                        cx.notify();
                        editor.with_completions_menu_matching_id(completion_id, |menu| {
                            if let Some(menu) = menu {
                                menu.start_markdown_parse_for_nearby_entries(cx)
                            }
                        });
                    })
                    .ok();
            }
        })
        .detach();
    }

    pub(super) fn start_markdown_parse_for_nearby_entries(&self, cx: &mut Context<Editor>) {
        // Enqueue parse tasks of nearer items first.
        //
        // TODO: This means that the nearer items will actually be further back in the cache, which
        // is not ideal. In practice this is fine because `get_or_create_markdown` moves the current
        // selection to the front (when `is_render = true`).
        let entry_indices = util::wrapped_usize_outward_from(
            self.selected_item,
            MARKDOWN_CACHE_BEFORE_ITEMS,
            MARKDOWN_CACHE_AFTER_ITEMS,
            self.entries.borrow().len(),
        );

        for index in entry_indices {
            self.get_or_create_entry_markdown(index, cx);
        }
    }

    fn get_or_create_entry_markdown(
        &self,
        index: usize,
        cx: &mut Context<Editor>,
    ) -> Option<Entity<Markdown>> {
        let entries = self.entries.borrow();
        if index >= entries.len() {
            return None;
        }
        let candidate_id = entries[index].as_match()?.candidate_id;
        let completions = self.completions.borrow();
        match &completions[candidate_id].documentation {
            Some(CompletionDocumentation::MultiLineMarkdown(source)) if !source.is_empty() => self
                .get_or_create_markdown(candidate_id, Some(source), false, &completions, cx)
                .map(|(_, markdown)| markdown),
            Some(_) => None,
            _ => None,
        }
    }

    pub(super) fn get_or_create_markdown(
        &self,
        candidate_id: usize,
        source: Option<&SharedString>,
        is_render: bool,
        completions: &[Completion],
        cx: &mut Context<Editor>,
    ) -> Option<(bool, Entity<Markdown>)> {
        let mut markdown_cache = self.markdown_cache.borrow_mut();

        let mut has_completion_match_cache_entry = false;
        let mut matching_entry = markdown_cache.iter().find_position(|(key, _)| match key {
            MarkdownCacheKey::ForCandidate { candidate_id: id } => *id == candidate_id,
            MarkdownCacheKey::ForCompletionMatch { .. } => {
                has_completion_match_cache_entry = true;
                false
            }
        });

        if has_completion_match_cache_entry && matching_entry.is_none() {
            if let Some(source) = source {
                matching_entry = markdown_cache.iter().find_position(|(key, _)| {
                    matches!(key, MarkdownCacheKey::ForCompletionMatch { markdown_source, .. }
                                if markdown_source == source)
                });
            } else {
                // Heuristic guess that documentation can be reused when new_text matches. This is
                // to mitigate documentation flicker while typing. If this is wrong, then resolution
                // should cause the correct documentation to be displayed soon.
                let completion = &completions[candidate_id];
                matching_entry = markdown_cache.iter().find_position(|(key, _)| {
                    matches!(key, MarkdownCacheKey::ForCompletionMatch { new_text, .. }
                                if new_text == &completion.new_text)
                });
            }
        }

        if let Some((cache_index, (key, markdown))) = matching_entry {
            let markdown = markdown.clone();

            // Since the markdown source matches, the key can now be ForCandidate.
            if source.is_some() && matches!(key, MarkdownCacheKey::ForCompletionMatch { .. }) {
                markdown_cache[cache_index].0 = MarkdownCacheKey::ForCandidate { candidate_id };
            }

            if is_render && cache_index != 0 {
                // Move the current selection's cache entry to the front.
                markdown_cache.rotate_right(1);
                let cache_len = markdown_cache.len();
                markdown_cache.swap(0, (cache_index + 1) % cache_len);
            }

            let is_parsing = markdown.update(cx, |markdown, cx| {
                if let Some(source) = source {
                    // `reset` is called as it's possible for documentation to change due to resolve
                    // requests. It does nothing if `source` is unchanged.
                    markdown.reset(source.clone(), cx);
                }
                markdown.is_parsing()
            });
            return Some((is_parsing, markdown));
        }

        let Some(source) = source else {
            // Can't create markdown as there is no source.
            return None;
        };

        if markdown_cache.len() < MARKDOWN_CACHE_MAX_SIZE {
            let markdown = cx.new(|cx| {
                Markdown::new(
                    source.clone(),
                    self.language_registry.clone(),
                    self.language.clone(),
                    cx,
                )
            });
            // Handles redraw when the markdown is done parsing. The current render is for a
            // deferred draw, and so without this did not redraw when `markdown` notified.
            cx.observe(&markdown, |_, _, cx| cx.notify()).detach();
            markdown_cache.push_front((
                MarkdownCacheKey::ForCandidate { candidate_id },
                markdown.clone(),
            ));
            Some((true, markdown))
        } else {
            debug_assert_eq!(markdown_cache.capacity(), MARKDOWN_CACHE_MAX_SIZE);
            // Moves the last cache entry to the start. The ring buffer is full, so this does no
            // copying and just shifts indexes.
            markdown_cache.rotate_right(1);
            markdown_cache[0].0 = MarkdownCacheKey::ForCandidate { candidate_id };
            let markdown = &markdown_cache[0].1;
            markdown.update(cx, |markdown, cx| markdown.reset(source.clone(), cx));
            Some((true, markdown.clone()))
        }
    }
}

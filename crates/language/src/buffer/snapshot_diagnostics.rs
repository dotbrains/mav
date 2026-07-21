use super::*;

impl BufferSnapshot {
    pub fn runnable_ranges(
        &self,
        offset_range: Range<usize>,
    ) -> impl Iterator<Item = RunnableRange> + '_ {
        runnable::runnable_ranges(self, offset_range)
    }

    /// Returns selections for remote peers intersecting the given range.
    #[allow(clippy::type_complexity)]
    pub fn selections_in_range(
        &self,
        range: Range<Anchor>,
        include_local: bool,
    ) -> impl Iterator<
        Item = (
            ReplicaId,
            bool,
            CursorShape,
            impl Iterator<Item = &Selection<Anchor>> + '_,
        ),
    > + '_ {
        self.remote_selections
            .iter()
            .filter(move |(replica_id, set)| {
                (include_local || **replica_id != self.text.replica_id())
                    && !set.selections.is_empty()
            })
            .map(move |(replica_id, set)| {
                let start_ix = match set.selections.binary_search_by(|probe| {
                    probe.end.cmp(&range.start, self).then(Ordering::Greater)
                }) {
                    Ok(ix) | Err(ix) => ix,
                };
                let end_ix = match set.selections.binary_search_by(|probe| {
                    probe.start.cmp(&range.end, self).then(Ordering::Less)
                }) {
                    Ok(ix) | Err(ix) => ix,
                };

                (
                    *replica_id,
                    set.line_mode,
                    set.cursor_shape,
                    set.selections[start_ix..end_ix].iter(),
                )
            })
    }

    /// Returns if the buffer contains any diagnostics.
    pub fn has_diagnostics(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    /// Returns all the diagnostics intersecting the given range.
    pub fn diagnostics_in_range<'a, T, O>(
        &'a self,
        search_range: Range<T>,
        reversed: bool,
    ) -> impl 'a + Iterator<Item = DiagnosticEntryRef<'a, O>>
    where
        T: 'a + Clone + ToOffset,
        O: 'a + FromAnchor,
    {
        let mut iterators: Vec<_> = self
            .diagnostics
            .iter()
            .map(|(_, collection)| {
                collection
                    .range::<T, text::Anchor>(search_range.clone(), self, true, reversed)
                    .peekable()
            })
            .collect();

        std::iter::from_fn(move || {
            let (next_ix, _) = iterators
                .iter_mut()
                .enumerate()
                .flat_map(|(ix, iter)| Some((ix, iter.peek()?)))
                .min_by(|(_, a), (_, b)| {
                    let cmp = a
                        .range
                        .start
                        .cmp(&b.range.start, self)
                        // when range is equal, sort by diagnostic severity
                        .then(a.diagnostic.severity.cmp(&b.diagnostic.severity))
                        // and stabilize order with group_id
                        .then(a.diagnostic.group_id.cmp(&b.diagnostic.group_id));
                    if reversed { cmp.reverse() } else { cmp }
                })?;
            iterators[next_ix]
                .next()
                .map(
                    |DiagnosticEntryRef { range, diagnostic }| DiagnosticEntryRef {
                        diagnostic,
                        range: FromAnchor::from_anchor(&range.start, self)
                            ..FromAnchor::from_anchor(&range.end, self),
                    },
                )
        })
    }

    /// Returns all the diagnostic groups associated with the given
    /// language server ID. If no language server ID is provided,
    /// all diagnostics groups are returned.
    pub fn diagnostic_groups(
        &self,
        language_server_id: Option<LanguageServerId>,
    ) -> Vec<(LanguageServerId, DiagnosticGroup<'_, Anchor>)> {
        let mut groups = Vec::new();

        if let Some(language_server_id) = language_server_id {
            if let Some(set) = self.diagnostics.get(&language_server_id) {
                set.groups(language_server_id, &mut groups, self);
            }
        } else {
            for (language_server_id, diagnostics) in self.diagnostics.iter() {
                diagnostics.groups(*language_server_id, &mut groups, self);
            }
        }

        groups.sort_by(|(id_a, group_a), (id_b, group_b)| {
            let a_start = &group_a.entries[group_a.primary_ix].range.start;
            let b_start = &group_b.entries[group_b.primary_ix].range.start;
            a_start.cmp(b_start, self).then_with(|| id_a.cmp(id_b))
        });

        groups
    }

    /// Returns an iterator over the diagnostics for the given group.
    pub fn diagnostic_group<O>(
        &self,
        group_id: usize,
    ) -> impl Iterator<Item = DiagnosticEntryRef<'_, O>> + use<'_, O>
    where
        O: FromAnchor + 'static,
    {
        self.diagnostics
            .iter()
            .flat_map(move |(_, set)| set.group(group_id, self))
    }

    /// An integer version number that accounts for all updates besides
    /// the buffer's text itself (which is versioned via a version vector).
    pub fn non_text_state_update_count(&self) -> usize {
        self.non_text_state_update_count
    }

    /// An integer version that changes when the buffer's syntax changes.
    pub fn syntax_update_count(&self) -> usize {
        self.syntax.update_count()
    }

    /// Returns a snapshot of underlying file.
    pub fn file(&self) -> Option<&Arc<dyn File>> {
        self.file.as_ref()
    }

    pub fn resolve_file_path(&self, include_root: bool, cx: &App) -> Option<String> {
        if let Some(file) = self.file() {
            if file.path().file_name().is_none() || include_root {
                Some(file.full_path(cx).to_string_lossy().into_owned())
            } else {
                Some(file.path().display(file.path_style(cx)).to_string())
            }
        } else {
            None
        }
    }

    pub fn words_in_range(&self, query: WordsQuery) -> BTreeMap<String, Range<Anchor>> {
        let query_str = query.fuzzy_contents;
        if query_str.is_some_and(|query| query.is_empty()) {
            return BTreeMap::default();
        }

        let classifier = CharClassifier::new(self.language.clone().map(|language| LanguageScope {
            language,
            override_id: None,
        }));

        let mut query_ix = 0;
        let query_chars = query_str.map(|query| query.chars().collect::<Vec<_>>());
        let query_len = query_chars.as_ref().map_or(0, |query| query.len());

        let mut words = BTreeMap::default();
        let mut current_word_start_ix = None;
        let mut chunk_ix = query.range.start;
        for chunk in self.chunks(
            query.range,
            LanguageAwareStyling {
                tree_sitter: false,
                diagnostics: false,
            },
        ) {
            for (i, c) in chunk.text.char_indices() {
                let ix = chunk_ix + i;
                if classifier.is_word(c) {
                    if current_word_start_ix.is_none() {
                        current_word_start_ix = Some(ix);
                    }

                    if let Some(query_chars) = &query_chars
                        && query_ix < query_len
                        && c.to_lowercase().eq(query_chars[query_ix].to_lowercase())
                    {
                        query_ix += 1;
                    }
                    continue;
                } else if let Some(word_start) = current_word_start_ix.take()
                    && query_ix == query_len
                {
                    let word_range = self.anchor_before(word_start)..self.anchor_after(ix);
                    let mut word_text = self.text_for_range(word_start..ix).peekable();
                    let first_char = word_text
                        .peek()
                        .and_then(|first_chunk| first_chunk.chars().next());
                    // Skip empty and "words" starting with digits as a heuristic to reduce useless completions
                    if !query.skip_digits
                        || first_char.is_none_or(|first_char| !first_char.is_digit(10))
                    {
                        words.insert(word_text.collect(), word_range);
                    }
                }
                query_ix = 0;
            }
            chunk_ix += chunk.text.len();
        }

        words
    }
}

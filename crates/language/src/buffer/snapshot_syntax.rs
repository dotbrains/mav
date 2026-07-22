use super::*;

impl BufferSnapshot {
    pub fn captures(
        &self,
        range: Range<usize>,
        query: fn(&Grammar) -> Option<&tree_sitter::Query>,
    ) -> SyntaxMapCaptures<'_> {
        self.syntax.captures(range, &self.text, query)
    }

    #[ztracing::instrument(skip_all)]
    pub(super) fn get_highlights(
        &self,
        range: Range<usize>,
    ) -> (SyntaxMapCaptures<'_>, Vec<HighlightMap>) {
        let captures = self.syntax.captures(range, &self.text, |grammar| {
            grammar
                .highlights_config
                .as_ref()
                .map(|config| &config.query)
        });
        let highlight_maps = captures
            .grammars()
            .iter()
            .map(|grammar| grammar.highlight_map())
            .collect();
        (captures, highlight_maps)
    }

    /// Iterates over chunks of text in the given range of the buffer. Text is chunked
    /// in an arbitrary way due to being stored in a [`Rope`](text::Rope). The text is also
    /// returned in chunks where each chunk has a single syntax highlighting style and
    /// diagnostic status.
    #[ztracing::instrument(skip_all)]
    pub fn chunks<T: ToOffset>(
        &self,
        range: Range<T>,
        language_aware: LanguageAwareStyling,
    ) -> BufferChunks<'_> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);

        let mut syntax = None;
        if language_aware.tree_sitter {
            syntax = Some(self.get_highlights(range.clone()));
        }
        BufferChunks::new(
            self.text.as_rope(),
            range,
            syntax,
            language_aware.diagnostics,
            Some(self),
        )
    }

    pub fn highlighted_text_for_range<T: ToOffset>(
        &self,
        range: Range<T>,
        override_style: Option<HighlightStyle>,
        syntax_theme: &SyntaxTheme,
    ) -> HighlightedText {
        HighlightedText::from_buffer_range(
            range,
            &self.text,
            &self.syntax,
            override_style,
            syntax_theme,
        )
    }

    /// Invokes the given callback for each line of text in the given range of the buffer.
    /// Uses callback to avoid allocating a string for each line.
    pub(super) fn for_each_line(&self, range: Range<Point>, mut callback: impl FnMut(u32, &str)) {
        let mut line = String::new();
        let mut row = range.start.row;
        for chunk in self
            .as_rope()
            .chunks_in_range(range.to_offset(self))
            .chain(["\n"])
        {
            for (newline_ix, text) in chunk.split('\n').enumerate() {
                if newline_ix > 0 {
                    callback(row, &line);
                    row += 1;
                    line.clear();
                }
                line.push_str(text);
            }
        }
    }

    /// Iterates over every [`SyntaxLayer`] in the buffer.
    pub fn syntax_layers(&self) -> impl Iterator<Item = SyntaxLayer<'_>> + '_ {
        self.syntax_layers_for_range(0..self.len(), true)
    }

    pub fn syntax_layer_at<D: ToOffset>(&self, position: D) -> Option<SyntaxLayer<'_>> {
        let offset = position.to_offset(self);
        self.syntax_layers_for_range(offset..offset, false)
            .filter(|l| {
                if let Some(ranges) = l.included_sub_ranges {
                    ranges.iter().any(|range| {
                        let start = range.start.to_offset(self);
                        start <= offset && {
                            let end = range.end.to_offset(self);
                            offset < end
                        }
                    })
                } else {
                    l.node().start_byte() <= offset && l.node().end_byte() > offset
                }
            })
            .last()
    }

    pub fn syntax_layers_for_range<D: ToOffset>(
        &self,
        range: Range<D>,
        include_hidden: bool,
    ) -> impl Iterator<Item = SyntaxLayer<'_>> + '_ {
        self.syntax
            .layers_for_range(range, &self.text, include_hidden)
    }

    pub fn syntax_layers_languages(&self) -> impl Iterator<Item = &Arc<Language>> {
        self.syntax.languages(&self, true)
    }

    pub fn smallest_syntax_layer_containing<D: ToOffset>(
        &self,
        range: Range<D>,
    ) -> Option<SyntaxLayer<'_>> {
        let range = range.to_offset(self);
        self.syntax
            .layers_for_range(range, &self.text, false)
            .max_by(|a, b| {
                if a.depth != b.depth {
                    a.depth.cmp(&b.depth)
                } else if a.offset.0 != b.offset.0 {
                    a.offset.0.cmp(&b.offset.0)
                } else {
                    a.node().end_byte().cmp(&b.node().end_byte()).reverse()
                }
            })
    }

    /// Returns the [`ModelineSettings`].
    pub fn modeline(&self) -> Option<&Arc<ModelineSettings>> {
        self.modeline.as_ref()
    }

    /// Returns the main [`Language`].
    pub fn language(&self) -> Option<&Arc<Language>> {
        self.language.as_ref()
    }

    /// Returns the [`Language`] at the given location.
    pub fn language_at<D: ToOffset>(&self, position: D) -> Option<&Arc<Language>> {
        self.syntax_layer_at(position)
            .map(|info| info.language)
            .or(self.language.as_ref())
    }

    /// Returns the settings for the language at the given location.
    pub fn settings_at<'a, D: ToOffset>(
        &'a self,
        position: D,
        cx: &'a App,
    ) -> Cow<'a, LanguageSettings> {
        LanguageSettings::for_buffer_snapshot(self, Some(position.to_offset(self)), cx)
    }

    pub fn char_classifier_at<T: ToOffset>(&self, point: T) -> CharClassifier {
        CharClassifier::new(self.language_scope_at(point))
    }

    /// Returns the [`LanguageScope`] at the given location.
    pub fn language_scope_at<D: ToOffset>(&self, position: D) -> Option<LanguageScope> {
        let offset = position.to_offset(self);
        let mut scope = None;
        let mut smallest_range_and_depth: Option<(Range<usize>, usize)> = None;
        let text: &TextBufferSnapshot = self;

        // Use the layer that has the smallest node intersecting the given point.
        for layer in self
            .syntax
            .layers_for_range(offset..offset, &self.text, false)
        {
            if let Some(ranges) = layer.included_sub_ranges
                && !offset_in_sub_ranges(ranges, offset, text)
            {
                continue;
            }

            let mut cursor = layer.node().walk();

            let mut range = None;
            loop {
                let child_range = cursor.node().byte_range();
                if !child_range.contains(&offset) {
                    break;
                }

                range = Some(child_range);
                if cursor.goto_first_child_for_byte(offset).is_none() {
                    break;
                }
            }

            if let Some(range) = range
                && smallest_range_and_depth.as_ref().is_none_or(
                    |(smallest_range, smallest_range_depth)| {
                        if layer.depth > *smallest_range_depth {
                            true
                        } else if layer.depth == *smallest_range_depth {
                            range.len() < smallest_range.len()
                        } else {
                            false
                        }
                    },
                )
            {
                smallest_range_and_depth = Some((range, layer.depth));
                scope = Some(LanguageScope {
                    language: layer.language.clone(),
                    override_id: layer.override_id(offset, &self.text),
                });
            }
        }

        scope.or_else(|| {
            self.language.clone().map(|language| LanguageScope {
                language,
                override_id: None,
            })
        })
    }
}

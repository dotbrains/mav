use super::*;

impl MultiBufferSnapshot {
    pub fn file_at<T: ToOffset>(&self, point: T) -> Option<&Arc<dyn File>> {
        self.point_to_buffer_offset(point)
            .and_then(|(buffer, _)| buffer.file())
    }

    pub fn language_at<T: ToOffset>(&self, offset: T) -> Option<&Arc<Language>> {
        self.point_to_buffer_offset(offset)
            .and_then(|(buffer, offset)| buffer.language_at(offset))
    }

    pub(super) fn language_settings<'a>(&'a self, cx: &'a App) -> Cow<'a, LanguageSettings> {
        self.excerpts
            .first()
            .map(|excerpt| excerpt.buffer_snapshot(self))
            .map(|buffer| LanguageSettings::for_buffer_snapshot(buffer, None, cx))
            .unwrap_or_else(move || self.language_settings_at(MultiBufferOffset::ZERO, cx))
    }

    pub fn language_settings_at<'a, T: ToOffset>(
        &'a self,
        point: T,
        cx: &'a App,
    ) -> Cow<'a, LanguageSettings> {
        if let Some((buffer, offset)) = self.point_to_buffer_offset(point) {
            buffer.settings_at(offset, cx)
        } else {
            Cow::Borrowed(&AllLanguageSettings::get_global(cx).defaults)
        }
    }

    pub fn language_scope_at<T: ToOffset>(&self, point: T) -> Option<LanguageScope> {
        self.point_to_buffer_offset(point)
            .and_then(|(buffer, offset)| buffer.language_scope_at(offset))
    }

    pub fn char_classifier_at<T: ToOffset>(&self, point: T) -> CharClassifier {
        self.point_to_buffer_offset(point)
            .map(|(buffer, offset)| buffer.char_classifier_at(offset))
            .unwrap_or_default()
    }

    pub fn language_indent_size_at<T: ToOffset>(
        &self,
        position: T,
        cx: &App,
    ) -> Option<IndentSize> {
        let (buffer_snapshot, offset) = self.point_to_buffer_offset(position)?;
        Some(buffer_snapshot.language_indent_size_at(offset, cx))
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub fn has_deleted_file(&self) -> bool {
        self.has_deleted_file
    }

    pub fn has_conflict(&self) -> bool {
        self.has_conflict
    }

    pub fn has_diagnostics(&self) -> bool {
        self.excerpts
            .iter()
            .any(|excerpt| excerpt.buffer_snapshot(self).has_diagnostics())
    }

    pub fn diagnostic_group(
        &self,
        buffer_id: BufferId,
        group_id: usize,
    ) -> impl Iterator<Item = DiagnosticEntryRef<'_, Point>> + '_ {
        self.lift_buffer_metadata::<Point, _, _>(
            Point::zero()..self.max_point(),
            move |buffer, range| {
                if buffer.remote_id() != buffer_id {
                    return None;
                };
                Some(
                    buffer
                        .diagnostics_in_range(range, false)
                        .filter(move |diagnostic| diagnostic.diagnostic.group_id == group_id)
                        .map(move |DiagnosticEntryRef { diagnostic, range }| (range, diagnostic)),
                )
            },
        )
        .map(|(range, diagnostic, _)| DiagnosticEntryRef { diagnostic, range })
    }

    pub fn diagnostics_in_range<'a, MBD>(
        &'a self,
        range: Range<MBD>,
    ) -> impl Iterator<Item = DiagnosticEntryRef<'a, MBD>> + 'a
    where
        MBD::TextDimension: 'a
            + text::ToOffset
            + text::FromAnchor
            + Sub<Output = MBD::TextDimension>
            + fmt::Debug
            + ops::Add<Output = MBD::TextDimension>
            + ops::AddAssign
            + Ord,
        MBD: MultiBufferDimension
            + Ord
            + Sub<Output = MBD::TextDimension>
            + ops::Add<MBD::TextDimension, Output = MBD>
            + ops::AddAssign<MBD::TextDimension>
            + 'a,
    {
        self.lift_buffer_metadata::<MBD, _, _>(range, move |buffer, buffer_range| {
            Some(
                buffer
                    .diagnostics_in_range(buffer_range.start..buffer_range.end, false)
                    .map(|entry| (entry.range, entry.diagnostic)),
            )
        })
        .map(|(range, diagnostic, _)| DiagnosticEntryRef { diagnostic, range })
    }

    pub fn diagnostics_with_buffer_ids_in_range<'a, MBD>(
        &'a self,
        range: Range<MBD>,
    ) -> impl Iterator<Item = (BufferId, DiagnosticEntryRef<'a, MBD>)> + 'a
    where
        MBD: MultiBufferDimension
            + Ord
            + Sub<Output = MBD::TextDimension>
            + ops::Add<MBD::TextDimension, Output = MBD>
            + ops::AddAssign<MBD::TextDimension>,
        MBD::TextDimension: Sub<Output = MBD::TextDimension>
            + ops::Add<Output = MBD::TextDimension>
            + text::ToOffset
            + text::FromAnchor
            + AddAssign<MBD::TextDimension>
            + Ord,
    {
        self.lift_buffer_metadata::<MBD, _, _>(range, move |buffer, buffer_range| {
            Some(
                buffer
                    .diagnostics_in_range(buffer_range.start..buffer_range.end, false)
                    .map(|entry| (entry.range, entry.diagnostic)),
            )
        })
        .map(|(range, diagnostic, excerpt)| {
            (
                excerpt.buffer_snapshot(self).remote_id(),
                DiagnosticEntryRef { diagnostic, range },
            )
        })
    }

    pub fn syntax_ancestor<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> Option<(tree_sitter::Node<'_>, Range<MultiBufferOffset>)> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let results =
            self.map_excerpt_ranges(range, |buffer, excerpt_range, input_buffer_range| {
                let Some(node) = buffer.syntax_ancestor(input_buffer_range) else {
                    return vec![];
                };
                let node_range = node.byte_range();
                if excerpt_range.context.start.0 <= node_range.start
                    && node_range.end <= excerpt_range.context.end.0
                {
                    vec![(
                        BufferOffset(node_range.start)..BufferOffset(node_range.end),
                        node,
                    )]
                } else {
                    vec![]
                }
            })?;
        let (output_range, node) = results.into_iter().next()?;
        Some((node, output_range))
    }

    pub fn outline(&self, theme: Option<&SyntaxTheme>) -> Option<Outline<Anchor>> {
        let buffer_snapshot = self.as_singleton()?;
        let excerpt = self.excerpts.first()?;
        let path_key_index = excerpt.path_key_index;
        let outline = buffer_snapshot.outline(theme);
        Some(Outline::new(
            outline
                .items
                .into_iter()
                .map(|item| OutlineItem {
                    depth: item.depth,
                    range: Anchor::range_in_buffer(path_key_index, item.range),
                    selection_range: Anchor::range_in_buffer(path_key_index, item.selection_range),
                    source_range_for_text: Anchor::range_in_buffer(
                        path_key_index,
                        item.source_range_for_text,
                    ),
                    text: item.text,
                    highlight_ranges: item.highlight_ranges,
                    name_ranges: item.name_ranges,
                    body_range: item
                        .body_range
                        .map(|body_range| Anchor::range_in_buffer(path_key_index, body_range)),
                    annotation_range: item.annotation_range.map(|annotation_range| {
                        Anchor::range_in_buffer(path_key_index, annotation_range)
                    }),
                })
                .collect(),
        ))
    }

    pub fn symbols_containing<T: ToOffset>(
        &self,
        offset: T,
        theme: Option<&SyntaxTheme>,
    ) -> Option<(BufferId, Vec<OutlineItem<Anchor>>)> {
        let anchor = self.anchor_before(offset);
        let target = anchor.try_seek_target(&self)?;
        let (_, _, excerpt) = self.excerpts.find((), &target, Bias::Left);
        let excerpt = excerpt?;
        let buffer_snapshot = excerpt.buffer_snapshot(self);
        Some((
            buffer_snapshot.remote_id(),
            buffer_snapshot
                .symbols_containing(
                    anchor
                        .excerpt_anchor()
                        .map(|anchor| anchor.text_anchor())
                        .unwrap_or(text::Anchor::min_for_buffer(buffer_snapshot.remote_id())),
                    theme,
                )
                .into_iter()
                .flat_map(|item| {
                    Some(OutlineItem {
                        depth: item.depth,
                        selection_range: Anchor::range_in_buffer(
                            excerpt.path_key_index,
                            item.selection_range,
                        ),
                        source_range_for_text: Anchor::range_in_buffer(
                            excerpt.path_key_index,
                            item.source_range_for_text,
                        ),
                        range: Anchor::range_in_buffer(excerpt.path_key_index, item.range),
                        text: item.text,
                        highlight_ranges: item.highlight_ranges,
                        name_ranges: item.name_ranges,
                        body_range: item.body_range.map(|body_range| {
                            Anchor::range_in_buffer(excerpt.path_key_index, body_range)
                        }),
                        annotation_range: item.annotation_range.map(|body_range| {
                            Anchor::range_in_buffer(excerpt.path_key_index, body_range)
                        }),
                    })
                })
                .collect(),
        ))
    }
}

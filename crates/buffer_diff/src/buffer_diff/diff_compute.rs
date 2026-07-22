use super::*;

pub(super) fn build_diff_options(
    language: Option<LanguageName>,
    language_scope: Option<language::LanguageScope>,
    cx: &App,
) -> Option<DiffOptions> {
    #[cfg(any(test, feature = "test-support"))]
    {
        if !cx.has_global::<settings::SettingsStore>() {
            return Some(DiffOptions {
                language_scope,
                max_word_diff_line_count: MAX_WORD_DIFF_LINE_COUNT,
                ..Default::default()
            });
        }
    }

    LanguageSettings::resolve(None, language.as_ref(), cx)
        .word_diff_enabled
        .then_some(DiffOptions {
            language_scope,
            max_word_diff_line_count: MAX_WORD_DIFF_LINE_COUNT,
            ..Default::default()
        })
}

pub(super) fn compute_hunks(
    diff_base: Option<(Arc<str>, Rope)>,
    buffer: &text::BufferSnapshot,
    diff_options: Option<DiffOptions>,
) -> SumTree<InternalDiffHunk> {
    let mut tree = SumTree::new(buffer);

    if let Some((diff_base, diff_base_rope)) = diff_base {
        let buffer_text = buffer.as_rope().to_string();

        // A common case in Mav is that the empty buffer is represented as just a newline,
        // but if we just compute a naive diff you get a "preserved" line in the middle,
        // which is a bit odd.
        if buffer_text == "\n" && diff_base.ends_with("\n") && diff_base.len() > 1 {
            tree.push(
                InternalDiffHunk {
                    buffer_range: buffer.anchor_before(0)..buffer.anchor_before(0),
                    diff_base_byte_range: 0..diff_base.len() - 1,
                    diff_base_point_range: Point::new(0, 0)
                        ..diff_base_rope.offset_to_point(diff_base.len() - 1),
                    base_word_diffs: Vec::default(),
                    buffer_word_diffs: Vec::default(),
                },
                buffer,
            );
            return tree;
        }

        let input = InternedInput::new(
            lines_with_terminator(diff_base.as_ref()),
            lines_with_terminator(buffer_text.as_str()),
        );
        let sink = HunkSink::new(&diff_base, &diff_base_rope, buffer, diff_options.as_ref());
        let hunks = imara_diff::diff(Algorithm::Histogram, &input, sink);
        for hunk in hunks {
            tree.push(hunk, buffer);
        }
    } else {
        tree.push(
            InternalDiffHunk {
                buffer_range: Anchor::min_max_range_for_buffer(buffer.remote_id()),
                diff_base_byte_range: 0..0,
                diff_base_point_range: Point::new(0, 0)..Point::new(0, 0),
                base_word_diffs: Vec::default(),
                buffer_word_diffs: Vec::default(),
            },
            buffer,
        );
    }

    tree
}
struct HunkSink<'a> {
    diff_base_rope: &'a Rope,
    buffer: &'a text::BufferSnapshot,
    diff_options: Option<&'a DiffOptions>,
    old_line_offsets: Vec<usize>,
    hunks: Vec<InternalDiffHunk>,
}

impl<'a> HunkSink<'a> {
    fn new(
        diff_base: &'a str,
        diff_base_rope: &'a Rope,
        buffer: &'a text::BufferSnapshot,
        diff_options: Option<&'a DiffOptions>,
    ) -> Self {
        let old_line_offsets = Self::compute_line_offsets(diff_base);
        Self {
            diff_base_rope,
            buffer,
            diff_options,
            old_line_offsets,
            hunks: Vec::new(),
        }
    }

    fn compute_line_offsets(text: &str) -> Vec<usize> {
        let mut offsets = vec![0];
        let mut offset = 0;
        for line in lines_with_terminator(text) {
            offset += line.len();
            offsets.push(offset);
        }
        offsets
    }
}

impl Sink for HunkSink<'_> {
    type Out = Vec<InternalDiffHunk>;

    fn process_change(&mut self, before: Range<u32>, after: Range<u32>) {
        let old_start = before.start as usize;
        let old_end = before.end as usize;
        let new_start = after.start as usize;
        let new_end = after.end as usize;

        let diff_base_byte_range = self.old_line_offsets[old_start]..self.old_line_offsets[old_end];

        let buffer_row_range = (new_start as u32)..(new_end as u32);

        let start = Point::new(buffer_row_range.start, 0);
        let end = Point::new(buffer_row_range.end, 0);
        let buffer_range = self.buffer.anchor_before(start)..self.buffer.anchor_before(end);

        let base_line_count = old_end - old_start;
        let buffer_line_count = new_end - new_start;

        let (base_word_diffs, buffer_word_diffs) = if let Some(diff_options) = self.diff_options
            && !buffer_row_range.is_empty()
            && base_line_count == buffer_line_count
            && diff_options.max_word_diff_line_count >= base_line_count
        {
            let base_text: String = self
                .diff_base_rope
                .chunks_in_range(diff_base_byte_range.clone())
                .collect();
            let buffer_text: String = self.buffer.text_for_range(buffer_range.clone()).collect();

            let (base_word_diffs, buffer_word_diffs_relative) = word_diff_ranges(
                &base_text,
                &buffer_text,
                DiffOptions {
                    language_scope: diff_options.language_scope.clone(),
                    ..*diff_options
                },
            );

            let buffer_start_offset = buffer_range.start.to_offset(self.buffer);
            let buffer_word_diffs = buffer_word_diffs_relative
                .into_iter()
                .map(|range| {
                    let start = self.buffer.anchor_after(buffer_start_offset + range.start);
                    let end = self.buffer.anchor_after(buffer_start_offset + range.end);
                    start..end
                })
                .collect();

            (base_word_diffs, buffer_word_diffs)
        } else {
            (Vec::default(), Vec::default())
        };

        self.hunks.push(InternalDiffHunk {
            buffer_range,
            diff_base_byte_range: diff_base_byte_range.clone(),
            diff_base_point_range: self
                .diff_base_rope
                .offset_to_point(diff_base_byte_range.start)
                ..self
                    .diff_base_rope
                    .offset_to_point(diff_base_byte_range.end),
            base_word_diffs,
            buffer_word_diffs,
        });
    }

    fn finish(self) -> Self::Out {
        self.hunks
    }
}

pub(super) fn compare_hunks(
    new_hunks: &SumTree<InternalDiffHunk>,
    old_hunks: &SumTree<InternalDiffHunk>,
    old_snapshot: &text::BufferSnapshot,
    new_snapshot: &text::BufferSnapshot,
    old_base_text: &text::BufferSnapshot,
    new_base_text: &text::BufferSnapshot,
) -> DiffChanged {
    let mut new_cursor = new_hunks.cursor::<()>(new_snapshot);
    let mut old_cursor = old_hunks.cursor::<()>(new_snapshot);
    old_cursor.next();
    new_cursor.next();
    let mut start = None;
    let mut end = None;
    let mut base_text_start: Option<Anchor> = None;
    let mut base_text_end: Option<Anchor> = None;

    let mut last_unchanged_new_hunk_end: Option<text::Anchor> = None;
    let mut has_changes = false;
    let mut extended_end_candidate: Option<text::Anchor> = None;

    loop {
        match (new_cursor.item(), old_cursor.item()) {
            (Some(new_hunk), Some(old_hunk)) => {
                match new_hunk
                    .buffer_range
                    .start
                    .cmp(&old_hunk.buffer_range.start, new_snapshot)
                {
                    Ordering::Less => {
                        has_changes = true;
                        extended_end_candidate = None;
                        start.get_or_insert(new_hunk.buffer_range.start);
                        base_text_start.get_or_insert(
                            new_base_text.anchor_before(new_hunk.diff_base_byte_range.start),
                        );
                        end.replace(new_hunk.buffer_range.end);
                        let new_diff_range_end =
                            new_base_text.anchor_after(new_hunk.diff_base_byte_range.end);
                        if base_text_end.is_none_or(|base_text_end| {
                            new_diff_range_end
                                .cmp(&base_text_end, &new_base_text)
                                .is_gt()
                        }) {
                            base_text_end = Some(new_diff_range_end)
                        }
                        new_cursor.next();
                    }
                    Ordering::Equal => {
                        if new_hunk != old_hunk {
                            has_changes = true;
                            extended_end_candidate = None;
                            start.get_or_insert(new_hunk.buffer_range.start);
                            base_text_start.get_or_insert(
                                new_base_text.anchor_before(new_hunk.diff_base_byte_range.start),
                            );
                            if old_hunk
                                .buffer_range
                                .end
                                .cmp(&new_hunk.buffer_range.end, new_snapshot)
                                .is_ge()
                            {
                                end.replace(old_hunk.buffer_range.end);
                            } else {
                                end.replace(new_hunk.buffer_range.end);
                            }

                            let old_hunk_diff_base_range_end =
                                old_base_text.anchor_after(old_hunk.diff_base_byte_range.end);
                            let new_hunk_diff_base_range_end =
                                new_base_text.anchor_after(new_hunk.diff_base_byte_range.end);

                            base_text_end.replace(
                                *old_hunk_diff_base_range_end
                                    .max(&new_hunk_diff_base_range_end, new_base_text),
                            );
                        } else {
                            if !has_changes {
                                last_unchanged_new_hunk_end = Some(new_hunk.buffer_range.end);
                            } else if extended_end_candidate.is_none() {
                                extended_end_candidate = Some(new_hunk.buffer_range.start);
                            }
                        }

                        new_cursor.next();
                        old_cursor.next();
                    }
                    Ordering::Greater => {
                        has_changes = true;
                        extended_end_candidate = None;
                        start.get_or_insert(old_hunk.buffer_range.start);
                        base_text_start.get_or_insert(
                            old_base_text.anchor_after(old_hunk.diff_base_byte_range.start),
                        );
                        end.replace(old_hunk.buffer_range.end);
                        let old_diff_range_end =
                            old_base_text.anchor_after(old_hunk.diff_base_byte_range.end);
                        if base_text_end.is_none_or(|base_text_end| {
                            old_diff_range_end
                                .cmp(&base_text_end, new_base_text)
                                .is_gt()
                        }) {
                            base_text_end = Some(old_diff_range_end)
                        }
                        old_cursor.next();
                    }
                }
            }
            (Some(new_hunk), None) => {
                has_changes = true;
                extended_end_candidate = None;
                start.get_or_insert(new_hunk.buffer_range.start);
                base_text_start
                    .get_or_insert(new_base_text.anchor_after(new_hunk.diff_base_byte_range.start));
                if end.is_none_or(|end| end.cmp(&new_hunk.buffer_range.end, &new_snapshot).is_le())
                {
                    end.replace(new_hunk.buffer_range.end);
                }
                let new_base_text_end =
                    new_base_text.anchor_after(new_hunk.diff_base_byte_range.end);
                if base_text_end.is_none_or(|base_text_end| {
                    new_base_text_end.cmp(&base_text_end, new_base_text).is_gt()
                }) {
                    base_text_end = Some(new_base_text_end)
                }
                new_cursor.next();
            }
            (None, Some(old_hunk)) => {
                has_changes = true;
                extended_end_candidate = None;
                start.get_or_insert(old_hunk.buffer_range.start);
                base_text_start
                    .get_or_insert(old_base_text.anchor_after(old_hunk.diff_base_byte_range.start));
                if end.is_none_or(|end| end.cmp(&old_hunk.buffer_range.end, &new_snapshot).is_le())
                {
                    end.replace(old_hunk.buffer_range.end);
                }
                let old_base_text_end =
                    old_base_text.anchor_after(old_hunk.diff_base_byte_range.end);
                if base_text_end.is_none_or(|base_text_end| {
                    old_base_text_end.cmp(&base_text_end, new_base_text).is_gt()
                }) {
                    base_text_end = Some(old_base_text_end);
                }
                old_cursor.next();
            }
            (None, None) => break,
        }
    }

    let changed_range = start.zip(end).map(|(start, end)| start..end);
    let base_text_changed_range = base_text_start
        .zip(base_text_end)
        .map(|(start, end)| (start..end).to_offset(new_base_text));

    let extended_range = if has_changes && let Some(changed_range) = changed_range.clone() {
        let extended_start = *last_unchanged_new_hunk_end
            .unwrap_or(text::Anchor::min_for_buffer(new_snapshot.remote_id()))
            .min(&changed_range.start, new_snapshot);
        let extended_start = new_snapshot
            .anchored_edits_since_in_range::<usize>(
                &old_snapshot.version(),
                extended_start..changed_range.start,
            )
            .map(|(_, anchors)| anchors.start)
            .min_by(|a, b| a.cmp(b, new_snapshot))
            .unwrap_or(changed_range.start);

        let extended_end = *extended_end_candidate
            .unwrap_or(text::Anchor::max_for_buffer(new_snapshot.remote_id()))
            .max(&changed_range.end, new_snapshot);
        let extended_end = new_snapshot
            .anchored_edits_since_in_range::<usize>(
                &old_snapshot.version(),
                changed_range.end..extended_end,
            )
            .map(|(_, anchors)| anchors.end)
            .max_by(|a, b| a.cmp(b, new_snapshot))
            .unwrap_or(changed_range.end);

        Some(extended_start..extended_end)
    } else {
        None
    };

    DiffChanged {
        changed_range,
        base_text_changed_range,
        extended_range,
        base_text_changed: false,
    }
}

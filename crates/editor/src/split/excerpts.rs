use super::*;

impl SplittableEditor {
    pub fn update_excerpts_for_path(
        &mut self,
        path: PathKey,
        buffer: Entity<Buffer>,
        ranges: impl IntoIterator<Item = Range<Point>> + Clone,
        context_line_count: u32,
        diff: Entity<BufferDiff>,
        cx: &mut Context<Self>,
    ) -> bool {
        let has_ranges = ranges.clone().into_iter().next().is_some();
        if self.lhs.is_none() {
            return self.rhs_multibuffer.update(cx, |rhs_multibuffer, cx| {
                let added_a_new_excerpt = rhs_multibuffer.update_excerpts_for_path(
                    path,
                    buffer.clone(),
                    ranges,
                    context_line_count,
                    cx,
                );
                if has_ranges
                    && rhs_multibuffer
                        .diff_for(buffer.read(cx).remote_id())
                        .is_none_or(|old_diff| old_diff.entity_id() != diff.entity_id())
                {
                    rhs_multibuffer.add_diff(diff, cx);
                }
                added_a_new_excerpt
            });
        }

        let result = self.rhs_multibuffer.update(cx, |rhs_multibuffer, cx| {
            let added_a_new_excerpt = rhs_multibuffer.update_excerpts_for_path(
                path.clone(),
                buffer.clone(),
                ranges,
                context_line_count,
                cx,
            );
            if has_ranges
                && rhs_multibuffer
                    .diff_for(buffer.read(cx).remote_id())
                    .is_none_or(|old_diff| old_diff.entity_id() != diff.entity_id())
            {
                rhs_multibuffer.add_diff(diff.clone(), cx);
            }
            added_a_new_excerpt
        });

        self.sync_lhs_for_paths(vec![(path, diff)], cx);
        result
    }

    fn expand_excerpts(
        &mut self,
        excerpt_anchors: impl Iterator<Item = Anchor> + Clone,
        lines: u32,
        direction: ExpandExcerptDirection,
        cx: &mut Context<Self>,
    ) {
        if self.lhs.is_none() {
            self.rhs_multibuffer.update(cx, |rhs_multibuffer, cx| {
                rhs_multibuffer.expand_excerpts(excerpt_anchors, lines, direction, cx);
            });
            return;
        }

        let paths: Vec<_> = self.rhs_multibuffer.update(cx, |rhs_multibuffer, cx| {
            let snapshot = rhs_multibuffer.snapshot(cx);
            let paths = excerpt_anchors
                .clone()
                .filter_map(|anchor| {
                    let (anchor, _) = snapshot.anchor_to_buffer_anchor(anchor)?;
                    let path = snapshot.path_for_buffer(anchor.buffer_id)?;
                    let diff = rhs_multibuffer.diff_for(anchor.buffer_id)?;
                    Some((path.clone(), diff))
                })
                .collect::<HashMap<_, _>>()
                .into_iter()
                .collect();
            rhs_multibuffer.expand_excerpts(excerpt_anchors, lines, direction, cx);
            paths
        });

        self.sync_lhs_for_paths(paths, cx);
    }

    pub fn remove_excerpts_for_path(&mut self, path: PathKey, cx: &mut Context<Self>) {
        self.rhs_multibuffer.update(cx, |rhs_multibuffer, cx| {
            rhs_multibuffer.remove_excerpts(path.clone(), cx);
        });

        if let Some(lhs) = &self.lhs {
            lhs.multibuffer.update(cx, |lhs_multibuffer, cx| {
                lhs_multibuffer.remove_excerpts(path, cx);
            });
        }
    }

    fn search_token(&self) -> SearchToken {
        SearchToken::new(self.focused_side() as u64)
    }

    fn editor_for_token(&self, token: SearchToken) -> Option<&Entity<Editor>> {
        if token.value() == SplitSide::Left as u64 {
            return self.lhs.as_ref().map(|lhs| &lhs.editor);
        }
        Some(&self.rhs_editor)
    }

    fn sync_lhs_for_paths(
        &self,
        paths: Vec<(PathKey, Entity<BufferDiff>)>,
        cx: &mut Context<Self>,
    ) {
        let Some(lhs) = &self.lhs else { return };

        self.rhs_multibuffer.update(cx, |rhs_multibuffer, cx| {
            for (path, diff) in paths {
                let main_buffer_id = diff.read(cx).buffer_id;
                let Some(main_buffer) = rhs_multibuffer.buffer(diff.read(cx).buffer_id) else {
                    lhs.multibuffer.update(cx, |lhs_multibuffer, lhs_cx| {
                        lhs_multibuffer.remove_excerpts(path, lhs_cx);
                    });
                    continue;
                };
                let main_buffer_snapshot = main_buffer.read(cx).snapshot();

                let base_text_buffer = diff.read(cx).base_text_buffer().clone();
                let diff_snapshot = diff.read(cx).snapshot(cx);
                let base_text_buffer_snapshot = base_text_buffer.read(cx).snapshot();

                let mut paired_ranges: Vec<(Range<Point>, ExcerptRange<text::Anchor>)> = Vec::new();

                let mut have_excerpt = false;
                let mut did_merge = false;
                let rhs_multibuffer_snapshot = rhs_multibuffer.snapshot(cx);
                for info in rhs_multibuffer_snapshot.excerpts_for_buffer(main_buffer_id) {
                    have_excerpt = true;
                    let rhs_context = info.context.to_point(&main_buffer_snapshot);
                    let lhs_context = buffer_range_to_base_text_range(
                        &rhs_context,
                        &diff_snapshot,
                        &main_buffer_snapshot,
                    );

                    if let Some((prev_lhs_context, prev_rhs_range)) = paired_ranges.last_mut()
                        && prev_lhs_context.end >= lhs_context.start
                    {
                        did_merge = true;
                        prev_lhs_context.end = lhs_context.end;
                        prev_rhs_range.context.end = info.context.end;
                        continue;
                    }

                    paired_ranges.push((lhs_context, info));
                }

                let (lhs_ranges, rhs_ranges): (Vec<_>, Vec<_>) = paired_ranges.into_iter().unzip();
                let lhs_ranges = lhs_ranges
                    .into_iter()
                    .map(|range| {
                        ExcerptRange::new(base_text_buffer_snapshot.anchor_range_outside(range))
                    })
                    .collect::<Vec<_>>();

                lhs.multibuffer.update(cx, |lhs_multibuffer, lhs_cx| {
                    lhs_multibuffer.update_path_excerpts(
                        path.clone(),
                        base_text_buffer,
                        &base_text_buffer_snapshot,
                        &lhs_ranges,
                        lhs_cx,
                    );
                    if have_excerpt
                        && lhs_multibuffer
                            .diff_for(base_text_buffer_snapshot.remote_id())
                            .is_none_or(|old_diff| old_diff.entity_id() != diff.entity_id())
                    {
                        lhs_multibuffer.add_inverted_diff(
                            diff.clone(),
                            main_buffer.clone(),
                            lhs_cx,
                        );
                    }
                });

                if did_merge {
                    rhs_multibuffer.update_path_excerpts(
                        path,
                        main_buffer,
                        &main_buffer_snapshot,
                        &rhs_ranges,
                        cx,
                    );
                }
            }
        });
    }

    fn width_changed(&mut self, width: Pixels, window: &mut Window, cx: &mut Context<Self>) {
        self.last_width = Some(width);

        let min_ems = EditorSettings::get_global(cx).minimum_split_diff_width;

        let style = self.rhs_editor.read(cx).create_style(cx);
        let font_id = window.text_system().resolve_font(&style.text.font());
        let font_size = style.text.font_size.to_pixels(window.rem_size());
        let em_advance = window
            .text_system()
            .em_advance(font_id, font_size)
            .unwrap_or(font_size);
        let min_width = em_advance * min_ems;
        let is_split = self.lhs.is_some();

        self.too_narrow_for_split = min_ems > 0.0 && width < min_width;

        match self.diff_view_style {
            DiffViewStyle::Unified => {}
            DiffViewStyle::Split => {
                if self.too_narrow_for_split && is_split {
                    self.unsplit(window, cx);
                } else if !self.too_narrow_for_split && !is_split {
                    self.split(window, cx);
                }
            }
        }
    }

    pub fn remove_excerpts_for_buffer(
        &mut self,
        buffer_id: BufferId,
        cx: &mut Context<'_, SplittableEditor>,
    ) {
        let snapshot = self.rhs_multibuffer.read(cx).snapshot(cx);
        let Some(path) = snapshot.path_for_buffer(buffer_id) else {
            return;
        };
        self.remove_excerpts_for_path(path.clone(), cx);
    }
}

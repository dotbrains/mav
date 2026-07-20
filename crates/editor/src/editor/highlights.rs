use super::*;

impl Editor {
    pub fn set_search_within_ranges(&mut self, ranges: &[Range<Anchor>], cx: &mut Context<Self>) {
        self.highlight_background(
            HighlightKey::SearchWithinRange,
            ranges,
            |_, colors| colors.colors().editor_document_highlight_read_background,
            cx,
        )
    }

    pub fn set_breadcrumb_header(&mut self, new_header: String) {
        self.breadcrumb_header = Some(new_header);
    }

    pub fn clear_search_within_ranges(&mut self, cx: &mut Context<Self>) {
        self.clear_background_highlights(HighlightKey::SearchWithinRange, cx);
    }

    pub fn highlight_background(
        &mut self,
        key: HighlightKey,
        ranges: &[Range<Anchor>],
        color_fetcher: impl Fn(&usize, &Theme) -> Hsla + Send + Sync + 'static,
        cx: &mut Context<Self>,
    ) {
        self.background_highlights
            .insert(key, (Arc::new(color_fetcher), Arc::from(ranges)));
        self.scrollbar_marker_state.dirty = true;
        cx.notify();
    }

    pub fn clear_background_highlights(
        &mut self,
        key: HighlightKey,
        cx: &mut Context<Self>,
    ) -> Option<BackgroundHighlight> {
        let text_highlights = self.background_highlights.remove(&key)?;
        if !text_highlights.1.is_empty() {
            self.scrollbar_marker_state.dirty = true;
            cx.notify();
        }
        Some(text_highlights)
    }

    pub fn highlight_gutter<T: 'static>(
        &mut self,
        ranges: impl Into<Vec<Range<Anchor>>>,
        color_fetcher: fn(&App) -> Hsla,
        cx: &mut Context<Self>,
    ) {
        self.gutter_highlights
            .insert(TypeId::of::<T>(), (color_fetcher, ranges.into()));
        cx.notify();
    }

    pub fn clear_gutter_highlights<T: 'static>(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<GutterHighlight> {
        cx.notify();
        self.gutter_highlights.remove(&TypeId::of::<T>())
    }

    pub fn insert_gutter_highlight<T: 'static>(
        &mut self,
        range: Range<Anchor>,
        color_fetcher: fn(&App) -> Hsla,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.buffer().read(cx).snapshot(cx);
        let mut highlights = self
            .gutter_highlights
            .remove(&TypeId::of::<T>())
            .map(|(_, highlights)| highlights)
            .unwrap_or_default();
        let ix = highlights.binary_search_by(|highlight| {
            Ordering::Equal
                .then_with(|| highlight.start.cmp(&range.start, &snapshot))
                .then_with(|| highlight.end.cmp(&range.end, &snapshot))
        });
        if let Err(ix) = ix {
            highlights.insert(ix, range);
        }
        self.gutter_highlights
            .insert(TypeId::of::<T>(), (color_fetcher, highlights));
    }

    pub fn remove_gutter_highlights<T: 'static>(
        &mut self,
        ranges_to_remove: Vec<Range<Anchor>>,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.buffer().read(cx).snapshot(cx);
        let Some((color_fetcher, mut gutter_highlights)) =
            self.gutter_highlights.remove(&TypeId::of::<T>())
        else {
            return;
        };
        let mut ranges_to_remove = ranges_to_remove.iter().peekable();
        gutter_highlights.retain(|highlight| {
            while let Some(range_to_remove) = ranges_to_remove.peek() {
                match range_to_remove.end.cmp(&highlight.start, &snapshot) {
                    Ordering::Less | Ordering::Equal => {
                        ranges_to_remove.next();
                    }
                    Ordering::Greater => {
                        match range_to_remove.start.cmp(&highlight.end, &snapshot) {
                            Ordering::Less | Ordering::Equal => {
                                return false;
                            }
                            Ordering::Greater => break,
                        }
                    }
                }
            }

            true
        });
        self.gutter_highlights
            .insert(TypeId::of::<T>(), (color_fetcher, gutter_highlights));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn all_text_highlights(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<(HighlightStyle, Vec<Range<DisplayPoint>>)> {
        let snapshot = self.snapshot(window, cx);
        self.display_map.update(cx, |display_map, _| {
            display_map
                .all_text_highlights()
                .map(|(_, highlight)| {
                    let (style, ranges) = highlight.as_ref();
                    (
                        *style,
                        ranges
                            .iter()
                            .map(|range| range.clone().to_display_points(&snapshot))
                            .collect(),
                    )
                })
                .collect()
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn all_text_background_highlights(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<(Range<DisplayPoint>, Hsla)> {
        let snapshot = self.snapshot(window, cx);
        let buffer = &snapshot.buffer_snapshot();
        let start = buffer.anchor_before(MultiBufferOffset(0));
        let end = buffer.anchor_after(buffer.len());
        self.sorted_background_highlights_in_range(start..end, &snapshot, cx.theme())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn sorted_background_highlights_in_range(
        &self,
        search_range: Range<Anchor>,
        display_snapshot: &DisplaySnapshot,
        theme: &Theme,
    ) -> Vec<(Range<DisplayPoint>, Hsla)> {
        let mut res = self.background_highlights_in_range(search_range, display_snapshot, theme);
        res.sort_by(|a, b| {
            a.0.start
                .cmp(&b.0.start)
                .then_with(|| a.0.end.cmp(&b.0.end))
                .then_with(|| a.1.cmp(&b.1))
        });
        res
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn search_background_highlights(&mut self, cx: &mut Context<Self>) -> Vec<Range<Point>> {
        let snapshot = self.buffer().read(cx).snapshot(cx);

        let highlights = self
            .background_highlights
            .get(&HighlightKey::BufferSearchHighlights);

        if let Some((_color, ranges)) = highlights {
            ranges
                .iter()
                .map(|range| range.start.to_point(&snapshot)..range.end.to_point(&snapshot))
                .collect_vec()
        } else {
            vec![]
        }
    }

    pub fn has_background_highlights(&self, key: HighlightKey) -> bool {
        self.background_highlights
            .get(&key)
            .is_some_and(|(_, highlights)| !highlights.is_empty())
    }

    /// Returns all background highlights for a given range.
    ///
    /// The order of highlights is not deterministic, do sort the ranges if needed for the logic.
    pub fn background_highlights_in_range(
        &self,
        search_range: Range<Anchor>,
        display_snapshot: &DisplaySnapshot,
        theme: &Theme,
    ) -> Vec<(Range<DisplayPoint>, Hsla)> {
        let mut results = Vec::new();
        for (color_fetcher, ranges) in self.background_highlights.values() {
            let start_ix = match ranges.binary_search_by(|probe| {
                let cmp = probe
                    .end
                    .cmp(&search_range.start, &display_snapshot.buffer_snapshot());
                if cmp.is_gt() {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }) {
                Ok(i) | Err(i) => i,
            };
            for (index, range) in ranges[start_ix..].iter().enumerate() {
                if range
                    .start
                    .cmp(&search_range.end, &display_snapshot.buffer_snapshot())
                    .is_ge()
                {
                    break;
                }

                let color = color_fetcher(&(start_ix + index), theme);
                let start = range.start.to_display_point(display_snapshot);
                let end = range.end.to_display_point(display_snapshot);
                results.push((start..end, color))
            }
        }
        results
    }

    pub fn gutter_highlights_in_range(
        &self,
        search_range: Range<Anchor>,
        display_snapshot: &DisplaySnapshot,
        cx: &App,
    ) -> Vec<(Range<DisplayPoint>, Hsla)> {
        let mut results = Vec::new();
        for (color_fetcher, ranges) in self.gutter_highlights.values() {
            let color = color_fetcher(cx);
            let start_ix = match ranges.binary_search_by(|probe| {
                let cmp = probe
                    .end
                    .cmp(&search_range.start, &display_snapshot.buffer_snapshot());
                if cmp.is_gt() {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }) {
                Ok(i) | Err(i) => i,
            };
            for range in &ranges[start_ix..] {
                if range
                    .start
                    .cmp(&search_range.end, &display_snapshot.buffer_snapshot())
                    .is_ge()
                {
                    break;
                }

                let start = range.start.to_display_point(display_snapshot);
                let end = range.end.to_display_point(display_snapshot);
                results.push((start..end, color))
            }
        }
        results
    }

    /// Get the text ranges corresponding to the redaction query
    pub fn redacted_ranges(
        &self,
        search_range: Range<Anchor>,
        display_snapshot: &DisplaySnapshot,
        cx: &App,
    ) -> Vec<Range<DisplayPoint>> {
        display_snapshot
            .buffer_snapshot()
            .redacted_ranges(search_range, |file| {
                if let Some(file) = file {
                    file.is_private()
                        && EditorSettings::get(
                            Some(SettingsLocation {
                                worktree_id: file.worktree_id(cx),
                                path: file.path().as_ref(),
                            }),
                            cx,
                        )
                        .redact_private_values
                } else {
                    false
                }
            })
            .map(|range| {
                range.start.to_display_point(display_snapshot)
                    ..range.end.to_display_point(display_snapshot)
            })
            .collect()
    }

    pub fn highlight_text_key(
        &mut self,
        key: HighlightKey,
        ranges: Vec<Range<Anchor>>,
        style: HighlightStyle,
        merge: bool,
        cx: &mut Context<Self>,
    ) {
        self.display_map.update(cx, |map, cx| {
            map.highlight_text(key, ranges, style, merge, cx);
        });
        cx.notify();
    }

    pub fn highlight_text(
        &mut self,
        key: HighlightKey,
        ranges: Vec<Range<Anchor>>,
        style: HighlightStyle,
        cx: &mut Context<Self>,
    ) {
        self.display_map.update(cx, |map, cx| {
            map.highlight_text(key, ranges, style, false, cx)
        });
        cx.notify();
    }

    pub fn text_highlights<'a>(
        &'a self,
        key: HighlightKey,
        cx: &'a App,
    ) -> Option<(HighlightStyle, &'a [Range<Anchor>])> {
        self.display_map.read(cx).text_highlights(key)
    }

    pub fn set_navigation_overlays(
        &mut self,
        key: NavigationOverlayKey,
        overlays: Vec<NavigationTargetOverlay>,
        cx: &mut Context<Self>,
    ) {
        let buffer_snapshot = self.buffer.read(cx).snapshot(cx);
        let mut covered_text_ranges = overlays
            .iter()
            .filter_map(|overlay| overlay.covered_text_range.clone())
            .collect::<Vec<_>>();
        covered_text_ranges.sort_by(|left, right| {
            left.start
                .cmp(&right.start, &buffer_snapshot)
                .then_with(|| left.end.cmp(&right.end, &buffer_snapshot))
        });

        self.display_map.update(cx, |map, cx| {
            map.clear_highlights(HighlightKey::NavigationOverlay(key));
            if !covered_text_ranges.is_empty() {
                map.highlight_text(
                    HighlightKey::NavigationOverlay(key),
                    covered_text_ranges,
                    HighlightStyle {
                        fade_out: Some(1.0),
                        ..Default::default()
                    },
                    false,
                    cx,
                );
            }
        });

        if overlays.is_empty() {
            self.navigation_overlays.remove(&key);
        } else {
            self.navigation_overlays.insert(key, Arc::from(overlays));
        }

        cx.notify();
    }

    pub fn clear_navigation_overlays(&mut self, key: NavigationOverlayKey, cx: &mut Context<Self>) {
        let removed = self.navigation_overlays.remove(&key).is_some();
        let cleared = self.display_map.update(cx, |map, _| {
            map.clear_highlights(HighlightKey::NavigationOverlay(key))
        });
        if removed || cleared {
            cx.notify();
        }
    }

    pub(crate) fn navigation_overlay_sets(
        &self,
    ) -> &HashMap<NavigationOverlayKey, Arc<[NavigationTargetOverlay]>> {
        &self.navigation_overlays
    }

    pub fn clear_highlights(&mut self, key: HighlightKey, cx: &mut Context<Self>) {
        let cleared = self
            .display_map
            .update(cx, |map, _| map.clear_highlights(key));
        if cleared {
            cx.notify();
        }
    }

    pub fn clear_highlights_with(
        &mut self,
        f: &mut dyn FnMut(&HighlightKey) -> bool,
        cx: &mut Context<Self>,
    ) {
        let cleared = self
            .display_map
            .update(cx, |map, _| map.clear_highlights_with(f));
        if cleared {
            cx.notify();
        }
    }
}

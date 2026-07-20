use super::*;

impl Editor {
    /// Adds a row highlight for the given range. If a row has multiple highlights, the
    /// last highlight added will be used.
    ///
    /// If the range ends at the beginning of a line, then that line will not be highlighted.
    pub fn highlight_rows<T: 'static>(
        &mut self,
        range: Range<Anchor>,
        color: fn(&App) -> Hsla,
        options: RowHighlightOptions,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.buffer().read(cx).snapshot(cx);
        let row_highlights = self.highlighted_rows.entry(TypeId::of::<T>()).or_default();
        let ix = row_highlights.binary_search_by(|highlight| {
            Ordering::Equal
                .then_with(|| highlight.range.start.cmp(&range.start, &snapshot))
                .then_with(|| highlight.range.end.cmp(&range.end, &snapshot))
        });

        if let Err(mut ix) = ix {
            let index = post_inc(&mut self.highlight_order);

            let mut merged = false;
            if ix > 0 {
                let prev_highlight = &mut row_highlights[ix - 1];
                if prev_highlight
                    .range
                    .end
                    .cmp(&range.start, &snapshot)
                    .is_ge()
                {
                    ix -= 1;
                    if prev_highlight.range.end.cmp(&range.end, &snapshot).is_lt() {
                        prev_highlight.range.end = range.end;
                    }
                    merged = true;
                    prev_highlight.index = index;
                    prev_highlight.color = color;
                    prev_highlight.options = options;
                }
            }

            if !merged {
                row_highlights.insert(
                    ix,
                    RowHighlight {
                        range,
                        index,
                        color,
                        options,
                        type_id: TypeId::of::<T>(),
                    },
                );
            }

            while let Some(next_highlight) = row_highlights.get(ix + 1) {
                let highlight = &row_highlights[ix];
                if next_highlight
                    .range
                    .start
                    .cmp(&highlight.range.end, &snapshot)
                    .is_le()
                {
                    if next_highlight
                        .range
                        .end
                        .cmp(&highlight.range.end, &snapshot)
                        .is_gt()
                    {
                        row_highlights[ix].range.end = next_highlight.range.end;
                    }
                    row_highlights.remove(ix + 1);
                } else {
                    break;
                }
            }
        }
    }

    /// Remove any highlighted row ranges of the given type that intersect the
    /// given ranges.
    pub fn remove_highlighted_rows<T: 'static>(
        &mut self,
        ranges_to_remove: Vec<Range<Anchor>>,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.buffer().read(cx).snapshot(cx);
        let row_highlights = self.highlighted_rows.entry(TypeId::of::<T>()).or_default();
        let mut ranges_to_remove = ranges_to_remove.iter().peekable();
        row_highlights.retain(|highlight| {
            while let Some(range_to_remove) = ranges_to_remove.peek() {
                match range_to_remove.end.cmp(&highlight.range.start, &snapshot) {
                    Ordering::Less | Ordering::Equal => {
                        ranges_to_remove.next();
                    }
                    Ordering::Greater => {
                        match range_to_remove.start.cmp(&highlight.range.end, &snapshot) {
                            Ordering::Less | Ordering::Equal => {
                                return false;
                            }
                            Ordering::Greater => break,
                        }
                    }
                }
            }

            true
        })
    }

    /// Clear all anchor ranges for a certain highlight context type, so no corresponding rows will be highlighted.
    pub fn clear_row_highlights<T: 'static>(&mut self) {
        self.highlighted_rows.remove(&TypeId::of::<T>());
    }

    /// For a highlight given context type, gets all anchor ranges that will be used for row highlighting.
    pub fn highlighted_rows<'a, T: 'static>(
        &'a self,
        cx: &'a App,
    ) -> impl 'a + Iterator<Item = (Range<Anchor>, Hsla)> {
        self.highlighted_rows
            .get(&TypeId::of::<T>())
            .map_or(&[] as &[_], |vec| vec.as_slice())
            .iter()
            .map(|highlight| (highlight.range.clone(), (highlight.color)(cx)))
    }

    /// Merges all anchor ranges for all context types ever set, picking the last highlight added in case of a row conflict.
    /// Returns a map of display rows that are highlighted and their corresponding highlight color.
    /// Allows to ignore certain kinds of highlights.
    pub fn highlighted_display_rows(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> BTreeMap<DisplayRow, LineHighlight> {
        let snapshot = self.snapshot(window, cx);
        let mut used_highlight_orders = HashMap::default();
        self.highlighted_rows
            .values()
            .flat_map(|highlighted_rows| highlighted_rows.iter())
            .fold(
                BTreeMap::<DisplayRow, LineHighlight>::new(),
                |mut unique_rows, highlight| {
                    let start = highlight.range.start.to_display_point(&snapshot);
                    let end = highlight.range.end.to_display_point(&snapshot);
                    let start_row = start.row().0;
                    let end_row = if !highlight.range.end.is_max() && end.column() == 0 {
                        end.row().0.saturating_sub(1)
                    } else {
                        end.row().0
                    };
                    for row in start_row..=end_row {
                        let used_index =
                            used_highlight_orders.entry(row).or_insert(highlight.index);
                        if highlight.index >= *used_index {
                            *used_index = highlight.index;
                            unique_rows.insert(
                                DisplayRow(row),
                                LineHighlight {
                                    include_gutter: highlight.options.include_gutter,
                                    border: None,
                                    background: (highlight.color)(cx).into(),
                                    type_id: Some(highlight.type_id),
                                },
                            );
                        }
                    }
                    unique_rows
                },
            )
    }

    pub fn highlighted_display_row_for_autoscroll(
        &self,
        snapshot: &DisplaySnapshot,
    ) -> Option<DisplayRow> {
        self.highlighted_rows
            .values()
            .flat_map(|highlighted_rows| highlighted_rows.iter())
            .filter_map(|highlight| {
                if highlight.options.autoscroll {
                    Some(highlight.range.start.to_display_point(snapshot).row())
                } else {
                    None
                }
            })
            .min()
    }
}

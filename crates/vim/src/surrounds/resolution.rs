use super::*;

impl Vim {
    fn object_to_bracket_pair(
        &self,
        object: Object,
        cx: &mut Context<Self>,
    ) -> Option<BracketPair> {
        if let Some(pair) = object_to_surround_pair(object) {
            return Some(pair.to_bracket_pair());
        }

        match object {
            Object::AnyBrackets => self.any_pair(BRACKET_PAIRS, cx),
            Object::AnyQuotes => self.any_pair(QUOTE_PAIRS, cx),
            Object::MiniQuotes | Object::MiniBrackets => self.mini_pair(object, cx),
            _ => None,
        }
    }

    fn any_pair(
        &self,
        allowed_pairs: &[SurroundPair],
        cx: &mut Context<Self>,
    ) -> Option<BracketPair> {
        // If we're dealing with `AnyBrackets`, which can map to multiple bracket
        // pairs, we'll need to first determine which `BracketPair` to target.
        // As such, we keep track of the smallest range size, so that in cases
        // like `({ name: "John" })` if the cursor is inside the curly brackets,
        // we target the curly brackets instead of the parentheses.
        let mut best_pair = None;
        let mut min_range_size = usize::MAX;

        let _ = self.editor.update(cx, |editor, cx| {
            let display_map = editor.display_snapshot(cx);
            let selections = editor.selections.all_adjusted_display(&display_map);
            // Even if there's multiple cursors, we'll simply rely on the first one
            // to understand what bracket pair to map to. I believe we could, if
            // worth it, go one step above and have a `BracketPair` per selection, so
            // that `AnyBracket` could work in situations where the transformation
            // below could be done.
            //
            // ```
            // (< name:ˇ'Mav' >)
            // <[ name:ˇ'DeltaDB' ]>
            // ```
            //
            // After using `csb{`:
            //
            // ```
            // (ˇ{ name:'Mav' })
            // <ˇ{ name:'DeltaDB' }>
            // ```
            if let Some(selection) = selections.first() {
                let relative_to = selection.head();
                let cursor_offset = relative_to.to_offset(&display_map, Bias::Left);

                for pair in allowed_pairs {
                    if let Some(range) = surrounding_markers(
                        &display_map,
                        relative_to,
                        true,
                        false,
                        pair.open,
                        pair.close,
                    ) {
                        let start_offset = range.start.to_offset(&display_map, Bias::Left);
                        let end_offset = range.end.to_offset(&display_map, Bias::Right);

                        if cursor_offset >= start_offset && cursor_offset <= end_offset {
                            let size = end_offset - start_offset;
                            if size < min_range_size {
                                min_range_size = size;
                                best_pair = Some(*pair);
                            }
                        }
                    }
                }
            }
        });

        best_pair.map(|p| p.to_bracket_pair())
    }

    fn mini_pair(&self, object: Object, cx: &mut Context<Self>) -> Option<BracketPair> {
        self.editor
            .update(cx, |editor, cx| {
                let display_map = editor.display_snapshot(cx);
                let selections = editor.selections.all_adjusted_display(&display_map);
                // For now, only primary selection is used to select the bracket/quote pair. It might be weird
                // if multi-select resulted in different quote kinds being replaced for different selections.
                // any_pair uses the same logic, so this should be consistent across {Any,Mini}{Quotes,Brackets}
                let selection = selections.first()?.clone();
                let range = object.range(&display_map, selection, true, None)?;
                let start_offset = range.start.to_offset(&display_map, Bias::Left);
                let (pair_char, _) = display_map.buffer_chars_at(start_offset).next()?;
                literal_surround_pair(pair_char)
            })
            .ok()
            .flatten()
            .map(|surround| surround.to_bracket_pair())
    }
}

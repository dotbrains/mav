use super::*;

impl Editor {
    pub(crate) fn set_use_auto_surround(&mut self, auto_surround: bool) {
        self.use_auto_surround = auto_surround;
    }

    pub(crate) fn find_possible_emoji_shortcode_at_position(
        snapshot: &MultiBufferSnapshot,
        position: Point,
    ) -> Option<String> {
        let mut chars = Vec::new();
        let mut found_colon = false;
        for char in snapshot.reversed_chars_at(position).take(100) {
            // Found a possible emoji shortcode in the middle of the buffer
            if found_colon {
                if char.is_whitespace() {
                    chars.reverse();
                    return Some(chars.iter().collect());
                }
                // If the previous character is not a whitespace, we are in the middle of a word
                // and we only want to complete the shortcode if the word is made up of other emojis
                let mut containing_word = String::new();
                for ch in snapshot
                    .reversed_chars_at(position)
                    .skip(chars.len() + 1)
                    .take(100)
                {
                    if ch.is_whitespace() {
                        break;
                    }
                    containing_word.push(ch);
                }
                let containing_word = containing_word.chars().rev().collect::<String>();
                if util::word_consists_of_emojis(containing_word.as_str()) {
                    chars.reverse();
                    return Some(chars.iter().collect());
                }
            }

            if char.is_whitespace() || !char.is_ascii() {
                return None;
            }
            if char == ':' {
                found_colon = true;
            } else {
                chars.push(char);
            }
        }
        // Found a possible emoji shortcode at the beginning of the buffer
        chars.reverse();
        Some(chars.iter().collect())
    }

    /// Iterate the given selections, and for each one, find the smallest surrounding
    /// autoclose region. This uses the ordering of the selections and the autoclose
    /// regions to avoid repeated comparisons.
    pub(crate) fn selections_with_autoclose_regions<'a, D: ToOffset + Clone>(
        &'a self,
        selections: impl IntoIterator<Item = Selection<D>>,
        buffer: &'a MultiBufferSnapshot,
    ) -> impl Iterator<Item = (Selection<D>, Option<&'a AutocloseRegion>)> {
        let mut i = 0;
        let mut regions = self.autoclose_regions.as_slice();
        selections.into_iter().map(move |selection| {
            let range = selection.start.to_offset(buffer)..selection.end.to_offset(buffer);

            let mut enclosing = None;
            while let Some(pair_state) = regions.get(i) {
                if pair_state.range.end.to_offset(buffer) < range.start {
                    regions = &regions[i + 1..];
                    i = 0;
                } else if pair_state.range.start.to_offset(buffer) > range.end {
                    break;
                } else {
                    if pair_state.selection_id == selection.id {
                        enclosing = Some(pair_state);
                    }
                    i += 1;
                }
            }

            (selection, enclosing)
        })
    }

    pub(crate) fn try_insert_snippet_at_selections(
        &mut self,
        action: &InsertSnippet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let insertion_ranges = self
            .selections
            .all::<MultiBufferOffset>(&self.display_snapshot(cx))
            .into_iter()
            .map(|selection| selection.range())
            .collect_vec();

        let snippet = if let Some(snippet_body) = &action.snippet {
            if action.language.is_none() && action.name.is_none() {
                Snippet::parse(snippet_body)?
            } else {
                bail!("`snippet` is mutually exclusive with `language` and `name`")
            }
        } else if let Some(name) = &action.name {
            let project = self.project().context("no project")?;
            let snippet_store = project.read(cx).snippets().read(cx);
            let snippet = snippet_store
                .snippets_for(action.language.clone(), cx)
                .into_iter()
                .find(|snippet| snippet.name == *name)
                .context("snippet not found")?;
            Snippet::parse(&snippet.body)?
        } else {
            // todo(andrew): open modal to select snippet
            bail!("`name` or `snippet` is required")
        };

        self.insert_snippet(&insertion_ranges, snippet, window, cx)
    }
}

use super::*;

impl BufferSnapshot {
    /// Returns a tuple of the range and character kind of the word
    /// surrounding the given position.
    pub fn surrounding_word<T: ToOffset>(
        &self,
        start: T,
        scope_context: Option<CharScopeContext>,
    ) -> (Range<usize>, Option<CharKind>) {
        let mut start = start.to_offset(self);
        let mut end = start;
        let mut next_chars = self.chars_at(start).take(128).peekable();
        let mut prev_chars = self.reversed_chars_at(start).take(128).peekable();

        let classifier = self.char_classifier_at(start).scope_context(scope_context);
        let word_kind = cmp::max(
            prev_chars.peek().copied().map(|c| classifier.kind(c)),
            next_chars.peek().copied().map(|c| classifier.kind(c)),
        );

        for ch in prev_chars {
            if Some(classifier.kind(ch)) == word_kind && ch != '\n' {
                start -= ch.len_utf8();
            } else {
                break;
            }
        }

        for ch in next_chars {
            if Some(classifier.kind(ch)) == word_kind && ch != '\n' {
                end += ch.len_utf8();
            } else {
                break;
            }
        }

        (start..end, word_kind)
    }

    /// Moves the TreeCursor to the smallest descendant or ancestor syntax node enclosing the given
    /// range. When `require_larger` is true, the node found must be larger than the query range.
    ///
    /// Returns true if a node was found, and false otherwise. In the `false` case the cursor will
    /// be moved to the root of the tree.
    fn goto_node_enclosing_range(
        cursor: &mut tree_sitter::TreeCursor,
        query_range: &Range<usize>,
        require_larger: bool,
    ) -> bool {
        let mut ascending = false;
        loop {
            let mut range = cursor.node().byte_range();
            if query_range.is_empty() {
                // When the query range is empty and the current node starts after it, move to the
                // previous sibling to find the node the containing node.
                if range.start > query_range.start {
                    cursor.goto_previous_sibling();
                    range = cursor.node().byte_range();
                }
            } else {
                // When the query range is non-empty and the current node ends exactly at the start,
                // move to the next sibling to find a node that extends beyond the start.
                if range.end == query_range.start {
                    cursor.goto_next_sibling();
                    range = cursor.node().byte_range();
                }
            }

            let encloses = range.contains_inclusive(query_range)
                && (!require_larger || range.len() > query_range.len());
            if !encloses {
                ascending = true;
                if !cursor.goto_parent() {
                    return false;
                }
                continue;
            } else if ascending {
                return true;
            }

            // Descend into the current node.
            if cursor
                .goto_first_child_for_byte(query_range.start)
                .is_none()
            {
                return true;
            }
        }
    }

    pub fn syntax_ancestor<'a, T: ToOffset>(
        &'a self,
        range: Range<T>,
    ) -> Option<tree_sitter::Node<'a>> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let mut result: Option<tree_sitter::Node<'a>> = None;
        for layer in self
            .syntax
            .layers_for_range(range.clone(), &self.text, true)
        {
            let mut cursor = layer.node().walk();

            // Find the node that both contains the range and is larger than it.
            if !Self::goto_node_enclosing_range(&mut cursor, &range, true) {
                continue;
            }

            let left_node = cursor.node();
            let mut layer_result = left_node;

            // For an empty range, try to find another node immediately to the right of the range.
            if left_node.end_byte() == range.start {
                let mut right_node = None;
                while !cursor.goto_next_sibling() {
                    if !cursor.goto_parent() {
                        break;
                    }
                }

                while cursor.node().start_byte() == range.start {
                    right_node = Some(cursor.node());
                    if !cursor.goto_first_child() {
                        break;
                    }
                }

                // If there is a candidate node on both sides of the (empty) range, then
                // decide between the two by favoring a named node over an anonymous token.
                // If both nodes are the same in that regard, favor the right one.
                if let Some(right_node) = right_node
                    && (right_node.is_named() || !left_node.is_named())
                {
                    layer_result = right_node;
                }
            }

            if let Some(previous_result) = &result
                && previous_result.byte_range().len() < layer_result.byte_range().len()
            {
                continue;
            }
            result = Some(layer_result);
        }

        result
    }

    /// Find the previous sibling syntax node at the given range.
    ///
    /// This function locates the syntax node that precedes the node containing
    /// the given range. It searches hierarchically by:
    /// 1. Finding the node that contains the given range
    /// 2. Looking for the previous sibling at the same tree level
    /// 3. If no sibling is found, moving up to parent levels and searching for siblings
    ///
    /// Returns `None` if there is no previous sibling at any ancestor level.
    pub fn syntax_prev_sibling<'a, T: ToOffset>(
        &'a self,
        range: Range<T>,
    ) -> Option<tree_sitter::Node<'a>> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let mut result: Option<tree_sitter::Node<'a>> = None;

        for layer in self
            .syntax
            .layers_for_range(range.clone(), &self.text, true)
        {
            let mut cursor = layer.node().walk();

            // Find the node that contains the range
            if !Self::goto_node_enclosing_range(&mut cursor, &range, false) {
                continue;
            }

            // Look for the previous sibling, moving up ancestor levels if needed
            loop {
                if cursor.goto_previous_sibling() {
                    let layer_result = cursor.node();

                    if let Some(previous_result) = &result {
                        if previous_result.byte_range().end < layer_result.byte_range().end {
                            continue;
                        }
                    }
                    result = Some(layer_result);
                    break;
                }

                // No sibling found at this level, try moving up to parent
                if !cursor.goto_parent() {
                    break;
                }
            }
        }

        result
    }

    /// Find the next sibling syntax node at the given range.
    ///
    /// This function locates the syntax node that follows the node containing
    /// the given range. It searches hierarchically by:
    /// 1. Finding the node that contains the given range
    /// 2. Looking for the next sibling at the same tree level
    /// 3. If no sibling is found, moving up to parent levels and searching for siblings
    ///
    /// Returns `None` if there is no next sibling at any ancestor level.
    pub fn syntax_next_sibling<'a, T: ToOffset>(
        &'a self,
        range: Range<T>,
    ) -> Option<tree_sitter::Node<'a>> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let mut result: Option<tree_sitter::Node<'a>> = None;

        for layer in self
            .syntax
            .layers_for_range(range.clone(), &self.text, true)
        {
            let mut cursor = layer.node().walk();

            // Find the node that contains the range
            if !Self::goto_node_enclosing_range(&mut cursor, &range, false) {
                continue;
            }

            // Look for the next sibling, moving up ancestor levels if needed
            loop {
                if cursor.goto_next_sibling() {
                    let layer_result = cursor.node();

                    if let Some(previous_result) = &result {
                        if previous_result.byte_range().start > layer_result.byte_range().start {
                            continue;
                        }
                    }
                    result = Some(layer_result);
                    break;
                }

                // No sibling found at this level, try moving up to parent
                if !cursor.goto_parent() {
                    break;
                }
            }
        }

        result
    }

    /// Returns the root syntax node within the given row
    pub fn syntax_root_ancestor(&self, position: Anchor) -> Option<tree_sitter::Node<'_>> {
        let start_offset = position.to_offset(self);

        let row = self.summary_for_anchor::<text::PointUtf16>(&position).row as usize;

        let layer = self
            .syntax
            .layers_for_range(start_offset..start_offset, &self.text, true)
            .next()?;

        let mut cursor = layer.node().walk();

        // Descend to the first leaf that touches the start of the range.
        while cursor.goto_first_child_for_byte(start_offset).is_some() {
            if cursor.node().end_byte() == start_offset {
                cursor.goto_next_sibling();
            }
        }

        // Ascend to the root node within the same row.
        while cursor.goto_parent() {
            if cursor.node().start_position().row != row {
                break;
            }
        }

        Some(cursor.node())
    }
}

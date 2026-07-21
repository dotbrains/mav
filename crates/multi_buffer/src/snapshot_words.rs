use super::*;

impl MultiBufferSnapshot {
    pub fn is_inside_word<T: ToOffset>(
        &self,
        position: T,
        scope_context: Option<CharScopeContext>,
    ) -> bool {
        let position = position.to_offset(self);
        let classifier = self
            .char_classifier_at(position)
            .scope_context(scope_context);
        let next_char_kind = self.chars_at(position).next().map(|c| classifier.kind(c));
        let prev_char_kind = self
            .reversed_chars_at(position)
            .next()
            .map(|c| classifier.kind(c));
        prev_char_kind.zip(next_char_kind) == Some((CharKind::Word, CharKind::Word))
    }

    pub fn surrounding_word<T: ToOffset>(
        &self,
        start: T,
        scope_context: Option<CharScopeContext>,
    ) -> (Range<MultiBufferOffset>, Option<CharKind>) {
        let mut start = start.to_offset(self);
        let mut end = start;
        let mut next_chars = self.chars_at(start).peekable();
        let mut prev_chars = self.reversed_chars_at(start).peekable();

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

    pub fn char_kind_before<T: ToOffset>(
        &self,
        start: T,
        scope_context: Option<CharScopeContext>,
    ) -> Option<CharKind> {
        let start = start.to_offset(self);
        let classifier = self.char_classifier_at(start).scope_context(scope_context);
        self.reversed_chars_at(start)
            .next()
            .map(|ch| classifier.kind(ch))
    }
}

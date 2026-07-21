use super::*;

#[cfg(any(test, feature = "test-support"))]
pub struct TestFile {
    pub path: Arc<RelPath>,
    pub root_name: String,
    pub local_root: Option<PathBuf>,
}

#[cfg(any(test, feature = "test-support"))]
impl File for TestFile {
    fn path(&self) -> &Arc<RelPath> {
        &self.path
    }

    fn full_path(&self, _: &gpui::App) -> PathBuf {
        PathBuf::from(self.root_name.clone()).join(self.path.as_std_path())
    }

    fn as_local(&self) -> Option<&dyn LocalFile> {
        if self.local_root.is_some() {
            Some(self)
        } else {
            None
        }
    }

    fn disk_state(&self) -> DiskState {
        unimplemented!()
    }

    fn file_name<'a>(&'a self, _: &'a gpui::App) -> &'a str {
        self.path().file_name().unwrap_or(self.root_name.as_ref())
    }

    fn worktree_id(&self, _: &App) -> WorktreeId {
        WorktreeId::from_usize(0)
    }

    fn to_proto(&self, _: &App) -> rpc::proto::File {
        unimplemented!()
    }

    fn is_private(&self) -> bool {
        false
    }

    fn path_style(&self, _cx: &App) -> PathStyle {
        PathStyle::local()
    }
}

#[cfg(any(test, feature = "test-support"))]
impl LocalFile for TestFile {
    fn abs_path(&self, _cx: &App) -> PathBuf {
        PathBuf::from(self.local_root.as_ref().unwrap())
            .join(&self.root_name)
            .join(self.path.as_std_path())
    }

    fn load(&self, _cx: &App) -> Task<Result<String>> {
        unimplemented!()
    }

    fn load_bytes(&self, _cx: &App) -> Task<Result<Vec<u8>>> {
        unimplemented!()
    }
}

pub(crate) fn contiguous_ranges(
    values: impl Iterator<Item = u32>,
    max_len: usize,
) -> impl Iterator<Item = Range<u32>> {
    let mut values = values;
    let mut current_range: Option<Range<u32>> = None;
    std::iter::from_fn(move || {
        loop {
            if let Some(value) = values.next() {
                if let Some(range) = &mut current_range
                    && value == range.end
                    && range.len() < max_len
                {
                    range.end += 1;
                    continue;
                }

                let prev_range = current_range.clone();
                current_range = Some(value..(value + 1));
                if prev_range.is_some() {
                    return prev_range;
                }
            } else {
                return current_range.take();
            }
        }
    })
}

#[derive(Default, Debug)]
pub struct CharClassifier {
    scope: Option<LanguageScope>,
    scope_context: Option<CharScopeContext>,
    ignore_punctuation: bool,
}

impl CharClassifier {
    pub fn new(scope: Option<LanguageScope>) -> Self {
        Self {
            scope,
            scope_context: None,
            ignore_punctuation: false,
        }
    }

    pub fn scope_context(self, scope_context: Option<CharScopeContext>) -> Self {
        Self {
            scope_context,
            ..self
        }
    }

    pub fn ignore_punctuation(self, ignore_punctuation: bool) -> Self {
        Self {
            ignore_punctuation,
            ..self
        }
    }

    pub fn is_whitespace(&self, c: char) -> bool {
        self.kind(c) == CharKind::Whitespace
    }

    pub fn is_word(&self, c: char) -> bool {
        self.kind(c) == CharKind::Word
    }

    pub fn is_punctuation(&self, c: char) -> bool {
        self.kind(c) == CharKind::Punctuation
    }

    pub fn kind_with(&self, c: char, ignore_punctuation: bool) -> CharKind {
        if c.is_alphanumeric() || c == '_' {
            return CharKind::Word;
        }

        if let Some(scope) = &self.scope {
            let characters = match self.scope_context {
                Some(CharScopeContext::Completion) => scope.completion_query_characters(),
                Some(CharScopeContext::LinkedEdit) => scope.linked_edit_characters(),
                None => scope.word_characters(),
            };
            if let Some(characters) = characters
                && characters.contains(&c)
            {
                return CharKind::Word;
            }
        }

        if c.is_whitespace() {
            return CharKind::Whitespace;
        }

        if ignore_punctuation {
            CharKind::Word
        } else {
            CharKind::Punctuation
        }
    }

    pub fn kind(&self, c: char) -> CharKind {
        self.kind_with(c, self.ignore_punctuation)
    }
}

/// Find all of the ranges of whitespace that occur at the ends of lines
/// in the given rope.
///
/// This could also be done with a regex search, but this implementation
/// avoids copying text.
pub fn trailing_whitespace_ranges(rope: &Rope) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();

    let mut offset = 0;
    let mut prev_chunk_trailing_whitespace_range = 0..0;
    for chunk in rope.chunks() {
        let mut prev_line_trailing_whitespace_range = 0..0;
        for (i, line) in chunk.split('\n').enumerate() {
            let line_end_offset = offset + line.len();
            let trimmed_line_len = line.trim_end_matches([' ', '\t']).len();
            let mut trailing_whitespace_range = (offset + trimmed_line_len)..line_end_offset;

            if i == 0 && trimmed_line_len == 0 {
                trailing_whitespace_range.start = prev_chunk_trailing_whitespace_range.start;
            }
            if !prev_line_trailing_whitespace_range.is_empty() {
                ranges.push(prev_line_trailing_whitespace_range);
            }

            offset = line_end_offset + 1;
            prev_line_trailing_whitespace_range = trailing_whitespace_range;
        }

        offset -= 1;
        prev_chunk_trailing_whitespace_range = prev_line_trailing_whitespace_range;
    }

    if !prev_chunk_trailing_whitespace_range.is_empty() {
        ranges.push(prev_chunk_trailing_whitespace_range);
    }

    ranges
}

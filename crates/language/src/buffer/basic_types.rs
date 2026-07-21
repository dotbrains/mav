use super::*;

/// Indicate whether a [`Buffer`] has permissions to edit.
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Capability {
    /// The buffer is a mutable replica.
    ReadWrite,
    /// The buffer is a mutable replica, but toggled to be only readable.
    Read,
    /// The buffer is a read-only replica.
    ReadOnly,
}

impl Capability {
    /// Returns `true` if the capability is `ReadWrite`.
    pub fn editable(self) -> bool {
        matches!(self, Capability::ReadWrite)
    }
}

pub type BufferRow = u32;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ParseStatus {
    Idle,
    Parsing,
}

/// The kind and amount of indentation in a particular line. For now,
/// assumes that indentation is all the same character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct IndentSize {
    /// The number of bytes that comprise the indentation.
    pub len: u32,
    /// The kind of whitespace used for indentation.
    pub kind: IndentKind,
}

/// A whitespace character that's used for indentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum IndentKind {
    /// An ASCII space character.
    #[default]
    Space,
    /// An ASCII tab character.
    Tab,
}

/// The shape of a selection cursor.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum CursorShape {
    /// A vertical bar
    #[default]
    Bar,
    /// A block that surrounds the following character
    Block,
    /// An underline that runs along the following character
    Underline,
    /// A box drawn around the following character
    Hollow,
}

impl From<settings::CursorShape> for CursorShape {
    fn from(shape: settings::CursorShape) -> Self {
        match shape {
            settings::CursorShape::Bar => CursorShape::Bar,
            settings::CursorShape::Block => CursorShape::Block,
            settings::CursorShape::Underline => CursorShape::Underline,
            settings::CursorShape::Hollow => CursorShape::Hollow,
        }
    }
}

/// A class of characters, used for characterizing a run of text.
#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Debug)]
pub enum CharKind {
    /// Whitespace.
    Whitespace,
    /// Punctuation.
    Punctuation,
    /// Word.
    Word,
}

/// Context for character classification within a specific scope.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CharScopeContext {
    /// Character classification for completion queries.
    ///
    /// This context treats certain characters as word constituents that would
    /// normally be considered punctuation, such as '-' in Tailwind classes
    /// ("bg-yellow-100") or '.' in import paths ("foo.ts").
    Completion,
    /// Character classification for linked edits.
    ///
    /// This context handles characters that should be treated as part of
    /// identifiers during linked editing operations, such as '.' in JSX
    /// component names like `<Animated.View>`.
    LinkedEdit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BracketMatch<T> {
    pub open_range: Range<T>,
    pub close_range: Range<T>,
    pub newline_only: bool,
    pub syntax_layer_depth: usize,
    pub color_index: Option<usize>,
}

impl<T> BracketMatch<T> {
    pub fn bracket_ranges(self) -> (Range<T>, Range<T>) {
        (self.open_range, self.close_range)
    }
}

impl IndentSize {
    /// Returns an [`IndentSize`] representing the given spaces.
    pub fn spaces(len: u32) -> Self {
        Self {
            len,
            kind: IndentKind::Space,
        }
    }

    /// Returns an [`IndentSize`] representing a tab.
    pub fn tab() -> Self {
        Self {
            len: 1,
            kind: IndentKind::Tab,
        }
    }

    /// An iterator over the characters represented by this [`IndentSize`].
    pub fn chars(&self) -> impl Iterator<Item = char> {
        iter::repeat(self.char()).take(self.len as usize)
    }

    /// The character representation of this [`IndentSize`].
    pub fn char(&self) -> char {
        match self.kind {
            IndentKind::Space => ' ',
            IndentKind::Tab => '\t',
        }
    }

    /// Consumes the current [`IndentSize`] and returns a new one that has
    /// been shrunk or enlarged by the given size along the given direction.
    pub fn with_delta(mut self, direction: Ordering, size: IndentSize) -> Self {
        match direction {
            Ordering::Less => {
                if self.kind == size.kind && self.len >= size.len {
                    self.len -= size.len;
                }
            }
            Ordering::Equal => {}
            Ordering::Greater => {
                if self.len == 0 {
                    self = size;
                } else if self.kind == size.kind {
                    self.len += size.len;
                }
            }
        }
        self
    }

    /// Returns the number of indentation characters to remove when outdenting to the
    /// previous editor tab stop.
    pub fn outdent_len(self, tab_size: NonZeroU32) -> u32 {
        if self.len == 0 {
            return 0;
        }

        match self.kind {
            IndentKind::Space => {
                let tab_size = tab_size.get();
                let columns_to_prev_tab_stop = self.len % tab_size;
                if columns_to_prev_tab_stop == 0 {
                    tab_size
                } else {
                    columns_to_prev_tab_stop
                }
            }
            IndentKind::Tab => 1,
        }
    }

    pub fn len_with_expanded_tabs(&self, tab_size: NonZeroU32) -> usize {
        match self.kind {
            IndentKind::Space => self.len as usize,
            IndentKind::Tab => self.len as usize * tab_size.get() as usize,
        }
    }
}

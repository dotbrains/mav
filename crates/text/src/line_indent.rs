use rope::Chunks;

/// Stores information about the indentation of a line (tabs and spaces).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineIndent {
    pub tabs: u32,
    pub spaces: u32,
    pub line_blank: bool,
}

impl LineIndent {
    pub fn from_chunks(chunks: &mut Chunks) -> Self {
        let mut tabs = 0;
        let mut spaces = 0;
        let mut line_blank = true;

        'outer: while let Some(chunk) = chunks.peek() {
            for ch in chunk.chars() {
                if ch == '\t' {
                    tabs += 1;
                } else if ch == ' ' {
                    spaces += 1;
                } else {
                    if ch != '\n' {
                        line_blank = false;
                    }
                    break 'outer;
                }
            }

            chunks.next();
        }

        Self {
            tabs,
            spaces,
            line_blank,
        }
    }

    /// Constructs a new `LineIndent` which only contains spaces.
    pub fn spaces(spaces: u32) -> Self {
        Self {
            tabs: 0,
            spaces,
            line_blank: true,
        }
    }

    /// Constructs a new `LineIndent` which only contains tabs.
    pub fn tabs(tabs: u32) -> Self {
        Self {
            tabs,
            spaces: 0,
            line_blank: true,
        }
    }

    /// Indicates whether the line is empty.
    pub fn is_line_empty(&self) -> bool {
        self.tabs == 0 && self.spaces == 0 && self.line_blank
    }

    /// Indicates whether the line is blank (contains only whitespace).
    pub fn is_line_blank(&self) -> bool {
        self.line_blank
    }

    /// Returns the number of indentation characters (tabs or spaces).
    pub fn raw_len(&self) -> u32 {
        self.tabs + self.spaces
    }

    /// Returns the number of indentation characters (tabs or spaces), taking tab size into account.
    pub fn len(&self, tab_size: u32) -> u32 {
        self.tabs * tab_size + self.spaces
    }
}

impl From<&str> for LineIndent {
    fn from(value: &str) -> Self {
        Self::from_iter(value.chars())
    }
}

impl FromIterator<char> for LineIndent {
    fn from_iter<T: IntoIterator<Item = char>>(chars: T) -> Self {
        let mut tabs = 0;
        let mut spaces = 0;
        let mut line_blank = true;
        for c in chars {
            if c == '\t' {
                tabs += 1;
            } else if c == ' ' {
                spaces += 1;
            } else {
                if c != '\n' {
                    line_blank = false;
                }
                break;
            }
        }
        Self {
            tabs,
            spaces,
            line_blank,
        }
    }
}

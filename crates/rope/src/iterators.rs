use super::*;

pub struct ChunkBitmaps<'a> {
    /// A slice of text up to 128 bytes in size
    pub text: &'a str,
    /// Bitmap of character locations in text. LSB ordered
    pub chars: Bitmap,
    /// Bitmap of tab locations in text. LSB ordered
    pub tabs: Bitmap,
    /// Bitmap of newlines location in text. LSB ordered
    pub newlines: Bitmap,
}

#[derive(Clone)]
pub struct Chunks<'a> {
    chunks: sum_tree::Cursor<'a, 'static, Chunk, usize>,
    range: Range<usize>,
    offset: usize,
    reversed: bool,
}

impl<'a> Chunks<'a> {
    pub fn new(rope: &'a Rope, range: Range<usize>, reversed: bool) -> Self {
        let mut chunks = rope.chunks.cursor(());
        let offset = if reversed {
            chunks.seek(&range.end, Bias::Left);
            range.end
        } else {
            chunks.seek(&range.start, Bias::Right);
            range.start
        };
        let chunk_offset = offset - chunks.start();
        if let Some(chunk) = chunks.item() {
            chunk.assert_char_boundary::<true>(chunk_offset);
        }
        Self {
            chunks,
            range,
            offset,
            reversed,
        }
    }

    fn offset_is_valid(&self) -> bool {
        if self.reversed {
            if self.offset <= self.range.start || self.offset > self.range.end {
                return false;
            }
        } else if self.offset < self.range.start || self.offset >= self.range.end {
            return false;
        }

        true
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn seek(&mut self, mut offset: usize) {
        offset = offset.clamp(self.range.start, self.range.end);

        if self.reversed {
            if offset > self.chunks.end() {
                self.chunks.seek_forward(&offset, Bias::Left);
            } else if offset <= *self.chunks.start() {
                self.chunks.seek(&offset, Bias::Left);
            }
        } else {
            if offset >= self.chunks.end() {
                self.chunks.seek_forward(&offset, Bias::Right);
            } else if offset < *self.chunks.start() {
                self.chunks.seek(&offset, Bias::Right);
            }
        };

        self.offset = offset;
    }

    pub fn set_range(&mut self, range: Range<usize>) {
        self.range = range.clone();
        self.seek(range.start);
    }

    /// Moves this cursor to the start of the next line in the rope.
    ///
    /// This method advances the cursor to the beginning of the next line.
    /// If the cursor is already at the end of the rope, this method does nothing.
    /// Reversed chunks iterators are not currently supported and will panic.
    ///
    /// Returns `true` if the cursor was successfully moved to the next line start,
    /// or `false` if the cursor was already at the end of the rope.
    pub fn next_line(&mut self) -> bool {
        assert!(!self.reversed);

        let mut found = false;
        if let Some(chunk) = self.peek() {
            if let Some(newline_ix) = chunk.find('\n') {
                self.offset += newline_ix + 1;
                found = self.offset <= self.range.end;
            } else {
                self.chunks
                    .search_forward(|summary| summary.text.lines.row > 0);
                self.offset = *self.chunks.start();

                if let Some(newline_ix) = self.peek().and_then(|chunk| chunk.find('\n')) {
                    self.offset += newline_ix + 1;
                    found = self.offset <= self.range.end;
                } else {
                    self.offset = self.chunks.end();
                }
            }

            if self.offset == self.chunks.end() {
                self.next();
            }
        }

        if self.offset > self.range.end {
            self.offset = cmp::min(self.offset, self.range.end);
            self.chunks.seek(&self.offset, Bias::Right);
        }

        found
    }

    /// Move this cursor to the preceding position in the rope that starts a new line.
    /// Reversed chunks iterators are not currently supported and will panic.
    ///
    /// If this cursor is not on the start of a line, it will be moved to the start of
    /// its current line. Otherwise it will be moved to the start of the previous line.
    /// It updates the cursor's position and returns true if a previous line was found,
    /// or false if the cursor was already at the start of the rope.
    pub fn prev_line(&mut self) -> bool {
        assert!(!self.reversed);

        let initial_offset = self.offset;

        if self.offset == *self.chunks.start() {
            self.chunks.prev();
        }

        if let Some(chunk) = self.chunks.item() {
            let mut end_ix = self.offset - *self.chunks.start();
            if chunk.text.as_bytes()[end_ix - 1] == b'\n' {
                end_ix -= 1;
            }

            if let Some(newline_ix) = chunk.text[..end_ix].rfind('\n') {
                self.offset = *self.chunks.start() + newline_ix + 1;
                if self.offset_is_valid() {
                    return true;
                }
            }
        }

        self.chunks
            .search_backward(|summary| summary.text.lines.row > 0);
        self.offset = *self.chunks.start();
        if let Some(chunk) = self.chunks.item()
            && let Some(newline_ix) = chunk.text.rfind('\n')
        {
            self.offset += newline_ix + 1;
            if self.offset_is_valid() {
                if self.offset == self.chunks.end() {
                    self.chunks.next();
                }

                return true;
            }
        }

        if !self.offset_is_valid() || self.chunks.item().is_none() {
            self.offset = self.range.start;
            self.chunks.seek(&self.offset, Bias::Right);
        }

        self.offset < initial_offset && self.offset == 0
    }

    pub fn peek(&self) -> Option<&'a str> {
        if !self.offset_is_valid() {
            return None;
        }

        let chunk = self.chunks.item()?;
        let chunk_start = *self.chunks.start();
        let slice_range = if self.reversed {
            let slice_start = cmp::max(chunk_start, self.range.start) - chunk_start;
            let slice_end = self.offset - chunk_start;
            slice_start..slice_end
        } else {
            let slice_start = self.offset - chunk_start;
            let slice_end = cmp::min(self.chunks.end(), self.range.end) - chunk_start;
            slice_start..slice_end
        };

        Some(&chunk.text[slice_range])
    }

    /// Returns bitmaps that represent character positions and tab positions
    pub fn peek_with_bitmaps(&self) -> Option<ChunkBitmaps<'a>> {
        if !self.offset_is_valid() {
            return None;
        }

        let chunk = self.chunks.item()?;
        let chunk_start = *self.chunks.start();
        let slice_range = if self.reversed {
            let slice_start = cmp::max(chunk_start, self.range.start) - chunk_start;
            let slice_end = self.offset - chunk_start;
            slice_start..slice_end
        } else {
            let slice_start = self.offset - chunk_start;
            let slice_end = cmp::min(self.chunks.end(), self.range.end) - chunk_start;
            slice_start..slice_end
        };
        let chunk_start_offset = slice_range.start;
        let slice_text = &chunk.text[slice_range];

        // Shift the tabs to align with our slice window
        let shifted_tabs = chunk.tabs() >> chunk_start_offset;
        let shifted_chars = chunk.chars() >> chunk_start_offset;
        let shifted_newlines = chunk.newlines() >> chunk_start_offset;

        Some(ChunkBitmaps {
            text: slice_text,
            chars: shifted_chars,
            tabs: shifted_tabs,
            newlines: shifted_newlines,
        })
    }

    pub fn lines(self) -> Lines<'a> {
        let reversed = self.reversed;
        Lines {
            chunks: self,
            current_line: String::new(),
            done: false,
            reversed,
        }
    }

    pub fn equals_str(&self, other: &str) -> bool {
        let chunk = self.clone();
        if chunk.reversed {
            let mut offset = other.len();
            for chunk in chunk {
                if other[0..offset].ends_with(chunk) {
                    offset -= chunk.len();
                } else {
                    return false;
                }
            }
            if offset != 0 {
                return false;
            }
        } else {
            let mut offset = 0;
            for chunk in chunk {
                if offset >= other.len() {
                    return false;
                }
                if other[offset..].starts_with(chunk) {
                    offset += chunk.len();
                } else {
                    return false;
                }
            }
            if offset != other.len() {
                return false;
            }
        }

        true
    }
}

pub struct ChunkWithBitmaps<'a>(pub Chunks<'a>);

impl<'a> Iterator for ChunkWithBitmaps<'a> {
    /// text, chars bitmap, tabs bitmap
    type Item = ChunkBitmaps<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let chunk_bitmaps = self.0.peek_with_bitmaps()?;
        if self.0.reversed {
            self.0.offset -= chunk_bitmaps.text.len();
            if self.0.offset <= *self.0.chunks.start() {
                self.0.chunks.prev();
            }
        } else {
            self.0.offset += chunk_bitmaps.text.len();
            if self.0.offset >= self.0.chunks.end() {
                self.0.chunks.next();
            }
        }

        Some(chunk_bitmaps)
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.peek()?;
        if self.reversed {
            self.offset -= chunk.len();
            if self.offset <= *self.chunks.start() {
                self.chunks.prev();
            }
        } else {
            self.offset += chunk.len();
            if self.offset >= self.chunks.end() {
                self.chunks.next();
            }
        }

        Some(chunk)
    }
}

pub struct Bytes<'a> {
    chunks: sum_tree::Cursor<'a, 'static, Chunk, usize>,
    range: Range<usize>,
    reversed: bool,
}

impl<'a> Bytes<'a> {
    pub fn new(rope: &'a Rope, range: Range<usize>, reversed: bool) -> Self {
        let mut chunks = rope.chunks.cursor(());
        if reversed {
            chunks.seek(&range.end, Bias::Left);
        } else {
            chunks.seek(&range.start, Bias::Right);
        }
        Self {
            chunks,
            range,
            reversed,
        }
    }

    pub fn peek(&self) -> Option<&'a [u8]> {
        let chunk = self.chunks.item()?;
        if self.reversed && self.range.start >= self.chunks.end() {
            return None;
        }
        let chunk_start = *self.chunks.start();
        if self.range.end <= chunk_start {
            return None;
        }
        let start = self.range.start.saturating_sub(chunk_start);
        let end = self.range.end - chunk_start;
        Some(&chunk.text.as_bytes()[start..chunk.text.len().min(end)])
    }
}

impl<'a> Iterator for Bytes<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.peek();
        if result.is_some() {
            if self.reversed {
                self.chunks.prev();
            } else {
                self.chunks.next();
            }
        }
        result
    }
}

impl io::Read for Bytes<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(chunk) = self.peek() {
            let len = cmp::min(buf.len(), chunk.len());
            if self.reversed {
                buf[..len].copy_from_slice(&chunk[chunk.len() - len..]);
                buf[..len].reverse();
                self.range.end -= len;
            } else {
                buf[..len].copy_from_slice(&chunk[..len]);
                self.range.start += len;
            }

            if len == chunk.len() {
                if self.reversed {
                    self.chunks.prev();
                } else {
                    self.chunks.next();
                }
            }
            Ok(len)
        } else {
            Ok(0)
        }
    }
}
pub struct Lines<'a> {
    chunks: Chunks<'a>,
    current_line: String,
    done: bool,
    reversed: bool,
}

impl<'a> Lines<'a> {
    pub fn next(&mut self) -> Option<&str> {
        if self.done {
            return None;
        }

        self.current_line.clear();

        while let Some(chunk) = self.chunks.peek() {
            let chunk_lines = chunk.split('\n');
            if self.reversed {
                let mut chunk_lines = chunk_lines.rev().peekable();
                if let Some(chunk_line) = chunk_lines.next() {
                    let done = chunk_lines.peek().is_some();
                    if done {
                        self.chunks
                            .seek(self.chunks.offset() - chunk_line.len() - "\n".len());
                        if self.current_line.is_empty() {
                            return Some(chunk_line);
                        }
                    }
                    self.current_line.insert_str(0, chunk_line);
                    if done {
                        return Some(&self.current_line);
                    }
                }
            } else {
                let mut chunk_lines = chunk_lines.peekable();
                if let Some(chunk_line) = chunk_lines.next() {
                    let done = chunk_lines.peek().is_some();
                    if done {
                        self.chunks
                            .seek(self.chunks.offset() + chunk_line.len() + "\n".len());
                        if self.current_line.is_empty() {
                            return Some(chunk_line);
                        }
                    }
                    self.current_line.push_str(chunk_line);
                    if done {
                        return Some(&self.current_line);
                    }
                }
            }

            self.chunks.next();
        }

        self.done = true;
        Some(&self.current_line)
    }

    pub fn seek(&mut self, offset: usize) {
        self.chunks.seek(offset);
        self.current_line.clear();
        self.done = false;
    }

    pub fn offset(&self) -> usize {
        self.chunks.offset()
    }
}

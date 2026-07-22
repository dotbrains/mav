use super::*;

impl Rope {
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks that `index`-th byte is the first byte in a UTF-8 code point
    /// sequence or the end of the string.
    ///
    /// The start and end of the string (when `index == self.len()`) are
    /// considered to be boundaries.
    ///
    /// Returns `false` if `index` is greater than `self.len()`.
    pub fn is_char_boundary(&self, offset: usize) -> bool {
        if self.chunks.is_empty() {
            return offset == 0;
        }
        let (start, _, item) = self.chunks.find::<usize, _>((), &offset, Bias::Left);
        let chunk_offset = offset - start;
        item.map(|chunk| chunk.is_char_boundary(chunk_offset))
            .unwrap_or(false)
    }

    #[track_caller]
    #[inline(always)]
    pub fn assert_char_boundary<const PANIC: bool>(&self, offset: usize) -> bool {
        if self.chunks.is_empty() && offset == 0 {
            return true;
        }
        let (start, _, item) = self.chunks.find::<usize, _>((), &offset, Bias::Left);
        match item {
            Some(chunk) => {
                let chunk_offset = offset - start;
                chunk.assert_char_boundary::<PANIC>(chunk_offset)
            }
            None if PANIC => {
                panic!(
                    "byte index {} is out of bounds of rope (length: {})",
                    offset,
                    self.len()
                );
            }
            None => {
                log::error!(
                    "byte index {} is out of bounds of rope (length: {})",
                    offset,
                    self.len()
                );
                false
            }
        }
    }

    pub fn floor_char_boundary(&self, index: usize) -> usize {
        if index >= self.len() {
            self.len()
        } else {
            let (start, _, item) = self.chunks.find::<usize, _>((), &index, Bias::Left);
            let chunk_offset = index - start;
            let lower_idx = item.map(|chunk| chunk.text.floor_char_boundary(chunk_offset));
            lower_idx.map_or_else(|| self.len(), |idx| start + idx)
        }
    }

    pub fn ceil_char_boundary(&self, index: usize) -> usize {
        if index > self.len() {
            self.len()
        } else {
            let (start, _, item) = self.chunks.find::<usize, _>((), &index, Bias::Left);
            let chunk_offset = index - start;
            let upper_idx = item.map(|chunk| chunk.text.ceil_char_boundary(chunk_offset));
            upper_idx.map_or_else(|| self.len(), |idx| start + idx)
        }
    }

    pub fn append(&mut self, rope: Rope) {
        if let Some(chunk) = rope.chunks.first()
            && (self
                .chunks
                .last()
                .is_some_and(|c| c.text.len() < chunk::MIN_BASE)
                || chunk.text.len() < chunk::MIN_BASE)
        {
            self.push_chunk(chunk.as_slice());

            let mut chunks = rope.chunks.cursor::<()>(());
            chunks.next();
            chunks.next();
            self.chunks.append(chunks.suffix(), ());
        } else {
            self.chunks.append(rope.chunks, ());
        }
        self.check_invariants();
    }

    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        let mut new_rope = Rope::new();
        let mut cursor = self.cursor(0);
        new_rope.append(cursor.slice(range.start));
        cursor.seek_forward(range.end);
        new_rope.push(text);
        new_rope.append(cursor.suffix());
        *self = new_rope;
    }

    pub fn slice(&self, range: Range<usize>) -> Rope {
        let mut cursor = self.cursor(0);
        cursor.seek_forward(range.start);
        cursor.slice(range.end)
    }

    pub fn slice_rows(&self, range: Range<u32>) -> Rope {
        // This would be more efficient with a forward advance after the first, but it's fine.
        let start = self.point_to_offset(Point::new(range.start, 0));
        let end = self.point_to_offset(Point::new(range.end, 0));
        self.slice(start..end)
    }

    pub fn push(&mut self, mut text: &str) {
        self.chunks.update_last(
            |last_chunk| {
                let split_ix = if last_chunk.text.len() + text.len() <= chunk::MAX_BASE {
                    text.len()
                } else {
                    let mut split_ix = cmp::min(
                        chunk::MIN_BASE.saturating_sub(last_chunk.text.len()),
                        text.len(),
                    );
                    while !text.is_char_boundary(split_ix) {
                        split_ix += 1;
                    }
                    split_ix
                };

                let (suffix, remainder) = text.split_at(split_ix);
                last_chunk.push_str(suffix);
                text = remainder;
            },
            (),
        );

        if text.is_empty() {
            self.check_invariants();
            return;
        }

        #[cfg(all(test, not(rust_analyzer)))]
        const NUM_CHUNKS: usize = 16;
        #[cfg(not(all(test, not(rust_analyzer))))]
        const NUM_CHUNKS: usize = 4;

        // We accommodate for NUM_CHUNKS chunks of size MAX_BASE
        // but given the chunk boundary can land within a character
        // we need to accommodate for the worst case where every chunk gets cut short by up to 4 bytes
        if text.len() > NUM_CHUNKS * chunk::MAX_BASE - NUM_CHUNKS * 4 {
            return self.push_large(text);
        }
        // 16 is enough as otherwise we will hit the branch above
        let mut new_chunks = ArrayVec::<_, NUM_CHUNKS, u8>::new();

        while !text.is_empty() {
            let mut split_ix = cmp::min(chunk::MAX_BASE, text.len());
            while !text.is_char_boundary(split_ix) {
                split_ix -= 1;
            }
            let (chunk, remainder) = text.split_at(split_ix);
            new_chunks.push(chunk).unwrap();
            text = remainder;
        }
        self.chunks
            .extend(new_chunks.into_iter().map(Chunk::new), ());

        self.check_invariants();
    }

    /// A copy of `push` specialized for working with large quantities of text.
    fn push_large(&mut self, mut text: &str) {
        // To avoid frequent reallocs when loading large swaths of file contents,
        // we estimate worst-case `new_chunks` capacity;
        // Chunk is a fixed-capacity buffer. If a character falls on
        // chunk boundary, we push it off to the following chunk (thus leaving a small bit of capacity unfilled in current chunk).
        // Worst-case chunk count when loading a file is then a case where every chunk ends up with that unused capacity.
        // Since we're working with UTF-8, each character is at most 4 bytes wide. It follows then that the worst case is where
        // a chunk ends with 3 bytes of a 4-byte character. These 3 bytes end up being stored in the following chunk, thus wasting
        // 3 bytes of storage in current chunk.
        // For example, a 1024-byte string can occupy between 32 (full ASCII, 1024/32) and 36 (full 4-byte UTF-8, 1024 / 29 rounded up) chunks.
        const MIN_CHUNK_SIZE: usize = chunk::MAX_BASE - 3;

        // We also round up the capacity up by one, for a good measure; we *really* don't want to realloc here, as we assume that the # of characters
        // we're working with there is large.
        let capacity = text.len().div_ceil(MIN_CHUNK_SIZE);
        let mut new_chunks = Vec::with_capacity(capacity);

        while !text.is_empty() {
            let mut split_ix = cmp::min(chunk::MAX_BASE, text.len());
            while !text.is_char_boundary(split_ix) {
                split_ix -= 1;
            }
            let (chunk, remainder) = text.split_at(split_ix);
            new_chunks.push(chunk);
            text = remainder;
        }

        #[cfg(all(test, not(rust_analyzer)))]
        const PARALLEL_THRESHOLD: usize = 4;
        #[cfg(not(all(test, not(rust_analyzer))))]
        const PARALLEL_THRESHOLD: usize = 84 * (2 * sum_tree::TREE_BASE);

        if new_chunks.len() >= PARALLEL_THRESHOLD {
            self.chunks
                .par_extend(new_chunks.into_par_iter().map(Chunk::new), ());
        } else {
            self.chunks
                .extend(new_chunks.into_iter().map(Chunk::new), ());
        }

        self.check_invariants();
    }

    pub(crate) fn push_chunk(&mut self, mut chunk: ChunkSlice) {
        self.chunks.update_last(
            |last_chunk| {
                let split_ix = if last_chunk.text.len() + chunk.len() <= chunk::MAX_BASE {
                    chunk.len()
                } else {
                    let mut split_ix = cmp::min(
                        chunk::MIN_BASE.saturating_sub(last_chunk.text.len()),
                        chunk.len(),
                    );
                    while !chunk.is_char_boundary(split_ix) {
                        split_ix += 1;
                    }
                    split_ix
                };

                let (suffix, remainder) = chunk.split_at(split_ix);
                last_chunk.append(suffix);
                chunk = remainder;
            },
            (),
        );

        if !chunk.is_empty() {
            self.chunks.push(chunk.into(), ());
        }
    }

    pub fn push_front(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.is_empty() {
            self.push(text);
            return;
        }
        if self
            .chunks
            .first()
            .is_some_and(|c| c.text.len() + text.len() <= chunk::MAX_BASE)
        {
            self.chunks
                .update_first(|first_chunk| first_chunk.prepend_str(text), ());
            self.check_invariants();
            return;
        }
        let suffix = mem::replace(self, Rope::from(text));
        self.append(suffix);
    }

    fn check_invariants(&self) {
        #[cfg(test)]
        {
            // Ensure all chunks except maybe the last one are not underflowing.
            // Allow some wiggle room for multibyte characters at chunk boundaries.
            let mut chunks = self.chunks.cursor::<()>(()).peekable();
            while let Some(chunk) = chunks.next() {
                if chunks.peek().is_some() {
                    assert!(chunk.text.len() + 3 >= chunk::MIN_BASE);
                }
            }
        }
    }

    pub fn summary(&self) -> TextSummary {
        self.chunks.summary().text
    }

    pub fn len(&self) -> usize {
        self.chunks.extent(())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn max_point(&self) -> Point {
        self.chunks.extent(())
    }

    pub fn max_point_utf16(&self) -> PointUtf16 {
        self.chunks.extent(())
    }

    pub fn cursor(&self, offset: usize) -> Cursor<'_> {
        Cursor::new(self, offset)
    }

    pub fn chars(&self) -> impl Iterator<Item = char> + '_ {
        self.chars_at(0)
    }

    pub fn chars_at(&self, start: usize) -> impl Iterator<Item = char> + '_ {
        self.chunks_in_range(start..self.len()).flat_map(str::chars)
    }

    pub fn reversed_chars_at(&self, start: usize) -> impl Iterator<Item = char> + '_ {
        self.reversed_chunks_in_range(0..start)
            .flat_map(|chunk| chunk.chars().rev())
    }

    pub fn bytes_in_range(&self, range: Range<usize>) -> Bytes<'_> {
        Bytes::new(self, range, false)
    }

    pub fn reversed_bytes_in_range(&self, range: Range<usize>) -> Bytes<'_> {
        Bytes::new(self, range, true)
    }

    pub fn chunks(&self) -> Chunks<'_> {
        self.chunks_in_range(0..self.len())
    }

    pub fn chunks_in_range(&self, range: Range<usize>) -> Chunks<'_> {
        Chunks::new(self, range, false)
    }

    pub fn reversed_chunks_in_range(&self, range: Range<usize>) -> Chunks<'_> {
        Chunks::new(self, range, true)
    }
}

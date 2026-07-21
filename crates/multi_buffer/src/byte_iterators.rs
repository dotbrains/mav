use super::*;

pub struct MultiBufferBytes<'a> {
    pub(super) range: Range<MultiBufferOffset>,
    pub(super) cursor: MultiBufferCursor<'a, MultiBufferOffset, BufferOffset>,
    pub(super) excerpt_bytes: Option<text::Bytes<'a>>,
    pub(super) has_trailing_newline: bool,
    pub(super) chunk: &'a [u8],
}

pub struct ReversedMultiBufferBytes<'a> {
    pub(super) range: Range<MultiBufferOffset>,
    pub(super) chunks: ReversedMultiBufferChunks<'a>,
    pub(super) chunk: &'a [u8],
}

impl MultiBufferBytes<'_> {
    fn consume(&mut self, len: usize) {
        self.range.start += len;
        self.chunk = &self.chunk[len..];

        if !self.range.is_empty() && self.chunk.is_empty() {
            if let Some(chunk) = self.excerpt_bytes.as_mut().and_then(|bytes| bytes.next()) {
                self.chunk = chunk;
            } else if self.has_trailing_newline {
                self.has_trailing_newline = false;
                self.chunk = b"\n";
            } else {
                self.cursor.next();
                if let Some(region) = self.cursor.region() {
                    let mut excerpt_bytes = region.buffer.bytes_in_range(
                        region.buffer_range.start
                            ..(region.buffer_range.start + (self.range.end - region.range.start))
                                .min(region.buffer_range.end),
                    );
                    self.chunk = excerpt_bytes.next().unwrap_or(&[]);
                    self.excerpt_bytes = Some(excerpt_bytes);
                    self.has_trailing_newline =
                        region.has_trailing_newline && self.range.end >= region.range.end;
                    if self.chunk.is_empty() && self.has_trailing_newline {
                        self.has_trailing_newline = false;
                        self.chunk = b"\n";
                    }
                }
            }
        }
    }
}

impl<'a> Iterator for MultiBufferBytes<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.chunk;
        if chunk.is_empty() {
            None
        } else {
            self.consume(chunk.len());
            Some(chunk)
        }
    }
}

impl io::Read for MultiBufferBytes<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = cmp::min(buf.len(), self.chunk.len());
        buf[..len].copy_from_slice(&self.chunk[..len]);
        if len > 0 {
            self.consume(len);
        }
        Ok(len)
    }
}

impl io::Read for ReversedMultiBufferBytes<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = cmp::min(buf.len(), self.chunk.len());
        buf[..len].copy_from_slice(&self.chunk[..len]);
        buf[..len].reverse();
        if len > 0 {
            self.range.end -= len;
            self.chunk = &self.chunk[..self.chunk.len() - len];
            if !self.range.is_empty()
                && self.chunk.is_empty()
                && let Some(chunk) = self.chunks.next()
            {
                self.chunk = chunk.as_bytes();
            }
        }
        Ok(len)
    }
}

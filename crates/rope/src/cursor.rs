use super::*;

pub struct Cursor<'a> {
    rope: &'a Rope,
    chunks: sum_tree::Cursor<'a, 'static, Chunk, usize>,
    offset: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(rope: &'a Rope, offset: usize) -> Self {
        let mut chunks = rope.chunks.cursor(());
        chunks.seek(&offset, Bias::Right);
        Self {
            rope,
            chunks,
            offset,
        }
    }

    pub fn seek_forward(&mut self, end_offset: usize) {
        assert!(
            end_offset >= self.offset,
            "cannot seek backward from {} to {}",
            self.offset,
            end_offset
        );
        assert!(
            end_offset <= self.rope.len(),
            "cannot summarize past end of rope"
        );

        self.chunks.seek_forward(&end_offset, Bias::Right);
        self.offset = end_offset;
    }

    pub fn slice(&mut self, end_offset: usize) -> Rope {
        assert!(
            end_offset >= self.offset,
            "cannot slice backward from {} to {}",
            self.offset,
            end_offset
        );
        assert!(
            end_offset <= self.rope.len(),
            "cannot summarize past end of rope"
        );

        let mut slice = Rope::new();
        if let Some(start_chunk) = self.chunks.item() {
            let start_ix = self.offset - self.chunks.start();
            let end_ix = cmp::min(end_offset, self.chunks.end()) - self.chunks.start();
            slice.push_chunk(start_chunk.slice(start_ix..end_ix));
        }

        if end_offset > self.chunks.end() {
            self.chunks.next();
            slice.append(Rope {
                chunks: self.chunks.slice(&end_offset, Bias::Right),
            });
            if let Some(end_chunk) = self.chunks.item() {
                let end_ix = end_offset - self.chunks.start();
                slice.push_chunk(end_chunk.slice(0..end_ix));
            }
        }

        self.offset = end_offset;
        slice
    }

    pub fn summary<D: TextDimension>(&mut self, end_offset: usize) -> D {
        assert!(
            end_offset >= self.offset,
            "cannot summarize backward from {} to {}",
            self.offset,
            end_offset
        );
        assert!(
            end_offset <= self.rope.len(),
            "cannot summarize past end of rope"
        );

        let mut summary = D::zero(());
        if let Some(start_chunk) = self.chunks.item() {
            let start_ix = self.offset - self.chunks.start();
            let end_ix = cmp::min(end_offset, self.chunks.end()) - self.chunks.start();
            summary.add_assign(&D::from_chunk(start_chunk.slice(start_ix..end_ix)));
        }

        if end_offset > self.chunks.end() {
            self.chunks.next();
            summary.add_assign(&self.chunks.summary(&end_offset, Bias::Right));
            if let Some(end_chunk) = self.chunks.item() {
                let end_ix = end_offset - self.chunks.start();
                summary.add_assign(&D::from_chunk(end_chunk.slice(0..end_ix)));
            }
        }

        self.offset = end_offset;
        summary
    }

    pub fn suffix(mut self) -> Rope {
        self.slice(self.rope.chunks.extent(()))
    }

    pub fn offset(&self) -> usize {
        self.offset
    }
}

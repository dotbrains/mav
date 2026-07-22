use super::*;

/// A chunk of a buffer's text, along with its syntax highlight and
/// diagnostic status.
#[derive(Clone, Debug, Default)]
pub struct Chunk<'a> {
    /// The text of the chunk.
    pub text: &'a str,
    /// The syntax highlighting style of the chunk.
    pub syntax_highlight_id: Option<HighlightId>,
    /// The highlight style that has been applied to this chunk in
    /// the editor.
    pub highlight_style: Option<HighlightStyle>,
    /// The severity of diagnostic associated with this chunk, if any.
    pub diagnostic_severity: Option<lsp::DiagnosticSeverity>,
    /// Whether this chunk of text is marked as unnecessary.
    pub is_unnecessary: bool,
    /// Whether this chunk of text should be underlined.
    pub underline: bool,
    /// Whether this chunk of text was originally a tab character.
    pub is_tab: bool,
    /// Whether this chunk of text was originally a tab character.
    pub is_inlay: bool,
    /// An optional recipe for how the chunk should be presented.
    pub renderer: Option<ChunkRenderer>,
    /// Bitmap of tab character locations in chunk
    pub tabs: u128,
    /// Bitmap of character locations in chunk
    pub chars: u128,
    /// Bitmap of newline locations in chunk
    pub newlines: u128,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChunkRendererId {
    Fold(FoldId),
    Inlay(InlayId),
}

/// A recipe for how the chunk should be presented.
#[derive(Clone)]
pub struct ChunkRenderer {
    /// The id of the renderer associated with this chunk.
    pub id: ChunkRendererId,
    /// Creates a custom element to represent this chunk.
    pub render: Arc<dyn Send + Sync + Fn(&mut ChunkRendererContext) -> AnyElement>,
    /// If true, the element is constrained to the shaped width of the text.
    pub constrain_width: bool,
    /// The width of the element, as measured during the last layout pass.
    ///
    /// This is None if the element has not been laid out yet.
    pub measured_width: Option<Pixels>,
}

pub struct ChunkRendererContext<'a, 'b> {
    pub window: &'a mut Window,
    pub context: &'b mut App,
    pub max_width: Pixels,
}

impl fmt::Debug for ChunkRenderer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ChunkRenderer")
            .field("constrain_width", &self.constrain_width)
            .finish()
    }
}

impl Deref for ChunkRendererContext<'_, '_> {
    type Target = App;

    fn deref(&self) -> &Self::Target {
        self.context
    }
}

impl DerefMut for ChunkRendererContext<'_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.context
    }
}

pub struct FoldChunks<'a> {
    transform_cursor: Cursor<'a, 'static, Transform, Dimensions<FoldOffset, InlayOffset>>,
    inlay_chunks: InlayChunks<'a>,
    inlay_chunk: Option<(InlayOffset, InlayChunk<'a>)>,
    inlay_offset: InlayOffset,
    output_offset: FoldOffset,
    max_output_offset: FoldOffset,
}

impl FoldChunks<'_> {
    #[ztracing::instrument(skip_all)]
    pub(crate) fn seek(&mut self, range: Range<FoldOffset>) {
        self.transform_cursor.seek(&range.start, Bias::Right);

        let inlay_start = {
            let overshoot = range.start - self.transform_cursor.start().0;
            self.transform_cursor.start().1 + overshoot
        };

        let transform_end = self.transform_cursor.end();

        let inlay_end = if self
            .transform_cursor
            .item()
            .is_none_or(|transform| transform.is_fold())
        {
            inlay_start
        } else if range.end < transform_end.0 {
            let overshoot = range.end - self.transform_cursor.start().0;
            self.transform_cursor.start().1 + overshoot
        } else {
            transform_end.1
        };

        self.inlay_chunks.seek(inlay_start..inlay_end);
        self.inlay_chunk = None;
        self.inlay_offset = inlay_start;
        self.output_offset = range.start;
        self.max_output_offset = range.end;
    }
}

impl<'a> Iterator for FoldChunks<'a> {
    type Item = Chunk<'a>;

    #[ztracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.output_offset >= self.max_output_offset {
            return None;
        }

        let transform = self.transform_cursor.item()?;

        // If we're in a fold, then return the fold's display text and
        // advance the transform and buffer cursors to the end of the fold.
        if let Some(placeholder) = transform.placeholder.as_ref() {
            self.inlay_chunk.take();
            self.inlay_offset += InlayOffset(transform.summary.input.len);

            while self.inlay_offset >= self.transform_cursor.end().1
                && self.transform_cursor.item().is_some()
            {
                self.transform_cursor.next();
            }

            self.output_offset.0 += placeholder.text.len();
            return Some(Chunk {
                text: &placeholder.text,
                chars: placeholder.chars,
                renderer: Some(placeholder.renderer.clone()),
                ..Default::default()
            });
        }

        // When we reach a non-fold region, seek the underlying text
        // chunk iterator to the next unfolded range.
        if self.inlay_offset == self.transform_cursor.start().1
            && self.inlay_chunks.offset() != self.inlay_offset
        {
            let transform_start = self.transform_cursor.start();
            let transform_end = self.transform_cursor.end();
            let inlay_end = if self.max_output_offset < transform_end.0 {
                let overshoot = self.max_output_offset - transform_start.0;
                transform_start.1 + overshoot
            } else {
                transform_end.1
            };

            self.inlay_chunks.seek(self.inlay_offset..inlay_end);
        }

        // Retrieve a chunk from the current location in the buffer.
        if self.inlay_chunk.is_none() {
            let chunk_offset = self.inlay_chunks.offset();
            self.inlay_chunk = self.inlay_chunks.next().map(|chunk| (chunk_offset, chunk));
        }

        // Otherwise, take a chunk from the buffer's text.
        if let Some((buffer_chunk_start, mut inlay_chunk)) = self.inlay_chunk.clone() {
            let chunk = &mut inlay_chunk.chunk;
            let buffer_chunk_end = buffer_chunk_start + chunk.text.len();
            let transform_end = self.transform_cursor.end().1;
            let chunk_end = buffer_chunk_end.min(transform_end);

            let bit_start = self.inlay_offset - buffer_chunk_start;
            let bit_end = chunk_end - buffer_chunk_start;
            chunk.text = &chunk.text[bit_start..bit_end];

            let bit_end = chunk_end - buffer_chunk_start;
            let mask = 1u128.unbounded_shl(bit_end as u32).wrapping_sub(1);

            chunk.tabs = (chunk.tabs >> bit_start) & mask;
            chunk.chars = (chunk.chars >> bit_start) & mask;
            chunk.newlines = (chunk.newlines >> bit_start) & mask;

            if chunk_end == transform_end {
                self.transform_cursor.next();
            } else if chunk_end == buffer_chunk_end {
                self.inlay_chunk.take();
            }

            self.inlay_offset = chunk_end;
            self.output_offset.0 += chunk.text.len();
            return Some(Chunk {
                text: chunk.text,
                tabs: chunk.tabs,
                chars: chunk.chars,
                newlines: chunk.newlines,
                syntax_highlight_id: chunk.syntax_highlight_id,
                highlight_style: chunk.highlight_style,
                diagnostic_severity: chunk.diagnostic_severity,
                is_unnecessary: chunk.is_unnecessary,
                is_tab: chunk.is_tab,
                is_inlay: chunk.is_inlay,
                underline: chunk.underline,
                renderer: inlay_chunk.renderer,
            });
        }

        None
    }
}

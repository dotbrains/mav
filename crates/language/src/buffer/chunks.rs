use super::*;

struct BufferChunkHighlights<'a> {
    captures: SyntaxMapCaptures<'a>,
    next_capture: Option<SyntaxMapCapture<'a>>,
    stack: Vec<(usize, HighlightId)>,
    highlight_maps: Vec<HighlightMap>,
}

/// An iterator that yields chunks of a buffer's text, along with their
/// syntax highlights and diagnostic status.
pub struct BufferChunks<'a> {
    buffer_snapshot: Option<&'a BufferSnapshot>,
    range: Range<usize>,
    chunks: text::Chunks<'a>,
    diagnostic_endpoints: Option<Peekable<vec::IntoIter<DiagnosticEndpoint>>>,
    error_depth: usize,
    warning_depth: usize,
    information_depth: usize,
    hint_depth: usize,
    unnecessary_depth: usize,
    underline: bool,
    highlights: Option<BufferChunkHighlights<'a>>,
}

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
    pub diagnostic_severity: Option<DiagnosticSeverity>,
    /// A bitset of which characters are tabs in this string.
    pub tabs: u128,
    /// Bitmap of character indices in this chunk
    pub chars: u128,
    /// Bitmap of newline indices in this chunk
    pub newlines: u128,
    /// Whether this chunk of text is marked as unnecessary.
    pub is_unnecessary: bool,
    /// Whether this chunk of text was originally a tab character.
    pub is_tab: bool,
    /// Whether this chunk of text was originally an inlay.
    pub is_inlay: bool,
    /// Whether to underline the corresponding text range in the editor.
    pub underline: bool,
}

/// A set of edits to a given version of a buffer, computed asynchronously.
#[derive(Debug, Clone)]
pub struct Diff {
    pub base_version: clock::Global,
    pub line_ending: LineEnding,
    pub edits: Vec<(Range<usize>, Arc<str>)>,
}

unsafe impl Send for BufferChunks<'_> {}

impl<'a> BufferChunks<'a> {
    pub(crate) fn new(
        text: &'a Rope,
        range: Range<usize>,
        syntax: Option<(SyntaxMapCaptures<'a>, Vec<HighlightMap>)>,
        diagnostics: bool,
        buffer_snapshot: Option<&'a BufferSnapshot>,
    ) -> Self {
        let mut highlights = None;
        if let Some((captures, highlight_maps)) = syntax {
            highlights = Some(BufferChunkHighlights {
                captures,
                next_capture: None,
                stack: Default::default(),
                highlight_maps,
            })
        }

        let diagnostic_endpoints = diagnostics.then(|| Vec::new().into_iter().peekable());
        let chunks = text.chunks_in_range(range.clone());

        let mut this = BufferChunks {
            range,
            buffer_snapshot,
            chunks,
            diagnostic_endpoints,
            error_depth: 0,
            warning_depth: 0,
            information_depth: 0,
            hint_depth: 0,
            unnecessary_depth: 0,
            underline: true,
            highlights,
        };
        this.initialize_diagnostic_endpoints();
        this
    }

    /// Seeks to the given byte offset in the buffer.
    pub fn seek(&mut self, range: Range<usize>) {
        let old_range = std::mem::replace(&mut self.range, range.clone());
        self.chunks.set_range(self.range.clone());
        if let Some(highlights) = self.highlights.as_mut() {
            if old_range.start <= self.range.start && old_range.end >= self.range.end {
                // Reuse existing highlights stack, as the new range is a subrange of the old one.
                highlights
                    .stack
                    .retain(|(end_offset, _)| *end_offset > range.start);
                if let Some(capture) = &highlights.next_capture
                    && range.start >= capture.node.start_byte()
                {
                    let next_capture_end = capture.node.end_byte();
                    if range.start < next_capture_end
                        && let Some(capture_id) =
                            highlights.highlight_maps[capture.grammar_index].get(capture.index)
                    {
                        highlights.stack.push((next_capture_end, capture_id));
                    }
                    highlights.next_capture.take();
                }
            } else if let Some(snapshot) = self.buffer_snapshot {
                let (captures, highlight_maps) = snapshot.get_highlights(self.range.clone());
                *highlights = BufferChunkHighlights {
                    captures,
                    next_capture: None,
                    stack: Default::default(),
                    highlight_maps,
                };
            } else {
                // We cannot obtain new highlights for a language-aware buffer iterator, as we don't have a buffer snapshot.
                // Seeking such BufferChunks is not supported.
                debug_assert!(
                    false,
                    "Attempted to seek on a language-aware buffer iterator without associated buffer snapshot"
                );
            }

            highlights.captures.set_byte_range(self.range.clone());
            self.initialize_diagnostic_endpoints();
        }
    }

    fn initialize_diagnostic_endpoints(&mut self) {
        if let Some(diagnostics) = self.diagnostic_endpoints.as_mut()
            && let Some(buffer) = self.buffer_snapshot
        {
            let mut diagnostic_endpoints = Vec::new();
            for entry in buffer.diagnostics_in_range::<_, usize>(self.range.clone(), false) {
                diagnostic_endpoints.push(DiagnosticEndpoint {
                    offset: entry.range.start,
                    is_start: true,
                    severity: entry.diagnostic.severity,
                    is_unnecessary: entry.diagnostic.is_unnecessary,
                    underline: entry.diagnostic.underline,
                });
                diagnostic_endpoints.push(DiagnosticEndpoint {
                    offset: entry.range.end,
                    is_start: false,
                    severity: entry.diagnostic.severity,
                    is_unnecessary: entry.diagnostic.is_unnecessary,
                    underline: entry.diagnostic.underline,
                });
            }
            diagnostic_endpoints
                .sort_unstable_by_key(|endpoint| (endpoint.offset, !endpoint.is_start));
            *diagnostics = diagnostic_endpoints.into_iter().peekable();
            self.hint_depth = 0;
            self.error_depth = 0;
            self.warning_depth = 0;
            self.information_depth = 0;
        }
    }

    /// The current byte offset in the buffer.
    pub fn offset(&self) -> usize {
        self.range.start
    }

    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    fn update_diagnostic_depths(&mut self, endpoint: DiagnosticEndpoint) {
        let depth = match endpoint.severity {
            DiagnosticSeverity::ERROR => &mut self.error_depth,
            DiagnosticSeverity::WARNING => &mut self.warning_depth,
            DiagnosticSeverity::INFORMATION => &mut self.information_depth,
            DiagnosticSeverity::HINT => &mut self.hint_depth,
            _ => return,
        };
        if endpoint.is_start {
            *depth += 1;
        } else {
            *depth -= 1;
        }

        if endpoint.is_unnecessary {
            if endpoint.is_start {
                self.unnecessary_depth += 1;
            } else {
                self.unnecessary_depth -= 1;
            }
        }
    }

    fn current_diagnostic_severity(&self) -> Option<DiagnosticSeverity> {
        if self.error_depth > 0 {
            Some(DiagnosticSeverity::ERROR)
        } else if self.warning_depth > 0 {
            Some(DiagnosticSeverity::WARNING)
        } else if self.information_depth > 0 {
            Some(DiagnosticSeverity::INFORMATION)
        } else if self.hint_depth > 0 {
            Some(DiagnosticSeverity::HINT)
        } else {
            None
        }
    }

    fn current_code_is_unnecessary(&self) -> bool {
        self.unnecessary_depth > 0
    }
}

impl<'a> Iterator for BufferChunks<'a> {
    type Item = Chunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut next_capture_start = usize::MAX;
        let mut next_diagnostic_endpoint = usize::MAX;

        if let Some(highlights) = self.highlights.as_mut() {
            while let Some((parent_capture_end, _)) = highlights.stack.last() {
                if *parent_capture_end <= self.range.start {
                    highlights.stack.pop();
                } else {
                    break;
                }
            }

            if highlights.next_capture.is_none() {
                highlights.next_capture = highlights.captures.next();
            }

            while let Some(capture) = highlights.next_capture.as_ref() {
                if self.range.start < capture.node.start_byte() {
                    next_capture_start = capture.node.start_byte();
                    break;
                } else {
                    let highlight_id =
                        highlights.highlight_maps[capture.grammar_index].get(capture.index);
                    if let Some(highlight_id) = highlight_id {
                        highlights
                            .stack
                            .push((capture.node.end_byte(), highlight_id));
                    }
                    highlights.next_capture = highlights.captures.next();
                }
            }
        }

        let mut diagnostic_endpoints = std::mem::take(&mut self.diagnostic_endpoints);
        if let Some(diagnostic_endpoints) = diagnostic_endpoints.as_mut() {
            while let Some(endpoint) = diagnostic_endpoints.peek().copied() {
                if endpoint.offset <= self.range.start {
                    self.update_diagnostic_depths(endpoint);
                    diagnostic_endpoints.next();
                    self.underline = endpoint.underline;
                } else {
                    next_diagnostic_endpoint = endpoint.offset;
                    break;
                }
            }
        }
        self.diagnostic_endpoints = diagnostic_endpoints;

        if let Some(ChunkBitmaps {
            text: chunk,
            chars: chars_map,
            tabs,
            newlines,
        }) = self.chunks.peek_with_bitmaps()
        {
            let chunk_start = self.range.start;
            let mut chunk_end = (self.chunks.offset() + chunk.len())
                .min(next_capture_start)
                .min(next_diagnostic_endpoint);
            let mut highlight_id = None;
            if let Some(highlights) = self.highlights.as_ref()
                && let Some((parent_capture_end, parent_highlight_id)) = highlights.stack.last()
            {
                chunk_end = chunk_end.min(*parent_capture_end);
                highlight_id = Some(*parent_highlight_id);
            }
            let bit_start = chunk_start - self.chunks.offset();
            let bit_end = chunk_end - self.chunks.offset();

            let slice = &chunk[bit_start..bit_end];

            let mask = 1u128
                .unbounded_shl((bit_end - bit_start) as u32)
                .wrapping_sub(1);
            let tabs = (tabs >> bit_start) & mask;
            let chars = (chars_map >> bit_start) & mask;
            let newlines = (newlines >> bit_start) & mask;

            self.range.start = chunk_end;
            if self.range.start == self.chunks.offset() + chunk.len() {
                self.chunks.next().unwrap();
            }

            Some(Chunk {
                text: slice,
                syntax_highlight_id: highlight_id,
                underline: self.underline,
                diagnostic_severity: self.current_diagnostic_severity(),
                is_unnecessary: self.current_code_is_unnecessary(),
                tabs,
                chars,
                newlines,
                ..Chunk::default()
            })
        } else {
            None
        }
    }
}

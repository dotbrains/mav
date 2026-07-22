use super::*;

impl Markdown {
    pub(super) fn is_code_block_wrapped(&self, id: usize) -> bool {
        self.wrapped_code_blocks.contains(&id)
    }

    pub(super) fn toggle_code_block_wrap(&mut self, id: usize) {
        if !self.wrapped_code_blocks.remove(&id) {
            self.wrapped_code_blocks.insert(id);
        }
    }

    pub(super) fn code_block_scroll_handle(&mut self, id: usize) -> Option<ScrollHandle> {
        (!self.is_code_block_wrapped(id)).then(|| {
            self.code_block_scroll_handles
                .entry(id)
                .or_insert_with(ScrollHandle::new)
                .clone()
        })
    }

    pub(super) fn retain_code_block_scroll_handles(&mut self, ids: &HashSet<usize>) {
        self.code_block_scroll_handles
            .retain(|id, _| ids.contains(id));
    }

    pub fn invalidate_mermaid_cache(&mut self, cx: &mut Context<Self>) {
        if !self.options.render_mermaid_diagrams || self.parsed_markdown.mermaid_diagrams.is_empty()
        {
            return;
        }

        self.mermaid_state.clear();
        self.mermaid_state.update(&self.parsed_markdown, cx);
        cx.notify();
    }

    pub(crate) fn is_mermaid_showing_code(&self, source_offset: usize) -> bool {
        self.mermaid_showing_code.contains(&source_offset)
    }

    pub(crate) fn toggle_mermaid_tab(&mut self, source_offset: usize) {
        if !self.mermaid_showing_code.remove(&source_offset) {
            self.mermaid_showing_code.insert(source_offset);
        }
    }

    pub(super) fn clear_code_block_scroll_handles(&mut self) {
        self.code_block_scroll_handles.clear();
    }

    pub(super) fn autoscroll_code_block(
        &self,
        source_index: usize,
        cursor_position: Point<Pixels>,
    ) {
        let Some((_, scroll_handle)) = self
            .code_block_scroll_handles
            .range(..=source_index)
            .next_back()
        else {
            return;
        };

        let bounds = scroll_handle.bounds();
        if cursor_position.y < bounds.top() || cursor_position.y > bounds.bottom() {
            return;
        }

        let horizontal_delta = if cursor_position.x < bounds.left() {
            bounds.left() - cursor_position.x
        } else if cursor_position.x > bounds.right() {
            bounds.right() - cursor_position.x
        } else {
            return;
        };

        let offset = scroll_handle.offset();
        scroll_handle.set_offset(point(offset.x + horizontal_delta, offset.y));
    }

    pub fn is_parsing(&self) -> bool {
        self.pending_parse.is_some()
    }

    pub fn scroll_to_heading(&mut self, slug: &str, cx: &mut Context<Self>) -> Option<usize> {
        if let Some(source_index) = self.parsed_markdown.heading_slugs.get(slug).copied() {
            self.autoscroll_request = Some(source_index);
            cx.notify();
            Some(source_index)
        } else {
            None
        }
    }

    pub fn source(&self) -> &SharedString {
        &self.source
    }

    pub fn first_code_block_language(&self) -> Option<Arc<Language>> {
        self.parsed_markdown.events.iter().find_map(|(_, event)| {
            let MarkdownEvent::Start(MarkdownTag::CodeBlock { kind, .. }) = event else {
                return None;
            };

            match kind {
                CodeBlockKind::FencedLang(language) => self
                    .parsed_markdown
                    .languages_by_name
                    .get(language)
                    .cloned(),
                CodeBlockKind::FencedSrc(path_range) => self
                    .parsed_markdown
                    .languages_by_path
                    .get(&path_range.path)
                    .cloned(),
                CodeBlockKind::Fenced | CodeBlockKind::Indented => None,
            }
        })
    }

    pub fn append(&mut self, text: &str, cx: &mut Context<Self>) {
        self.source = SharedString::new(self.source.to_string() + text);
        self.parse(cx);
    }

    pub fn replace(&mut self, source: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.source = source.into();
        self.parse(cx);
    }

    pub fn request_autoscroll_to_source_index(
        &mut self,
        source_index: usize,
        cx: &mut Context<Self>,
    ) {
        self.autoscroll_request = Some(source_index);
        cx.refresh_windows();
    }

    pub(super) fn footnote_definition_content_start(&self, label: &SharedString) -> Option<usize> {
        self.parsed_markdown
            .footnote_definitions
            .get(label)
            .copied()
    }
}

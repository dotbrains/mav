use super::*;

impl Markdown {
    pub(super) fn parse(&mut self, cx: &mut Context<Self>) {
        if self.source.is_empty() {
            self.should_reparse = false;
            self.pending_parse.take();
            self.parsed_markdown = ParsedMarkdown {
                source: self.source.clone(),
                ..Default::default()
            };
            self.active_root_block = None;
            self.images_by_source_offset.clear();
            self.mermaid_state.clear();
            cx.notify();
            cx.refresh_windows();
            return;
        }

        if self.pending_parse.is_some() {
            self.should_reparse = true;
            return;
        }
        self.should_reparse = false;
        self.pending_parse = Some(self.start_background_parse(cx));
    }

    pub(super) fn start_background_parse(&self, cx: &Context<Self>) -> Task<()> {
        let source = self.source.clone();
        let should_parse_links_only = self.options.parse_links_only;
        let should_parse_html = self.options.parse_html;
        let should_render_mermaid_diagrams = self.options.render_mermaid_diagrams;
        let should_parse_heading_slugs = self.options.parse_heading_slugs;
        let should_parse_metadata_blocks = self.options.render_metadata_blocks;
        let language_registry = self.language_registry.clone();
        let fallback = self.fallback_code_block_language.clone();

        let parsed = cx.background_spawn(async move {
            if should_parse_links_only {
                return (
                    ParsedMarkdown {
                        events: Arc::from(parse_links_only(source.as_ref())),
                        source,
                        languages_by_name: TreeMap::default(),
                        languages_by_path: TreeMap::default(),
                        root_block_starts: Arc::default(),
                        html_blocks: BTreeMap::default(),
                        metadata_blocks: BTreeMap::default(),
                        mermaid_diagrams: BTreeMap::default(),
                        heading_slugs: HashMap::default(),
                        footnote_definitions: HashMap::default(),
                    },
                    Default::default(),
                );
            }

            let parsed = parse_markdown_with_options(
                &source,
                should_parse_html,
                should_parse_heading_slugs,
                should_parse_metadata_blocks,
            );
            let events = parsed.events;
            let language_names = parsed.language_names;
            let paths = parsed.language_paths;
            let root_block_starts = parsed.root_block_starts;
            let html_blocks = parsed.html_blocks;
            let metadata_blocks = parsed.metadata_blocks;
            let heading_slugs = parsed.heading_slugs;
            let footnote_definitions = parsed.footnote_definitions;
            let mermaid_diagrams = if should_render_mermaid_diagrams {
                extract_mermaid_diagrams(&source, &events)
            } else {
                BTreeMap::default()
            };
            let mut images_by_source_offset = HashMap::default();
            let mut languages_by_name = TreeMap::default();
            let mut languages_by_path = TreeMap::default();
            if let Some(registry) = language_registry.as_ref() {
                for name in language_names {
                    let language = if !name.is_empty() {
                        registry.language_for_name_or_extension(&name).left_future()
                    } else if let Some(fallback) = &fallback {
                        registry.language_for_name(fallback.as_ref()).right_future()
                    } else {
                        continue;
                    };
                    if let Ok(language) = language.await {
                        languages_by_name.insert(name, language);
                    }
                }

                for path in paths {
                    if let Ok(language) = registry
                        .load_language_for_file_path(Path::new(path.as_ref()))
                        .await
                    {
                        languages_by_path.insert(path, language);
                    }
                }
            }

            for (range, event) in &events {
                if let MarkdownEvent::Start(MarkdownTag::Image { dest_url, .. }) = event
                    && let Some(data_url) = dest_url.strip_prefix("data:")
                {
                    let Some((mime_info, data)) = data_url.split_once(',') else {
                        continue;
                    };
                    let Some((mime_type, encoding)) = mime_info.split_once(';') else {
                        continue;
                    };
                    let Some(format) = ImageFormat::from_mime_type(mime_type) else {
                        continue;
                    };
                    let is_base64 = encoding == "base64";
                    if is_base64
                        && let Some(bytes) = base64::prelude::BASE64_STANDARD
                            .decode(data)
                            .log_with_level(Level::Debug)
                    {
                        let image = Arc::new(Image::from_bytes(format, bytes));
                        images_by_source_offset.insert(range.start, image);
                    }
                }
            }

            (
                ParsedMarkdown {
                    source,
                    events: Arc::from(events),
                    languages_by_name,
                    languages_by_path,
                    root_block_starts: Arc::from(root_block_starts),
                    html_blocks,
                    metadata_blocks,
                    mermaid_diagrams,
                    heading_slugs,
                    footnote_definitions,
                },
                images_by_source_offset,
            )
        });

        cx.spawn(async move |this, cx| {
            let (parsed, images_by_source_offset) = parsed.await;

            this.update(cx, |this, cx| {
                this.parsed_markdown = parsed;
                this.images_by_source_offset = images_by_source_offset;
                if this.active_root_block.is_some_and(|block_index| {
                    block_index >= this.parsed_markdown.root_block_starts.len()
                }) {
                    this.active_root_block = None;
                }
                if this.options.render_mermaid_diagrams {
                    let parsed_markdown = this.parsed_markdown.clone();
                    this.mermaid_state.update(&parsed_markdown, cx);
                    this.mermaid_showing_code
                        .retain(|offset| parsed_markdown.mermaid_diagrams.contains_key(offset));
                } else {
                    this.mermaid_state.clear();
                    this.mermaid_showing_code.clear();
                }
                this.pending_parse.take();
                if this.should_reparse {
                    this.parse(cx);
                }
                cx.notify();
                cx.refresh_windows();
            })
            .ok();
        })
    }
}

use super::*;

#[derive(Debug, PartialEq, Clone)]
pub enum ContentBlock {
    Empty,
    Markdown {
        markdown: Entity<Markdown>,
    },
    EmbeddedResource {
        resource: acp::EmbeddedResource,
        markdown: Option<Entity<Markdown>>,
    },
    ResourceLink {
        resource_link: acp::ResourceLink,
    },
    Image {
        image: Arc<gpui::Image>,
        dimensions: Option<gpui::Size<u32>>,
    },
}

impl ContentBlock {
    pub fn new(
        block: acp::ContentBlock,
        language_registry: &Arc<LanguageRegistry>,
        path_style: PathStyle,
        cx: &mut App,
    ) -> Self {
        let mut this = Self::Empty;
        this.append(block, language_registry, path_style, cx);
        this
    }

    pub fn new_combined(
        blocks: impl IntoIterator<Item = acp::ContentBlock>,
        language_registry: Arc<LanguageRegistry>,
        path_style: PathStyle,
        cx: &mut App,
    ) -> Self {
        let mut this = Self::Empty;
        for block in blocks {
            this.append(block, &language_registry, path_style, cx);
        }
        this
    }

    pub fn new_tool_call_content(
        block: acp::ContentBlock,
        language_registry: &Arc<LanguageRegistry>,
        path_style: PathStyle,
        cx: &mut App,
    ) -> Self {
        match block {
            acp::ContentBlock::Resource(resource) => {
                if let Some((image, dimensions)) = Self::decode_embedded_resource_image(&resource) {
                    Self::Image { image, dimensions }
                } else {
                    let markdown =
                        Self::embedded_resource_markdown(&resource, language_registry, cx);
                    Self::EmbeddedResource { resource, markdown }
                }
            }
            block => Self::new(block, language_registry, path_style, cx),
        }
    }

    pub fn append(
        &mut self,
        block: acp::ContentBlock,
        language_registry: &Arc<LanguageRegistry>,
        path_style: PathStyle,
        cx: &mut App,
    ) {
        match (&mut *self, &block) {
            (ContentBlock::Empty, acp::ContentBlock::ResourceLink(resource_link)) => {
                *self = ContentBlock::ResourceLink {
                    resource_link: resource_link.clone(),
                };
            }
            (ContentBlock::Empty, acp::ContentBlock::Image(image_content)) => {
                if let Some((image, dimensions)) = Self::decode_image(image_content) {
                    *self = ContentBlock::Image { image, dimensions };
                } else {
                    let new_content = Self::image_md(image_content);
                    *self = Self::create_markdown_block(new_content, language_registry, cx);
                }
            }
            (ContentBlock::Empty, _) => {
                let new_content = Self::block_string_contents(&block, path_style);
                *self = Self::create_markdown_block(new_content, language_registry, cx);
            }
            (ContentBlock::Markdown { markdown }, _) => {
                let new_content = Self::block_string_contents(&block, path_style);
                markdown.update(cx, |markdown, cx| markdown.append(&new_content, cx));
            }
            (ContentBlock::ResourceLink { resource_link }, _) => {
                let existing_content = Self::resource_link_md(&resource_link.uri, path_style);
                let new_content = Self::block_string_contents(&block, path_style);
                let combined = format!("{}\n{}", existing_content, new_content);
                *self = Self::create_markdown_block(combined, language_registry, cx);
            }
            (ContentBlock::EmbeddedResource { resource, .. }, _) => {
                let existing_content =
                    Self::embedded_resource_string_contents(resource, path_style);
                let new_content = Self::block_string_contents(&block, path_style);
                let combined = format!("{}\n{}", existing_content, new_content);
                *self = Self::create_markdown_block(combined, language_registry, cx);
            }
            (ContentBlock::Image { .. }, _) => {
                let new_content = Self::block_string_contents(&block, path_style);
                let combined = format!("`Image`\n{}", new_content);
                *self = Self::create_markdown_block(combined, language_registry, cx);
            }
        }
    }

    /// Updates a Markdown block in place from a streaming text `block`, reusing
    /// the existing `Markdown` entity rather than recreating it. Appends only the
    /// new suffix when the update is a continuation (the common streaming case),
    /// otherwise re-sets the source. Returns `false` when an in-place update isn't
    /// applicable, so the caller can fall back to replacing the block wholesale.
    ///
    /// Recreating the entity on every streamed snapshot causes the rendered
    /// element to tear down and rebuild, which flickers badly.
    pub fn update_text_in_place(&mut self, block: &acp::ContentBlock, cx: &mut App) -> bool {
        let ContentBlock::Markdown { markdown } = self else {
            return false;
        };
        let acp::ContentBlock::Text(text_content) = block else {
            return false;
        };
        let new_content = &text_content.text;
        markdown.update(cx, |markdown, cx| {
            let current = markdown.source().to_string();
            match new_content.strip_prefix(&current) {
                Some("") => {}
                Some(suffix) => markdown.append(suffix, cx),
                None => markdown.reset(new_content.clone().into(), cx),
            }
        });
        true
    }

    fn decode_image(
        image_content: &acp::ImageContent,
    ) -> Option<(Arc<gpui::Image>, Option<gpui::Size<u32>>)> {
        Self::decode_image_data(&image_content.data, &image_content.mime_type)
    }

    fn decode_embedded_resource_image(
        resource: &acp::EmbeddedResource,
    ) -> Option<(Arc<gpui::Image>, Option<gpui::Size<u32>>)> {
        let acp::EmbeddedResourceResource::BlobResourceContents(blob) = &resource.resource else {
            return None;
        };
        let mime_type = blob.mime_type.as_deref()?;
        Self::decode_image_data(&blob.blob, mime_type)
    }

    fn decode_image_data(
        data: &str,
        mime_type: &str,
    ) -> Option<(Arc<gpui::Image>, Option<gpui::Size<u32>>)> {
        use base64::Engine as _;

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data.as_bytes())
            .ok()?;
        let format = gpui::ImageFormat::from_mime_type(mime_type)?;
        let dimensions = Self::image_dimensions(&bytes, format);
        Some((Arc::new(gpui::Image::from_bytes(format, bytes)), dimensions))
    }

    fn image_dimensions(bytes: &[u8], format: gpui::ImageFormat) -> Option<gpui::Size<u32>> {
        let format = match format {
            gpui::ImageFormat::Png => image::ImageFormat::Png,
            gpui::ImageFormat::Jpeg => image::ImageFormat::Jpeg,
            gpui::ImageFormat::Webp => image::ImageFormat::WebP,
            gpui::ImageFormat::Gif => image::ImageFormat::Gif,
            gpui::ImageFormat::Svg => return None,
            gpui::ImageFormat::Bmp => image::ImageFormat::Bmp,
            gpui::ImageFormat::Tiff => image::ImageFormat::Tiff,
            gpui::ImageFormat::Ico => image::ImageFormat::Ico,
            gpui::ImageFormat::Pnm => image::ImageFormat::Pnm,
        };

        image::ImageReader::with_format(std::io::Cursor::new(bytes), format)
            .into_dimensions()
            .ok()
            .map(|(width, height)| gpui::Size { width, height })
    }

    fn create_markdown_block(
        content: String,
        language_registry: &Arc<LanguageRegistry>,
        cx: &mut App,
    ) -> ContentBlock {
        ContentBlock::Markdown {
            markdown: Self::create_markdown(content, language_registry, cx),
        }
    }

    fn create_markdown(
        content: String,
        language_registry: &Arc<LanguageRegistry>,
        cx: &mut App,
    ) -> Entity<Markdown> {
        cx.new(|cx| {
            Markdown::new_with_options(
                content.into(),
                Some(language_registry.clone()),
                None,
                MarkdownOptions {
                    render_mermaid_diagrams: true,
                    render_metadata_blocks: true,
                    ..Default::default()
                },
                cx,
            )
        })
    }

    fn embedded_resource_markdown(
        resource: &acp::EmbeddedResource,
        language_registry: &Arc<LanguageRegistry>,
        cx: &mut App,
    ) -> Option<Entity<Markdown>> {
        match &resource.resource {
            acp::EmbeddedResourceResource::TextResourceContents(text) => Some(
                Self::create_markdown(Self::text_resource_markdown(text), language_registry, cx),
            ),
            acp::EmbeddedResourceResource::BlobResourceContents(_) => None,
            _ => None,
        }
    }

    pub(super) fn text_resource_markdown(resource: &acp::TextResourceContents) -> String {
        match text_resource_render_mode(resource.mime_type.as_deref()) {
            TextResourceRenderMode::Markdown => resource.text.clone(),
            TextResourceRenderMode::CodeBlock(language) => {
                Self::fenced_code_block(&resource.text, language)
            }
        }
    }

    pub fn text_content<'a>(&'a self, cx: &'a App) -> Option<&'a str> {
        match self {
            ContentBlock::Markdown { markdown } => Some(markdown.read(cx).source()),
            ContentBlock::EmbeddedResource { resource, .. } => match &resource.resource {
                acp::EmbeddedResourceResource::TextResourceContents(text) => Some(&text.text),
                acp::EmbeddedResourceResource::BlobResourceContents(_) => None,
                _ => None,
            },
            ContentBlock::Empty
            | ContentBlock::ResourceLink { .. }
            | ContentBlock::Image { .. } => None,
        }
    }

    fn fenced_code_block(text: &str, language: Option<&str>) -> String {
        let fence_len = text
            .as_bytes()
            .chunk_by(|left, right| left == right)
            .filter(|chunk| chunk.first() == Some(&b'`'))
            .map(|chunk| chunk.len() + 1)
            .max()
            .unwrap_or(3)
            .max(3);
        let fence = "`".repeat(fence_len);

        let mut markdown = String::new();
        markdown.push_str(&fence);
        if let Some(language) = language {
            markdown.push_str(language);
        }
        markdown.push('\n');
        markdown.push_str(text);
        if !text.ends_with('\n') {
            markdown.push('\n');
        }
        markdown.push_str(&fence);
        markdown
    }

    fn embedded_resource_string_contents(
        resource: &acp::EmbeddedResource,
        path_style: PathStyle,
    ) -> String {
        match &resource.resource {
            acp::EmbeddedResourceResource::TextResourceContents(text) => {
                Self::resource_link_md(&text.uri, path_style)
            }
            acp::EmbeddedResourceResource::BlobResourceContents(blob) => {
                Self::resource_link_md(&blob.uri, path_style)
            }
            _ => String::new(),
        }
    }

    fn embedded_resource_text(resource: &acp::EmbeddedResource) -> &str {
        match &resource.resource {
            acp::EmbeddedResourceResource::TextResourceContents(text) => &text.text,
            acp::EmbeddedResourceResource::BlobResourceContents(blob) => &blob.uri,
            _ => "",
        }
    }

    pub(super) fn embedded_resource_label(resource: &acp::EmbeddedResource) -> &str {
        match &resource.resource {
            acp::EmbeddedResourceResource::TextResourceContents(text) => &text.uri,
            acp::EmbeddedResourceResource::BlobResourceContents(blob) => &blob.uri,
            _ => "",
        }
    }

    pub fn embedded_resource(&self) -> Option<(&acp::EmbeddedResource, Option<&Entity<Markdown>>)> {
        match self {
            ContentBlock::EmbeddedResource { resource, markdown } => {
                Some((resource, markdown.as_ref()))
            }
            _ => None,
        }
    }

    pub fn visible_content(&self, cx: &App) -> bool {
        match self {
            ContentBlock::Empty => false,
            ContentBlock::Markdown { markdown } => !markdown.read(cx).source().trim().is_empty(),
            ContentBlock::EmbeddedResource { resource, markdown } => match markdown {
                Some(markdown) => !markdown.read(cx).source().trim().is_empty(),
                None => !Self::embedded_resource_text(resource).trim().is_empty(),
            },
            ContentBlock::ResourceLink { .. } | ContentBlock::Image { .. } => true,
        }
    }

    fn block_string_contents(block: &acp::ContentBlock, path_style: PathStyle) -> String {
        match block {
            acp::ContentBlock::Text(text_content) => text_content.text.clone(),
            acp::ContentBlock::ResourceLink(resource_link) => {
                Self::resource_link_md(&resource_link.uri, path_style)
            }
            acp::ContentBlock::Resource(acp::EmbeddedResource {
                resource:
                    acp::EmbeddedResourceResource::TextResourceContents(acp::TextResourceContents {
                        uri,
                        ..
                    }),
                ..
            }) => Self::resource_link_md(uri, path_style),
            acp::ContentBlock::Image(image) => Self::image_md(image),
            _ => String::new(),
        }
    }

    fn resource_link_md(uri: &str, path_style: PathStyle) -> String {
        if let Some(uri) = MentionUri::parse(uri, path_style).log_err() {
            uri.as_link().to_string()
        } else {
            uri.to_string()
        }
    }

    fn image_md(_image: &acp::ImageContent) -> String {
        "`Image`".into()
    }

    pub fn to_markdown<'a>(&'a self, cx: &'a App) -> &'a str {
        match self {
            ContentBlock::Empty => "",
            ContentBlock::Markdown { markdown } => markdown.read(cx).source(),
            ContentBlock::EmbeddedResource { resource, markdown } => {
                if let Some(markdown) = markdown {
                    markdown.read(cx).source()
                } else {
                    Self::embedded_resource_label(resource)
                }
            }
            ContentBlock::ResourceLink { resource_link } => &resource_link.uri,
            ContentBlock::Image { .. } => "`Image`",
        }
    }

    pub fn markdown(&self) -> Option<&Entity<Markdown>> {
        match self {
            ContentBlock::Empty => None,
            ContentBlock::Markdown { markdown } => Some(markdown),
            ContentBlock::EmbeddedResource { markdown, .. } => markdown.as_ref(),
            ContentBlock::ResourceLink { .. } => None,
            ContentBlock::Image { .. } => None,
        }
    }

    pub fn resource_link(&self) -> Option<&acp::ResourceLink> {
        match self {
            ContentBlock::ResourceLink { resource_link } => Some(resource_link),
            _ => None,
        }
    }

    pub fn image(&self) -> Option<(&Arc<gpui::Image>, Option<gpui::Size<u32>>)> {
        match self {
            ContentBlock::Image { image, dimensions } => Some((image, *dimensions)),
            _ => None,
        }
    }
}

enum TextResourceRenderMode {
    Markdown,
    CodeBlock(Option<&'static str>),
}

fn text_resource_render_mode(mime_type: Option<&str>) -> TextResourceRenderMode {
    let Some(mime_type) = mime_type else {
        return TextResourceRenderMode::CodeBlock(None);
    };
    let Ok(mime) = mime_type.parse::<mime::Mime>() else {
        return TextResourceRenderMode::CodeBlock(None);
    };

    let type_ = mime.type_().as_str();
    let subtype = mime.subtype().as_str();
    let suffix = mime.suffix().map(|suffix| suffix.as_str());

    if matches!(
        (type_, subtype),
        ("text", "markdown") | ("text", "x-markdown")
    ) {
        return TextResourceRenderMode::Markdown;
    }

    let language = match (type_, subtype, suffix) {
        (_, "json", _) | (_, _, Some("json")) => Some("json"),
        (_, "xml", _) | (_, _, Some("xml")) => Some("xml"),
        ("text", "html", _) => Some("html"),
        ("text", "css", _) => Some("css"),
        ("text", "csv", _) => Some("csv"),
        ("text", "tab-separated-values", _) => Some("tsv"),
        ("text", "javascript", _) | ("application", "javascript", _) => Some("javascript"),
        ("application", "x-javascript", _) => Some("javascript"),
        ("text", "typescript", _) | ("application", "typescript", _) => Some("typescript"),
        ("text", "x-shellscript", _) | ("application", "x-shellscript", _) => Some("sh"),
        ("application", "x-sh", _) => Some("sh"),
        ("text", "x-python", _) => Some("python"),
        ("text", "x-rust", _) => Some("rust"),
        ("text", "x-go", _) => Some("go"),
        ("text", "x-ruby", _) => Some("ruby"),
        ("text", "x-c", _) => Some("c"),
        // `mime` parses `text/x-c++` as subtype `x-c+` with an empty suffix.
        ("text", "x-c+", Some("")) => Some("cpp"),
        ("text", "plain", _) => None,
        ("text", _, _) => None,
        ("application", "graphql", _) => Some("graphql"),
        ("application", "toml", _) => Some("toml"),
        ("application", "yaml", _) | ("application", "x-yaml", _) => Some("yaml"),
        (_, _, Some("yaml" | "yml")) => Some("yaml"),
        _ => return TextResourceRenderMode::CodeBlock(None),
    };

    TextResourceRenderMode::CodeBlock(language)
}

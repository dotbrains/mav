use super::*;

#[path = "element/block_helpers.rs"]
mod block_helpers;
#[path = "element/end_tag.rs"]
mod end_tag;
#[path = "element/interaction.rs"]
mod interaction;
#[path = "element/layout.rs"]
mod layout;
#[path = "element/start_tag.rs"]
mod start_tag;

pub enum AutoscrollBehavior {
    /// Propagate the request up the element tree for the nearest
    /// scrollable ancestor (e.g. `List`) to handle.
    Propagate,
    /// Directly control a specific scroll handle.
    Controlled(ScrollHandle),
}

pub struct MarkdownElement {
    markdown: Entity<Markdown>,
    pub(super) style: MarkdownStyle,
    pub(super) code_block_renderer: CodeBlockRenderer,
    pub(super) on_url_click: Option<Box<dyn Fn(SharedString, &mut Window, &mut App)>>,
    pub(super) code_span_link: Option<CodeSpanLinkCallback>,
    pub(super) on_source_click: Option<SourceClickCallback>,
    pub(super) on_checkbox_toggle: Option<CheckboxToggleCallback>,
    pub(super) image_resolver: Option<Box<dyn Fn(&str) -> Option<ImageSource>>>,
    pub(super) show_root_block_markers: bool,
    pub(super) autoscroll: AutoscrollBehavior,
}

impl MarkdownElement {
    pub fn new(markdown: Entity<Markdown>, style: MarkdownStyle) -> Self {
        Self {
            markdown,
            style,
            code_block_renderer: CodeBlockRenderer::Default {
                copy_button_visibility: CopyButtonVisibility::VisibleOnHover,
                wrap_button_visibility: WrapButtonVisibility::Hidden,
                border: false,
            },
            on_url_click: None,
            code_span_link: None,
            on_source_click: None,
            on_checkbox_toggle: None,
            image_resolver: None,
            show_root_block_markers: false,
            autoscroll: AutoscrollBehavior::Propagate,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn rendered_text(
        markdown: Entity<Markdown>,
        cx: &mut gpui::VisualTestContext,
        style: impl FnOnce(&Window, &App) -> MarkdownStyle,
    ) -> String {
        use gpui::size;

        let (text, _) = cx.draw(
            Default::default(),
            size(px(600.0), px(600.0)),
            |window, cx| Self::new(markdown, style(window, cx)),
        );
        text.text
            .lines
            .iter()
            .map(|line| line.layout.wrapped_text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn code_block_renderer(mut self, variant: CodeBlockRenderer) -> Self {
        self.code_block_renderer = variant;
        self
    }

    pub fn on_url_click(
        mut self,
        handler: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_url_click = Some(Box::new(handler));
        self
    }

    pub fn on_code_span_link(
        mut self,
        callback: impl Fn(&str, &App) -> Option<SharedString> + 'static,
    ) -> Self {
        self.code_span_link = Some(Arc::new(callback));
        self
    }

    pub fn on_source_click(
        mut self,
        handler: impl Fn(usize, usize, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_source_click = Some(Box::new(handler));
        self
    }

    pub fn on_checkbox_toggle(
        mut self,
        handler: impl Fn(Range<usize>, bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_checkbox_toggle = Some(Rc::new(handler));
        self
    }

    pub fn image_resolver(
        mut self,
        resolver: impl Fn(&str) -> Option<ImageSource> + 'static,
    ) -> Self {
        self.image_resolver = Some(Box::new(resolver));
        self
    }

    pub fn show_root_block_markers(mut self) -> Self {
        self.show_root_block_markers = true;
        self
    }

    pub fn scroll_handle(mut self, scroll_handle: ScrollHandle) -> Self {
        self.autoscroll = AutoscrollBehavior::Controlled(scroll_handle);
        self
    }
}

impl IntoElement for MarkdownElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

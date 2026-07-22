use super::*;

#[derive(Clone)]
pub struct FoldPlaceholder {
    /// Creates an element to represent this fold's placeholder.
    pub render: Arc<dyn Send + Sync + Fn(FoldId, Range<Anchor>, &mut App) -> AnyElement>,
    /// If true, the element is constrained to the shaped width of an ellipsis.
    pub constrain_width: bool,
    /// If true, merges the fold with an adjacent one.
    pub merge_adjacent: bool,
    /// Category of the fold. Useful for carefully removing from overlapping folds.
    pub type_tag: Option<TypeId>,
    /// Text provided by the language server to display in place of the folded range.
    /// When set, this is used instead of the default "⋯" ellipsis.
    pub collapsed_text: Option<SharedString>,
}

impl Default for FoldPlaceholder {
    fn default() -> Self {
        Self {
            render: Arc::new(|_, _, _| gpui::Empty.into_any_element()),
            constrain_width: true,
            merge_adjacent: true,
            type_tag: None,
            collapsed_text: None,
        }
    }
}

impl FoldPlaceholder {
    /// Returns a styled `Div` container with the standard fold‐placeholder
    /// look (background, hover, active, rounded corners, full size).
    /// Callers add children and event handlers on top.
    pub fn fold_element(fold_id: FoldId, cx: &App) -> Stateful<gpui::Div> {
        use gpui::{InteractiveElement as _, StatefulInteractiveElement as _, Styled as _};
        use settings::Settings as _;
        use theme::ActiveTheme as _;
        use theme_settings::ThemeSettings;
        let settings = ThemeSettings::get_global(cx);
        gpui::div()
            .id(fold_id)
            .font(settings.buffer_font.clone())
            .text_color(cx.theme().colors().text_placeholder)
            .bg(cx.theme().colors().ghost_element_background)
            .hover(|style| style.bg(cx.theme().colors().ghost_element_hover))
            .active(|style| style.bg(cx.theme().colors().ghost_element_active))
            .rounded_xs()
            .size_full()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test() -> Self {
        Self {
            render: Arc::new(|_id, _range, _cx| gpui::Empty.into_any_element()),
            constrain_width: true,
            merge_adjacent: true,
            type_tag: None,
            collapsed_text: None,
        }
    }
}

impl fmt::Debug for FoldPlaceholder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FoldPlaceholder")
            .field("constrain_width", &self.constrain_width)
            .field("collapsed_text", &self.collapsed_text)
            .finish()
    }
}

impl Eq for FoldPlaceholder {}

impl PartialEq for FoldPlaceholder {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.render, &other.render)
            && self.constrain_width == other.constrain_width
            && self.collapsed_text == other.collapsed_text
    }
}

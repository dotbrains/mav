use super::*;

type LinkStyleCallback = Rc<dyn Fn(&str, &App) -> Option<TextStyleRefinement>>;
#[derive(Clone, Copy, Default)]
pub struct BlockQuoteKindColors {
    pub note: Hsla,
    pub tip: Hsla,
    pub important: Hsla,
    pub warning: Hsla,
    pub caution: Hsla,
}

impl BlockQuoteKindColors {
    pub(super) fn for_kind(&self, kind: Option<BlockQuoteKind>, default: Hsla) -> Hsla {
        match kind {
            Some(BlockQuoteKind::Note) => self.note,
            Some(BlockQuoteKind::Tip) => self.tip,
            Some(BlockQuoteKind::Important) => self.important,
            Some(BlockQuoteKind::Warning) => self.warning,
            Some(BlockQuoteKind::Caution) => self.caution,
            None => default,
        }
    }
}

#[derive(Clone, Default)]
pub struct HeadingLevelStyles {
    pub h1: Option<TextStyleRefinement>,
    pub h2: Option<TextStyleRefinement>,
    pub h3: Option<TextStyleRefinement>,
    pub h4: Option<TextStyleRefinement>,
    pub h5: Option<TextStyleRefinement>,
    pub h6: Option<TextStyleRefinement>,
}

#[derive(Clone)]
pub struct MarkdownStyle {
    pub base_text_style: TextStyle,
    pub container_style: StyleRefinement,
    pub code_block: StyleRefinement,
    pub code_block_overflow_x_scroll: bool,
    pub inline_code: TextStyleRefinement,
    pub block_quote: TextStyleRefinement,
    pub link: TextStyleRefinement,
    pub link_callback: Option<LinkStyleCallback>,
    pub rule_color: Hsla,
    pub block_quote_border_color: Hsla,
    pub block_quote_kind_colors: BlockQuoteKindColors,
    pub syntax: Arc<SyntaxTheme>,
    pub selection_background_color: Hsla,
    pub heading: StyleRefinement,
    pub heading_level_styles: Option<HeadingLevelStyles>,
    pub heading_border_color: Option<Hsla>,
    pub height_is_multiple_of_line_height: bool,
    pub prevent_mouse_interaction: bool,
    pub table_columns_min_size: bool,
    pub soft_break_as_hard_break: bool,
}

impl Default for MarkdownStyle {
    fn default() -> Self {
        Self {
            base_text_style: Default::default(),
            container_style: Default::default(),
            code_block: Default::default(),
            code_block_overflow_x_scroll: false,
            inline_code: Default::default(),
            block_quote: Default::default(),
            link: Default::default(),
            link_callback: None,
            rule_color: Default::default(),
            block_quote_border_color: Default::default(),
            block_quote_kind_colors: Default::default(),
            syntax: Arc::new(SyntaxTheme::default()),
            selection_background_color: Default::default(),
            heading: Default::default(),
            heading_level_styles: None,
            heading_border_color: None,
            height_is_multiple_of_line_height: false,
            prevent_mouse_interaction: false,
            table_columns_min_size: false,
            soft_break_as_hard_break: false,
        }
    }
}

#[derive(Clone, Copy)]
pub enum MarkdownFont {
    Agent,
    Editor,
    Preview,
}

impl MarkdownStyle {
    pub fn themed(font: MarkdownFont, window: &Window, cx: &App) -> Self {
        let colors = cx.theme().colors();
        let syntax = cx.theme().syntax().clone();
        Self::themed_with_overrides(font, colors, &syntax, window, cx)
    }

    /// Like [`Self::themed`], but takes explicit [`ThemeColors`] and
    /// [`SyntaxTheme`] so callers (e.g. the markdown preview) can render the
    /// markdown using a theme other than the active editor theme.
    pub fn themed_with_overrides(
        font: MarkdownFont,
        colors: &theme::ThemeColors,
        syntax: &Arc<SyntaxTheme>,
        window: &Window,
        cx: &App,
    ) -> Self {
        let theme_settings = ThemeSettings::get_global(cx);
        let is_preview = matches!(font, MarkdownFont::Preview);

        let buffer_font_weight = theme_settings.buffer_font.weight;
        let (buffer_font_size, ui_font_size) = match font {
            MarkdownFont::Agent => (
                theme_settings.agent_buffer_font_size(cx),
                theme_settings.agent_ui_font_size(cx),
            ),
            MarkdownFont::Editor => (
                theme_settings.buffer_font_size(cx),
                theme_settings.ui_font_size(cx),
            ),
            MarkdownFont::Preview => (
                theme_settings.markdown_preview_font_size(cx),
                theme_settings.ui_font_size(cx),
            ),
        };

        let body_font_family = if is_preview {
            theme_settings.markdown_preview_font_family().clone()
        } else {
            theme_settings.ui_font.family.clone()
        };
        let code_font_family = if is_preview {
            theme_settings.markdown_preview_code_font_family().clone()
        } else {
            theme_settings.buffer_font.family.clone()
        };

        let mut text_style = window.text_style();
        let line_height = buffer_font_size * 1.75;

        text_style.refine(&TextStyleRefinement {
            font_family: Some(body_font_family),
            font_fallbacks: theme_settings.ui_font.fallbacks.clone(),
            font_features: Some(theme_settings.ui_font.features.clone()),
            font_size: Some(if is_preview {
                rems(1.0).into()
            } else {
                ui_font_size.into()
            }),
            line_height: Some(line_height.into()),
            color: Some(colors.text),
            ..Default::default()
        });

        let style = MarkdownStyle {
            base_text_style: text_style.clone(),
            syntax: syntax.clone(),
            selection_background_color: colors.element_selection_background,
            rule_color: colors.border,
            block_quote_border_color: colors.border,
            block_quote_kind_colors: {
                let status = cx.theme().status();
                BlockQuoteKindColors {
                    note: status.info,
                    tip: status.success,
                    important: status.info,
                    warning: status.warning,
                    caution: status.error,
                }
            },
            code_block_overflow_x_scroll: true,
            code_block: StyleRefinement {
                padding: EdgesRefinement {
                    top: Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(8.)))),
                    left: Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(8.)))),
                    right: Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(8.)))),
                    bottom: Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(8.)))),
                },
                margin: EdgesRefinement {
                    top: Some(Length::Definite(px(8.).into())),
                    left: Some(Length::Definite(px(0.).into())),
                    right: Some(Length::Definite(px(0.).into())),
                    bottom: Some(Length::Definite(px(12.).into())),
                },
                border_style: Some(BorderStyle::Solid),
                border_widths: EdgesRefinement {
                    top: Some(AbsoluteLength::Pixels(px(1.))),
                    left: Some(AbsoluteLength::Pixels(px(1.))),
                    right: Some(AbsoluteLength::Pixels(px(1.))),
                    bottom: Some(AbsoluteLength::Pixels(px(1.))),
                },
                border_color: Some(colors.border_variant),
                background: Some(colors.editor_background.into()),
                text: TextStyleRefinement {
                    font_family: Some(code_font_family.clone()),
                    font_fallbacks: theme_settings.buffer_font.fallbacks.clone(),
                    font_features: Some(theme_settings.buffer_font.features.clone()),
                    font_size: Some(buffer_font_size.into()),
                    font_weight: Some(buffer_font_weight),
                    ..Default::default()
                },
                ..Default::default()
            },
            inline_code: TextStyleRefinement {
                font_family: Some(code_font_family),
                font_fallbacks: theme_settings.buffer_font.fallbacks.clone(),
                font_features: Some(theme_settings.buffer_font.features.clone()),
                font_size: Some(buffer_font_size.into()),
                font_weight: Some(buffer_font_weight),
                background_color: Some(colors.editor_foreground.opacity(0.08)),
                ..Default::default()
            },
            link: TextStyleRefinement {
                background_color: Some(colors.editor_foreground.opacity(0.025)),
                color: Some(colors.text_accent),
                underline: Some(UnderlineStyle {
                    color: Some(colors.text_accent.opacity(0.5)),
                    thickness: px(1.),
                    ..Default::default()
                }),
                ..Default::default()
            },
            soft_break_as_hard_break: matches!(font, MarkdownFont::Agent),
            heading_level_styles: matches!(font, MarkdownFont::Agent).then_some(
                HeadingLevelStyles {
                    h1: Some(TextStyleRefinement {
                        font_size: Some(rems(1.15).into()),
                        ..Default::default()
                    }),
                    h2: Some(TextStyleRefinement {
                        font_size: Some(rems(1.1).into()),
                        ..Default::default()
                    }),
                    h3: Some(TextStyleRefinement {
                        font_size: Some(rems(1.05).into()),
                        ..Default::default()
                    }),
                    h4: Some(TextStyleRefinement {
                        font_size: Some(rems(1.).into()),
                        ..Default::default()
                    }),
                    h5: Some(TextStyleRefinement {
                        font_size: Some(rems(0.95).into()),
                        ..Default::default()
                    }),
                    h6: Some(TextStyleRefinement {
                        font_size: Some(rems(0.875).into()),
                        ..Default::default()
                    }),
                },
            ),
            ..Default::default()
        };

        if is_preview {
            style.with_preview_overrides(colors)
        } else {
            style
        }
    }

    fn with_preview_overrides(mut self, colors: &theme::ThemeColors) -> Self {
        let body_font_size = rems(0.92);
        self.base_text_style.font_size = body_font_size.into();
        self.container_style.text.font_size = Some(body_font_size.into());

        self.base_text_style.color = colors.text_muted.blend(colors.text.opacity(0.25));
        self.inline_code.color = Some(colors.text);
        self.heading.text.color = Some(colors.text);

        self.heading_level_styles = Some(HeadingLevelStyles {
            h1: Some(TextStyleRefinement {
                font_size: Some(rems(1.45).into()),
                ..Default::default()
            }),
            h2: Some(TextStyleRefinement {
                font_size: Some(rems(1.3).into()),
                ..Default::default()
            }),
            h3: Some(TextStyleRefinement {
                font_size: Some(rems(1.1).into()),
                ..Default::default()
            }),
            h4: Some(TextStyleRefinement {
                font_size: Some(rems(1.01).into()),
                ..Default::default()
            }),
            h5: Some(TextStyleRefinement {
                font_size: Some(rems(0.95).into()),
                ..Default::default()
            }),
            h6: Some(TextStyleRefinement {
                font_size: Some(rems(0.85).into()),
                ..Default::default()
            }),
        });

        self.heading_border_color = Some(colors.border_variant);

        self
    }

    pub fn with_buffer_font(mut self, cx: &App) -> Self {
        let theme_settings = ThemeSettings::get_global(cx);
        self.base_text_style.font_family = theme_settings.buffer_font.family.clone();
        self.base_text_style.font_fallbacks = theme_settings.buffer_font.fallbacks.clone();
        self.base_text_style.font_features = theme_settings.buffer_font.features.clone();
        self.base_text_style.font_weight = theme_settings.buffer_font.weight;
        self
    }

    pub fn with_muted_text(mut self, cx: &App) -> Self {
        let colors = cx.theme().colors();
        self.base_text_style.color = colors.text_muted;
        self
    }
}

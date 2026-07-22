use super::*;

pub(super) fn parse_blocks(
    blocks: &[HoverBlock],
    language_registry: Option<&Arc<LanguageRegistry>>,
    language: Option<Arc<Language>>,
    cx: &mut AsyncWindowContext,
) -> Option<Entity<Markdown>> {
    let combined_text = blocks
        .iter()
        .map(|block| match &block.kind {
            project::HoverBlockKind::PlainText | project::HoverBlockKind::Markdown => {
                Cow::Borrowed(block.text.trim())
            }
            project::HoverBlockKind::Code { language } => {
                Cow::Owned(format!("```{}\n{}\n```", language, block.text.trim()))
            }
        })
        .join("\n\n");

    cx.new_window_entity(|_window, cx| {
        Markdown::new(
            combined_text.into(),
            language_registry.cloned(),
            language.map(|language| language.name()),
            cx,
        )
    })
    .ok()
}

pub fn hover_markdown_style(window: &Window, cx: &App) -> MarkdownStyle {
    let settings = ThemeSettings::get_global(cx);
    let ui_font_family = settings.ui_font.family.clone();
    let ui_font_features = settings.ui_font.features.clone();
    let ui_font_fallbacks = settings.ui_font.fallbacks.clone();
    let buffer_font_family = settings.buffer_font.family.clone();
    let buffer_font_features = settings.buffer_font.features.clone();
    let buffer_font_fallbacks = settings.buffer_font.fallbacks.clone();
    let buffer_font_weight = settings.buffer_font.weight;

    let mut base_text_style = window.text_style();
    base_text_style.refine(&TextStyleRefinement {
        font_family: Some(ui_font_family),
        font_features: Some(ui_font_features),
        font_fallbacks: ui_font_fallbacks,
        color: Some(cx.theme().colors().editor_foreground),
        ..Default::default()
    });
    MarkdownStyle {
        base_text_style,
        code_block: StyleRefinement::default()
            .my(rems(1.))
            .font_buffer(cx)
            .font_features(buffer_font_features.clone())
            .font_weight(buffer_font_weight),
        inline_code: TextStyleRefinement {
            background_color: Some(cx.theme().colors().background),
            font_family: Some(buffer_font_family),
            font_features: Some(buffer_font_features),
            font_fallbacks: buffer_font_fallbacks,
            font_weight: Some(buffer_font_weight),
            ..Default::default()
        },
        rule_color: cx.theme().colors().border,
        block_quote_border_color: Color::Muted.color(cx),
        block_quote: TextStyleRefinement {
            color: Some(Color::Muted.color(cx)),
            ..Default::default()
        },
        link: TextStyleRefinement {
            color: Some(cx.theme().colors().editor_foreground),
            underline: Some(gpui::UnderlineStyle {
                thickness: px(1.),
                color: Some(cx.theme().colors().editor_foreground),
                wavy: false,
            }),
            ..Default::default()
        },
        syntax: cx.theme().syntax().clone(),
        selection_background_color: cx.theme().colors().element_selection_background,
        heading: StyleRefinement::default()
            .font_weight(FontWeight::BOLD)
            .text_base()
            .mt(rems(1.))
            .mb_0(),
        table_columns_min_size: true,
        soft_break_as_hard_break: true,
        ..Default::default()
    }
}

pub fn diagnostics_markdown_style(window: &Window, cx: &App) -> MarkdownStyle {
    let settings = ThemeSettings::get_global(cx);
    let ui_font_family = settings.ui_font.family.clone();
    let ui_font_fallbacks = settings.ui_font.fallbacks.clone();
    let ui_font_features = settings.ui_font.features.clone();
    let buffer_font_family = settings.buffer_font.family.clone();
    let buffer_font_features = settings.buffer_font.features.clone();
    let buffer_font_fallbacks = settings.buffer_font.fallbacks.clone();

    let mut base_text_style = window.text_style();
    base_text_style.refine(&TextStyleRefinement {
        font_family: Some(ui_font_family),
        font_features: Some(ui_font_features),
        font_fallbacks: ui_font_fallbacks,
        color: Some(cx.theme().colors().editor_foreground),
        ..Default::default()
    });
    MarkdownStyle {
        base_text_style,
        code_block: StyleRefinement::default().my(rems(1.)).font_buffer(cx),
        inline_code: TextStyleRefinement {
            background_color: Some(cx.theme().colors().editor_background.opacity(0.5)),
            font_family: Some(buffer_font_family),
            font_features: Some(buffer_font_features),
            font_fallbacks: buffer_font_fallbacks,
            ..Default::default()
        },
        rule_color: cx.theme().colors().border,
        block_quote_border_color: Color::Muted.color(cx),
        block_quote: TextStyleRefinement {
            color: Some(Color::Muted.color(cx)),
            ..Default::default()
        },
        link: TextStyleRefinement {
            color: Some(cx.theme().colors().editor_foreground),
            underline: Some(gpui::UnderlineStyle {
                thickness: px(1.),
                color: Some(cx.theme().colors().editor_foreground),
                wavy: false,
            }),
            ..Default::default()
        },
        syntax: cx.theme().syntax().clone(),
        selection_background_color: cx.theme().colors().element_selection_background,
        height_is_multiple_of_line_height: true,
        heading: StyleRefinement::default()
            .font_weight(FontWeight::BOLD)
            .text_base()
            .mb_0(),
        table_columns_min_size: true,
        ..Default::default()
    }
}

pub fn open_markdown_url(
    workspace: Option<Entity<Workspace>>,
    link: SharedString,
    window: &mut Window,
    cx: &mut App,
) {
    if let Ok(uri) = Url::parse(&link)
        && uri.scheme() == "file"
        && let Some(workspace) = workspace
    {
        workspace.update(cx, |workspace, cx| {
            let task = workspace.open_abs_path(
                PathBuf::from(uri.path()),
                OpenOptions {
                    visible: Some(OpenVisible::None),
                    ..Default::default()
                },
                window,
                cx,
            );

            cx.spawn_in(window, async move |_, cx| {
                let item = task.await?;
                // Ruby LSP uses URLs with #L1,1-4,4
                // we'll just take the first number and assume it's a line number
                let Some(fragment) = uri.fragment() else {
                    return anyhow::Ok(());
                };
                let mut accum = 0u32;
                for c in fragment.chars() {
                    if c >= '0' && c <= '9' && accum < u32::MAX / 2 {
                        accum *= 10;
                        accum += c as u32 - '0' as u32;
                    } else if accum > 0 {
                        break;
                    }
                }
                if accum == 0 {
                    return Ok(());
                }
                let Some(editor) = cx.update(|_, cx| item.act_as::<Editor>(cx))? else {
                    return Ok(());
                };
                editor.update_in(cx, |editor, window, cx| {
                    editor.change_selections(Default::default(), window, cx, |selections| {
                        selections.select_ranges([
                            text::Point::new(accum - 1, 0)..text::Point::new(accum - 1, 0)
                        ]);
                    });
                })
            })
            .detach_and_log_err(cx);
        });
        return;
    }

    if let Some(workspace) = workspace {
        workspace.update(cx, |workspace, cx| {
            workspace.open_url_or_file(&link, None, window, cx);
        });
    } else {
        cx.open_url(&link);
    }
}

use super::*;
use gpui::{RenderImage, TestAppContext, UpdateGlobal, size};
use language::{Language, LanguageConfig, LanguageMatcher};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct TestWindow;

impl Render for TestWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn ensure_theme_initialized(cx: &mut TestAppContext) {
    cx.update(|cx| {
        if !cx.has_global::<settings::SettingsStore>() {
            settings::init(cx);
        }
        if !cx.has_global::<theme::GlobalTheme>() {
            theme_settings::init(theme::LoadThemes::JustBase, cx);
        }
    });
}

fn render_markdown(markdown: &str, cx: &mut TestAppContext) -> RenderedText {
    render_markdown_with_language_registry(markdown, None, cx)
}

fn render_markdown_with_code_span_link(
    markdown: &str,
    callback: impl Fn(&str, &App) -> Option<SharedString> + 'static,
    cx: &mut TestAppContext,
) -> RenderedText {
    render_markdown_with_code_span_link_style(markdown, MarkdownStyle::default(), callback, cx)
}

fn render_markdown_with_code_span_link_style(
    markdown: &str,
    style: MarkdownStyle,
    callback: impl Fn(&str, &App) -> Option<SharedString> + 'static,
    cx: &mut TestAppContext,
) -> RenderedText {
    ensure_theme_initialized(cx);

    let (_, cx) = cx.add_window_view(|_, _| TestWindow);
    let markdown = cx.new(|cx| Markdown::new(markdown.to_string().into(), None, None, cx));
    cx.run_until_parked();
    let (rendered, _) = cx.draw(
        Default::default(),
        size(px(600.0), px(600.0)),
        |_window, _cx| {
            MarkdownElement::new(markdown, style)
                .on_code_span_link(callback)
                .code_block_renderer(CodeBlockRenderer::Default {
                    copy_button_visibility: CopyButtonVisibility::Hidden,
                    wrap_button_visibility: WrapButtonVisibility::Hidden,
                    border: false,
                })
        },
    );
    rendered.text
}

fn render_markdown_with_language_registry(
    markdown: &str,
    language_registry: Option<Arc<LanguageRegistry>>,
    cx: &mut TestAppContext,
) -> RenderedText {
    render_markdown_with_options(markdown, language_registry, MarkdownOptions::default(), cx)
}

fn render_markdown_with_options(
    markdown: &str,
    language_registry: Option<Arc<LanguageRegistry>>,
    options: MarkdownOptions,
    cx: &mut TestAppContext,
) -> RenderedText {
    ensure_theme_initialized(cx);

    let (_, cx) = cx.add_window_view(|_, _| TestWindow);
    let markdown = cx.new(|cx| {
        Markdown::new_with_options(
            markdown.to_string().into(),
            language_registry,
            None,
            options,
            cx,
        )
    });
    cx.run_until_parked();
    let (rendered, _) = cx.draw(
        Default::default(),
        size(px(600.0), px(600.0)),
        |_window, _cx| {
            MarkdownElement::new(markdown, MarkdownStyle::default()).code_block_renderer(
                CodeBlockRenderer::Default {
                    copy_button_visibility: CopyButtonVisibility::Hidden,
                    wrap_button_visibility: WrapButtonVisibility::Hidden,
                    border: false,
                },
            )
        },
    );
    rendered.text
}

fn render_markdown_with_image_resolver(
    markdown: &str,
    options: MarkdownOptions,
    resolver: impl Fn(&str) -> Option<ImageSource> + 'static,
    cx: &mut TestAppContext,
) -> RenderedText {
    ensure_theme_initialized(cx);

    let (_, cx) = cx.add_window_view(|_, _| TestWindow);
    let markdown = cx
        .new(|cx| Markdown::new_with_options(markdown.to_string().into(), None, None, options, cx));
    cx.run_until_parked();
    let (rendered, _) = cx.draw(
        Default::default(),
        size(px(600.0), px(600.0)),
        |_window, _cx| {
            MarkdownElement::new(markdown, MarkdownStyle::default())
                .image_resolver(resolver)
                .code_block_renderer(CodeBlockRenderer::Default {
                    copy_button_visibility: CopyButtonVisibility::Hidden,
                    wrap_button_visibility: WrapButtonVisibility::Hidden,
                    border: false,
                })
        },
    );
    rendered.text
}

fn test_image(cx: &mut TestAppContext) -> Arc<RenderImage> {
    cx.update(|cx| {
        cx.svg_renderer()
            .render_single_frame(
                br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"></svg>"#,
                1.0,
            )
            .expect("test svg should render")
    })
}

fn nbsp(n: usize) -> String {
    "\u{00A0}".repeat(n)
}

fn has_code_block(markdown: &str) -> bool {
    let parsed_data = parse_markdown_with_options(markdown, false, false, false);
    parsed_data
        .events
        .iter()
        .any(|(_, event)| matches!(event, MarkdownEvent::Start(MarkdownTag::CodeBlock { .. })))
}

#[track_caller]
fn assert_mappings(rendered: &RenderedText, expected: Vec<Vec<(usize, usize)>>) {
    assert_eq!(rendered.lines.len(), expected.len(), "line count mismatch");
    for (line_ix, line_mappings) in expected.into_iter().enumerate() {
        let line = &rendered.lines[line_ix];

        assert!(
            line.source_mappings.windows(2).all(|mappings| {
                mappings[0].source_index < mappings[1].source_index
                    && mappings[0].rendered_index < mappings[1].rendered_index
            }),
            "line {} has duplicate mappings: {:?}",
            line_ix,
            line.source_mappings
        );

        for (rendered_ix, source_ix) in line_mappings {
            assert_eq!(
                line.source_index_for_rendered_index(rendered_ix),
                source_ix,
                "line {}, rendered_ix {}",
                line_ix,
                rendered_ix
            );

            assert_eq!(
                line.rendered_index_for_source_index(source_ix),
                rendered_ix,
                "line {}, source_ix {}",
                line_ix,
                source_ix
            );
        }
    }
}

#[path = "tests/code_blocks.rs"]
mod code_blocks;
#[path = "tests/images_breaks.rs"]
mod images_breaks;
#[path = "tests/links_context.rs"]
mod links_context;
#[path = "tests/mappings_bounds.rs"]
mod mappings_bounds;
#[path = "tests/preview_theme.rs"]
mod preview_theme;
#[path = "tests/selection.rs"]
mod selection;
#[path = "tests/table_escape.rs"]
mod table_escape;

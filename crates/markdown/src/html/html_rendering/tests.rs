use gpui::{TestAppContext, size};
use ui::prelude::*;

use crate::{
    CodeBlockRenderer, CopyButtonVisibility, Markdown, MarkdownElement, MarkdownOptions,
    MarkdownStyle, WrapButtonVisibility,
};

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

fn render_markdown_text(markdown: &str, cx: &mut TestAppContext) -> crate::RenderedText {
    struct TestWindow;

    impl Render for TestWindow {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    ensure_theme_initialized(cx);

    let (_, cx) = cx.add_window_view(|_, _| TestWindow);
    let markdown = cx.new(|cx| Markdown::new(markdown.to_string().into(), None, None, cx));
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

#[gpui::test]
fn test_html_block_rendering_smoke(cx: &mut TestAppContext) {
    let rendered = render_markdown_text(
        "<h1>Hello</h1><blockquote><p>world</p></blockquote><ul><li>item</li></ul>",
        cx,
    );

    let rendered_lines = rendered
        .lines
        .iter()
        .map(|line| line.layout.wrapped_text())
        .collect::<Vec<_>>();

    assert_eq!(
        rendered_lines.concat().replace('\n', ""),
        "<h1>Hello</h1><blockquote><p>world</p></blockquote><ul><li>item</li></ul>"
    );
}

#[gpui::test]
fn test_html_block_rendering_can_be_enabled(cx: &mut TestAppContext) {
    struct TestWindow;

    impl Render for TestWindow {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    ensure_theme_initialized(cx);

    let (_, cx) = cx.add_window_view(|_, _| TestWindow);
    let markdown = cx.new(|cx| {
        Markdown::new_with_options(
            "<h1>Hello</h1><blockquote><p>world</p></blockquote><ul><li>item</li></ul>".into(),
            None,
            None,
            MarkdownOptions {
                parse_html: true,
                ..Default::default()
            },
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

    let rendered_lines = rendered
        .text
        .lines
        .iter()
        .map(|line| line.layout.wrapped_text())
        .collect::<Vec<_>>();

    assert_eq!(rendered_lines[0], "Hello");
    assert_eq!(rendered_lines[1], "world");
    assert!(rendered_lines.iter().any(|line| line.contains("item")));
}

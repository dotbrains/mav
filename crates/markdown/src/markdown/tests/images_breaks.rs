use super::*;

#[gpui::test]
fn test_soft_break_keeps_space_in_paragraph_with_image(cx: &mut TestAppContext) {
    let image = test_image(cx);
    let rendered = render_markdown_with_image_resolver(
        "Here is an image ![alt](https://example.com/a.png) and more text\nthat continues",
        MarkdownOptions::default(),
        move |_| Some(ImageSource::Render(image.clone())),
        cx,
    );
    let text: String = rendered
        .lines
        .iter()
        .map(|line| line.layout.wrapped_text())
        .collect();
    assert!(
        text.contains("more text that continues"),
        "soft break in an image paragraph should still separate words with a space; got: {text:?}"
    );
    assert!(
        !text.contains("textthat"),
        "soft break between words must not be dropped; got: {text:?}"
    );
}

#[gpui::test]
fn test_soft_break_after_image_does_not_insert_leading_space(cx: &mut TestAppContext) {
    let image = test_image(cx);
    let rendered = render_markdown_with_image_resolver(
        "![alt](https://example.com/a.png)\ncaption",
        MarkdownOptions::default(),
        move |_| Some(ImageSource::Render(image.clone())),
        cx,
    );
    let text: String = rendered
        .lines
        .iter()
        .map(|line| line.layout.wrapped_text())
        .collect();
    assert_eq!(
        text, "caption",
        "a soft break after an image should not insert leading whitespace before caption text; got: {text:?}"
    );
}

#[gpui::test]
fn test_break_between_images_does_not_inject_leading_space(cx: &mut TestAppContext) {
    let image = test_image(cx);
    let rendered = render_markdown_with_image_resolver(
        "![Image 3](https://example.com/3.png)\n<br>\n![Image 4](https://example.com/4.png)",
        MarkdownOptions {
            parse_html: true,
            ..Default::default()
        },
        move |_| Some(ImageSource::Render(image.clone())),
        cx,
    );
    let stray_whitespace_line = rendered.lines.iter().find_map(|line| {
        let text = line.layout.wrapped_text();
        (!text.is_empty() && text.trim().is_empty()).then_some(text)
    });
    assert!(
        stray_whitespace_line.is_none(),
        "soft break after a <br> between images must not render a stray space \
             before the wrapped image; got whitespace-only line: {stray_whitespace_line:?}"
    );
}

#[gpui::test]
fn test_break_in_tight_list_item_after_image_item_is_newline(cx: &mut TestAppContext) {
    let image = test_image(cx);
    let rendered = render_markdown_with_image_resolver(
        "- ![alt](https://example.com/a.png)\n- first<br>second",
        MarkdownOptions {
            parse_html: true,
            ..Default::default()
        },
        move |_| Some(ImageSource::Render(image.clone())),
        cx,
    );
    let text: String = rendered
        .lines
        .iter()
        .map(|line| line.layout.wrapped_text())
        .collect();
    assert!(
        text.contains("first\nsecond"),
        "break in a text-only tight list item should render as a newline; got: {text:?}"
    );
}

#[gpui::test]
fn test_hard_style_soft_break_after_image_moves_caption_to_next_row(cx: &mut TestAppContext) {
    ensure_theme_initialized(cx);

    let image = test_image(cx);
    let markdown_source = "![alt](https://example.com/a.png)\ncaption";
    let caption_range = markdown_source.len() - "caption".len()..markdown_source.len();

    let (_, cx) = cx.add_window_view(|_, _| TestWindow);
    let markdown = cx.new(|cx| {
        Markdown::new_with_options(
            markdown_source.to_string().into(),
            None,
            None,
            MarkdownOptions::default(),
            cx,
        )
    });
    cx.run_until_parked();

    let mut caption_top = |soft_break_as_hard_break: bool| {
        let mut style = MarkdownStyle::default();
        style.soft_break_as_hard_break = soft_break_as_hard_break;
        let image = image.clone();
        let (rendered, _) = cx.draw(
            Default::default(),
            size(px(600.0), px(600.0)),
            |_window, _cx| {
                MarkdownElement::new(markdown.clone(), style)
                    .image_resolver(move |_| Some(ImageSource::Render(image.clone())))
                    .code_block_renderer(CodeBlockRenderer::Default {
                        copy_button_visibility: CopyButtonVisibility::Hidden,
                        wrap_button_visibility: WrapButtonVisibility::Hidden,
                        border: false,
                    })
            },
        );
        rendered
            .text
            .bounds_for_source_range(caption_range.clone())
            .into_iter()
            .next()
            .expect("caption should have text bounds")
            .top()
    };

    let caption_top_with_break = caption_top(true);
    let caption_top_without_break = caption_top(false);

    assert!(
        caption_top_with_break > caption_top_without_break,
        "caption should render below the image for hard-style soft breaks; \
             top with break: {caption_top_with_break:?}, top without break: {caption_top_without_break:?}"
    );
}

#[gpui::test]
fn test_inline_br_renders_as_line_break(cx: &mut TestAppContext) {
    let options = MarkdownOptions {
        parse_html: true,
        ..Default::default()
    };

    for br in ["<br>", "<br/>", "<br />"] {
        let md = format!("first{br}second");
        let rendered = render_markdown_with_options(&md, None, options, cx);
        let text: String = rendered
            .lines
            .iter()
            .map(|line| line.layout.wrapped_text())
            .collect();
        assert!(
            !text.contains(br),
            "{br} should not appear as literal text; got: {text:?}"
        );
        assert!(
            text.contains("first\nsecond"),
            "{br} should produce a newline between 'first' and 'second'; got: {text:?}"
        );
    }
}

#[gpui::test]
fn test_hard_break_in_text_paragraph_after_paragraph(cx: &mut TestAppContext) {
    let options = MarkdownOptions {
        parse_html: true,
        ..Default::default()
    };
    let rendered = render_markdown_with_options("para one\n\nfirst<br>second", None, options, cx);
    let all_text: String = rendered
        .lines
        .iter()
        .map(|line| line.layout.wrapped_text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        all_text.contains("first") && all_text.contains("second"),
        "both sides of <br> should be present; got: {all_text:?}"
    );
    assert!(
        !all_text.contains("<br>"),
        "<br> should not appear as literal text; got: {all_text:?}"
    );
}

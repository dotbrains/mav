use super::*;

#[gpui::test]
fn test_link_detected_for_source_index(cx: &mut TestAppContext) {
    let rendered = render_markdown("[Click here](https://example.com)", cx);

    assert_eq!(rendered.links.len(), 1);
    assert_eq!(rendered.links[0].destination_url, "https://example.com");

    // Source index 1 ('C' in "Click") is inside the link's source range
    let link = rendered.link_for_source_index(1);
    assert!(link.is_some());
    assert_eq!(link.unwrap().destination_url, "https://example.com");

    // A source index past the end of the link range returns None
    let past_end = rendered.links[0].source_range.end;
    assert!(rendered.link_for_source_index(past_end).is_none());
}

#[gpui::test]
fn test_link_for_source_index_ignores_plain_text(cx: &mut TestAppContext) {
    let rendered = render_markdown("Hello world", cx);

    assert!(rendered.links.is_empty());
    assert!(rendered.link_for_source_index(0).is_none());
    assert!(rendered.link_for_source_index(5).is_none());
}

#[gpui::test]
fn test_code_span_link_detected_for_source_index(cx: &mut TestAppContext) {
    let source = "see `foo.rs` for details";
    let rendered = render_markdown_with_code_span_link(
        source,
        |text, _cx| (text == "foo.rs").then(|| "file:///tmp/foo.rs".into()),
        cx,
    );

    assert_eq!(rendered.links.len(), 1);
    assert_eq!(rendered.links[0].destination_url, "file:///tmp/foo.rs");

    let code_index = source.find("foo.rs").unwrap();
    let link = rendered.link_for_source_index(code_index);
    assert!(link.is_some());
    assert_eq!(link.unwrap().destination_url, "file:///tmp/foo.rs");

    assert!(
        rendered
            .link_for_source_index(source.find("see").unwrap())
            .is_none()
    );
}

#[gpui::test]
fn test_code_span_link_receives_decoded_inline_code(cx: &mut TestAppContext) {
    let source = r"| Pattern |
| --- |
| `a\|b` |";
    let rendered = render_markdown_with_code_span_link(
        source,
        |text, _cx| (text == "a|b").then(|| "file:///tmp/a-or-b".into()),
        cx,
    );

    assert_eq!(rendered.links.len(), 1);
    assert_eq!(rendered.links[0].destination_url, "file:///tmp/a-or-b");
}

#[gpui::test]
fn test_code_span_link_ignores_code_when_mouse_interaction_is_prevented(cx: &mut TestAppContext) {
    let callback_count = Arc::new(AtomicUsize::new(0));
    let rendered = render_markdown_with_code_span_link_style(
        "see `foo.rs` for details",
        MarkdownStyle {
            prevent_mouse_interaction: true,
            ..MarkdownStyle::default()
        },
        {
            let callback_count = callback_count.clone();
            move |text, _cx| {
                callback_count.fetch_add(1, Ordering::Relaxed);
                (text == "foo.rs").then(|| "file:///tmp/foo.rs".into())
            }
        },
        cx,
    );

    assert!(rendered.links.is_empty());
    assert_eq!(callback_count.load(Ordering::Relaxed), 0);
}

#[gpui::test]
fn test_code_span_link_ignores_code_without_callback(cx: &mut TestAppContext) {
    let rendered = render_markdown("see `foo.rs` for details", cx);

    assert!(rendered.links.is_empty());
}

#[gpui::test]
fn test_code_span_link_ignores_code_inside_markdown_link(cx: &mut TestAppContext) {
    let source = "see [`foo.rs`](https://example.com) for details";
    let rendered = render_markdown_with_code_span_link(
        source,
        |text, _cx| (text == "foo.rs").then(|| "file:///tmp/foo.rs".into()),
        cx,
    );

    assert_eq!(rendered.links.len(), 1);
    assert_eq!(rendered.links[0].destination_url, "https://example.com");
}

#[gpui::test]
fn test_context_menu_link_initial_state(cx: &mut TestAppContext) {
    ensure_theme_initialized(cx);
    let (_, cx) = cx.add_window_view(|_, _| TestWindow);
    let markdown =
        cx.new(|cx| Markdown::new("Hello [world](https://example.com)".into(), None, None, cx));
    cx.run_until_parked();

    cx.update(|_window, cx| {
        assert!(markdown.read(cx).context_menu_link().is_none());
    });
}

#[gpui::test]
fn test_capture_for_context_menu(cx: &mut TestAppContext) {
    ensure_theme_initialized(cx);
    let (_, cx) = cx.add_window_view(|_, _| TestWindow);
    let markdown = cx.new(|cx| Markdown::new("some text".into(), None, None, cx));
    cx.run_until_parked();

    // Simulates right-clicking on a link, with "text" selected
    let url: SharedString = "https://example.com".into();
    markdown.update(cx, |md, _cx| {
        md.selection.start = 5;
        md.selection.end = 9;
        md.capture_for_context_menu(Some(url.clone()), None);
    });
    cx.update(|_window, cx| {
        let markdown = markdown.read(cx);
        assert_eq!(
            markdown.context_menu_link().map(SharedString::as_ref),
            Some("https://example.com")
        );
        assert_eq!(
            markdown
                .context_menu_selected_markdown()
                .map(SharedString::as_ref),
            Some("text")
        );
        assert_eq!(
            markdown
                .context_menu_selected_text()
                .map(SharedString::as_ref),
            Some("text")
        );
    });

    // Simulates right-clicking on plain text with no selection — everything is cleared
    markdown.update(cx, |md, _cx| {
        md.selection.start = 0;
        md.selection.end = 0;
        md.capture_for_context_menu(None, None);
    });
    cx.update(|_window, cx| {
        let markdown = markdown.read(cx);
        assert!(markdown.context_menu_link().is_none());
        assert!(markdown.context_menu_selected_markdown().is_none());
        assert!(markdown.context_menu_selected_text().is_none());
    });
}

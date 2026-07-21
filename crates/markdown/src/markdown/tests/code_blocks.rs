use super::*;

#[gpui::test]
fn test_code_block_controls_are_unique_across_markdown_entities(cx: &mut TestAppContext) {
    struct TestWindow;

    impl Render for TestWindow {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    struct TestMarkdowns {
        first_markdown: Entity<Markdown>,
        second_markdown: Entity<Markdown>,
    }

    impl Render for TestMarkdowns {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .child(MarkdownElement::new(
                    self.first_markdown.clone(),
                    MarkdownStyle::default(),
                ))
                .child(MarkdownElement::new(
                    self.second_markdown.clone(),
                    MarkdownStyle::default(),
                ))
        }
    }

    ensure_theme_initialized(cx);

    let (_, cx) = cx.add_window_view(|_, _| TestWindow);
    let markdown = "```sh\necho hello\n```";
    let first_markdown = cx.new(|cx| Markdown::new(markdown.into(), None, None, cx));
    let second_markdown = cx.new(|cx| Markdown::new(markdown.into(), None, None, cx));
    cx.run_until_parked();

    cx.draw(Default::default(), size(px(600.0), px(600.0)), |_, cx| {
        cx.new(|_| TestMarkdowns {
            first_markdown: first_markdown.clone(),
            second_markdown: second_markdown.clone(),
        })
        .into_any_element()
    });
}

#[gpui::test]
fn test_active_search_highlight_uses_match_index(cx: &mut TestAppContext) {
    let markdown = cx.new(|cx| Markdown::new("zero one two".into(), None, None, cx));

    markdown.update(cx, |markdown, cx| {
        markdown.set_search_highlights(vec![0..4, 5..8, 9..12], Some(0), cx);
        assert_eq!(markdown.search_highlights(), &[0..4, 5..8, 9..12]);
        assert_eq!(markdown.active_search_highlight(), Some(0));

        markdown.set_active_search_highlight(Some(1), cx);
        assert_eq!(markdown.active_search_highlight(), Some(1));

        markdown.set_active_search_highlight(Some(2), cx);
        assert_eq!(markdown.active_search_highlight(), Some(2));

        markdown.set_active_search_highlight(Some(3), cx);
        assert_eq!(markdown.active_search_highlight(), None);
    });
}

#[gpui::test]
fn test_wrapped_code_block_has_no_scroll_handle(cx: &mut TestAppContext) {
    let markdown =
        cx.new(|cx| Markdown::new("```rust\nlet value = 1;\n```".into(), None, None, cx));

    markdown.update(cx, |markdown, _| {
        assert!(markdown.code_block_scroll_handle(0).is_some());

        markdown.toggle_code_block_wrap(0);
        assert!(markdown.code_block_scroll_handle(0).is_none());

        markdown.toggle_code_block_wrap(0);
        assert!(markdown.code_block_scroll_handle(0).is_some());
    });
}

#[gpui::test]
fn test_frontmatter_renders_without_delimiters(cx: &mut TestAppContext) {
    let rendered = render_markdown_with_options(
        "---\ntitle: Post\n---\nBody",
        None,
        MarkdownOptions {
            render_metadata_blocks: true,
            ..Default::default()
        },
        cx,
    );
    assert_eq!(rendered.text_for_range(0..24), "title\nPost\nBody");
}

#[gpui::test]
fn test_frontmatter_falls_back_to_code_block_for_nested_yaml(cx: &mut TestAppContext) {
    let rendered = render_markdown_with_options(
        "---\ntags:\n  - mav\n---\nBody",
        None,
        MarkdownOptions {
            render_metadata_blocks: true,
            ..Default::default()
        },
        cx,
    );
    assert_eq!(rendered.text_for_range(0..26), "tags:\n  - mav\nBody");
}

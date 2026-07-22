use super::*;

#[gpui::test]
fn test_hover_markdown_preserves_soft_breaks(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let cx = cx.add_empty_window();
    let text = concat!(
        "class super(object)\n",
        "|  super(type) -> unbound super object\n",
        "|  super(type, obj) -> bound super object"
    );
    let markdown = cx.new(|cx| Markdown::new(text.into(), None, None, cx));
    cx.run_until_parked();

    let rendered = MarkdownElement::rendered_text(markdown, cx, hover_markdown_style);

    // The two soft breaks must render as real newline characters rather
    // than being collapsed into spaces.
    assert_eq!(
        rendered.matches('\n').count(),
        2,
        "expected two hard line breaks, got {rendered:?}"
    );
    let lines: Vec<&str> = rendered.split('\n').collect();
    assert_eq!(
        lines,
        [
            "class super(object)",
            "|  super(type) -> unbound super object",
            "|  super(type, obj) -> bound super object",
        ]
    );
    // The two spaces after each `|` continuation marker are preserved verbatim.
    assert!(lines[1].starts_with("|  super"));
    assert!(lines[2].starts_with("|  super"));
    // No tabs are introduced anywhere in the rendered output.
    assert!(!rendered.contains('\t'));
    // And the full rendering matches the source exactly.
    assert_eq!(rendered, text);
}

use super::*;
use gpui::TestAppContext;
use text::{OffsetRangeExt, Point, Unclipped};

fn make_symbol(
    name: &str,
    kind: lsp::SymbolKind,
    range: std::ops::Range<(u32, u32)>,
    selection_range: std::ops::Range<(u32, u32)>,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    use text::PointUtf16;
    DocumentSymbol {
        name: name.to_string(),
        kind,
        range: Unclipped(PointUtf16::new(range.start.0, range.start.1))
            ..Unclipped(PointUtf16::new(range.end.0, range.end.1)),
        selection_range: Unclipped(PointUtf16::new(
            selection_range.start.0,
            selection_range.start.1,
        ))
            ..Unclipped(PointUtf16::new(
                selection_range.end.0,
                selection_range.end.1,
            )),
        children,
    }
}

#[gpui::test]
async fn test_flatten_document_symbols(cx: &mut TestAppContext) {
    let buffer = cx.new(|cx| {
        Buffer::local(
            concat!(
                "struct Foo {\n",
                "    bar: u32,\n",
                "    baz: String,\n",
                "}\n",
                "\n",
                "impl Foo {\n",
                "    fn new() -> Self {\n",
                "        Foo { bar: 0, baz: String::new() }\n",
                "    }\n",
                "}\n",
            ),
            cx,
        )
    });

    let symbols = vec![
        make_symbol(
            "Foo",
            lsp::SymbolKind::STRUCT,
            (0, 0)..(3, 1),
            (0, 7)..(0, 10),
            vec![
                make_symbol(
                    "bar",
                    lsp::SymbolKind::FIELD,
                    (1, 4)..(1, 13),
                    (1, 4)..(1, 7),
                    vec![],
                ),
                make_symbol(
                    "baz",
                    lsp::SymbolKind::FIELD,
                    (2, 4)..(2, 15),
                    (2, 4)..(2, 7),
                    vec![],
                ),
            ],
        ),
        make_symbol(
            "Foo",
            lsp::SymbolKind::STRUCT,
            (5, 0)..(9, 1),
            (5, 5)..(5, 8),
            vec![make_symbol(
                "new",
                lsp::SymbolKind::FUNCTION,
                (6, 4)..(8, 5),
                (6, 7)..(6, 10),
                vec![],
            )],
        ),
    ];

    let snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());

    let mut items = Vec::new();
    flatten_document_symbols(&symbols, &snapshot, 0, &mut items);

    assert_eq!(items.len(), 5);

    assert_eq!(items[0].depth, 0);
    assert_eq!(items[0].text, "struct Foo");
    assert_eq!(items[0].name_ranges, vec![7..10]);
    assert_eq!(
        items[0].selection_range.to_point(&snapshot),
        Point::new(0, 7)..Point::new(0, 10)
    );

    assert_eq!(items[1].depth, 1);
    assert_eq!(items[1].text, "bar");
    assert_eq!(items[1].name_ranges, vec![0..3]);
    assert_eq!(
        items[1].selection_range.to_point(&snapshot),
        Point::new(1, 4)..Point::new(1, 7)
    );

    assert_eq!(items[2].depth, 1);
    assert_eq!(items[2].text, "baz");
    assert_eq!(items[2].name_ranges, vec![0..3]);
    assert_eq!(
        items[2].selection_range.to_point(&snapshot),
        Point::new(2, 4)..Point::new(2, 7)
    );

    assert_eq!(items[3].depth, 0);
    assert_eq!(items[3].text, "impl Foo");
    assert_eq!(items[3].name_ranges, vec![5..8]);
    assert_eq!(
        items[3].selection_range.to_point(&snapshot),
        Point::new(5, 5)..Point::new(5, 8)
    );

    assert_eq!(items[4].depth, 1);
    assert_eq!(items[4].text, "fn new");
    assert_eq!(items[4].name_ranges, vec![3..6]);
    assert_eq!(
        items[4].selection_range.to_point(&snapshot),
        Point::new(6, 7)..Point::new(6, 10)
    );
}

#[gpui::test]
async fn test_empty_symbols(cx: &mut TestAppContext) {
    let buffer = cx.new(|cx| Buffer::local("", cx));
    let snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());

    let symbols: Vec<DocumentSymbol> = Vec::new();
    let mut items = Vec::new();
    flatten_document_symbols(&symbols, &snapshot, 0, &mut items);
    assert!(items.is_empty());
}

#[gpui::test]
async fn test_newlines_collapsed_in_name(cx: &mut TestAppContext) {
    let buffer = cx.new(|cx| Buffer::local("x = 1\ny = 2\n", cx));

    let symbols = vec![
        make_symbol(
            "line1\nline2",
            lsp::SymbolKind::VARIABLE,
            (0, 0)..(0, 5),
            (0, 0)..(0, 1),
            vec![],
        ),
        make_symbol(
            "  a  \n  b  ",
            lsp::SymbolKind::VARIABLE,
            (1, 0)..(1, 5),
            (1, 0)..(1, 1),
            vec![],
        ),
        make_symbol(
            "a\r\nb",
            lsp::SymbolKind::VARIABLE,
            (0, 0)..(1, 5),
            (0, 0)..(0, 1),
            vec![],
        ),
        make_symbol(
            "a\n\nb",
            lsp::SymbolKind::VARIABLE,
            (0, 0)..(1, 5),
            (0, 0)..(0, 1),
            vec![],
        ),
    ];

    let snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());
    let mut items = Vec::new();
    flatten_document_symbols(&symbols, &snapshot, 0, &mut items);

    assert_eq!(items.len(), 4);
    assert_eq!(items[0].text, "line1 line2");
    assert_eq!(items[1].text, "a b");
    assert_eq!(items[2].text, "a b");
    assert_eq!(items[3].text, "a b");
}

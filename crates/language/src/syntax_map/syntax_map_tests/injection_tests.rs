use super::*;

#[gpui::test]
fn test_dynamic_language_injection(cx: &mut App) {
    let registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
    let markdown = markdown_lang();
    let markdown_inline = Arc::new(markdown_inline_lang());
    registry.add(markdown.clone());
    registry.add(markdown_inline.clone());
    registry.add(rust_lang());
    registry.add(Arc::new(ruby_lang()));

    let mut buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        r#"
            This is a code block:

            ```rs
            fn foo() {}
            ```
        "#
        .unindent(),
    );

    let mut syntax_map = SyntaxMap::new(&buffer);
    syntax_map.set_language_registry(registry.clone());
    syntax_map.reparse(markdown.clone(), &buffer);
    syntax_map.reparse(markdown_inline.clone(), &buffer);
    assert_layers_for_range(
        &syntax_map,
        &buffer,
        Point::new(3, 0)..Point::new(3, 0),
        &[
            "(document (section (paragraph (inline)) (fenced_code_block (fenced_code_block_delimiter) (info_string (language)) (block_continuation) (code_fence_content (block_continuation)) (fenced_code_block_delimiter))))",
            "(inline (code_span (code_span_delimiter) (code_span_delimiter)))",
            "...(function_item name: (identifier) parameters: (parameters) body: (block)...",
        ],
    );

    // Replace `rs` with a path to ending in `.rb` in code block.
    let macro_name_range = range_for_text(&buffer, "rs");
    buffer.edit([(macro_name_range, "foo/bar/baz.rb")]);
    syntax_map.interpolate(&buffer);
    syntax_map.reparse(markdown.clone(), &buffer);
    syntax_map.reparse(markdown_inline.clone(), &buffer);
    assert_layers_for_range(
        &syntax_map,
        &buffer,
        Point::new(3, 0)..Point::new(3, 0),
        &[
            "(document (section (paragraph (inline)) (fenced_code_block (fenced_code_block_delimiter) (info_string (language)) (block_continuation) (code_fence_content (block_continuation)) (fenced_code_block_delimiter))))",
            "(inline (code_span (code_span_delimiter) (code_span_delimiter)))",
            "...(call method: (identifier) arguments: (argument_list (call method: (identifier) arguments: (argument_list) block: (block)...",
        ],
    );

    // Replace Ruby with a language that hasn't been loaded yet.
    let macro_name_range = range_for_text(&buffer, "foo/bar/baz.rb");
    buffer.edit([(macro_name_range, "html")]);
    syntax_map.interpolate(&buffer);
    syntax_map.reparse(markdown.clone(), &buffer);
    syntax_map.reparse(markdown_inline.clone(), &buffer);
    assert_layers_for_range(
        &syntax_map,
        &buffer,
        Point::new(3, 0)..Point::new(3, 0),
        &[
            "(document (section (paragraph (inline)) (fenced_code_block (fenced_code_block_delimiter) (info_string (language)) (block_continuation) (code_fence_content (block_continuation)) (fenced_code_block_delimiter))))",
            "(inline (code_span (code_span_delimiter) (code_span_delimiter)))",
        ],
    );
    assert!(syntax_map.contains_unknown_injections());

    registry.add(Arc::new(html_lang()));
    syntax_map.reparse(markdown, &buffer);
    syntax_map.reparse(markdown_inline, &buffer);
    assert_layers_for_range(
        &syntax_map,
        &buffer,
        Point::new(3, 0)..Point::new(3, 0),
        &[
            "(document (section (paragraph (inline)) (fenced_code_block (fenced_code_block_delimiter) (info_string (language)) (block_continuation) (code_fence_content (block_continuation)) (fenced_code_block_delimiter))))",
            "(inline (code_span (code_span_delimiter) (code_span_delimiter)))",
            "(document (text))",
        ],
    );
    assert!(!syntax_map.contains_unknown_injections());
}

#[gpui::test]
fn test_rust_json_macro_empty_string_highlighting(cx: &mut App) {
    let registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
    let language = rust_lang();
    registry.add(language.clone());

    let buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        r#"
            serde_json::json!({
                "email": "",
                "password": "password123",
                "requires2FA": false
            })
        "#
        .unindent(),
    );

    let mut syntax_map = SyntaxMap::new(&buffer);
    syntax_map.set_language_registry(registry);
    syntax_map.reparse(language, &buffer);

    assert_capture_ranges(
        &syntax_map,
        &buffer,
        &["string"],
        r#"
            serde_json::json!({
                «"email"»: «""»,
                «"password"»: «"password123"»,
                «"requires2FA"»: false
            })
        "#,
    );

    assert_capture_ranges(
        &syntax_map,
        &buffer,
        &["boolean"],
        r#"
            serde_json::json!({
                "email": "",
                "password": "password123",
                "requires2FA": «false»
            })
        "#,
    );
}

#[gpui::test]
fn test_typing_multiple_new_injections(cx: &mut App) {
    let (buffer, syntax_map) = test_edit_sequence(
        "Rust",
        &[
            "fn a() { test_macro }",
            "fn a() { test_macro«!» }",
            "fn a() { test_macro!«()» }",
            "fn a() { test_macro!(«b») }",
            "fn a() { test_macro!(b«.») }",
            "fn a() { test_macro!(b.«c») }",
            "fn a() { test_macro!(b.c«()») }",
            "fn a() { test_macro!(b.c(«vec»)) }",
            "fn a() { test_macro!(b.c(vec«!»)) }",
            "fn a() { test_macro!(b.c(vec!«[]»)) }",
            "fn a() { test_macro!(b.c(vec![«d»])) }",
            "fn a() { test_macro!(b.c(vec![d«.»])) }",
            "fn a() { test_macro!(b.c(vec![d.«e»])) }",
        ],
        cx,
    );

    assert_capture_ranges(
        &syntax_map,
        &buffer,
        &["property"],
        "fn a() { test_macro!(b.«c»(vec![d.«e»])) }",
    );
}

#[gpui::test]
fn test_pasting_new_injection_line_between_others(cx: &mut App) {
    let (buffer, syntax_map) = test_edit_sequence(
        "Rust",
        &[
            "
                fn a() {
                    b!(B {});
                    c!(C {});
                    d!(D {});
                    e!(E {});
                    f!(F {});
                    g!(G {});
                }
            ",
            "
                fn a() {
                    b!(B {});
                    c!(C {});
                    d!(D {});
                «    h!(H {});
                »    e!(E {});
                    f!(F {});
                    g!(G {});
                }
            ",
        ],
        cx,
    );

    assert_capture_ranges(
        &syntax_map,
        &buffer,
        &["type"],
        "
        fn a() {
            b!(«B» {});
            c!(«C» {});
            d!(«D» {});
            h!(«H» {});
            e!(«E» {});
            f!(«F» {});
            g!(«G» {});
        }
        ",
    );
}

#[gpui::test]
fn test_joining_injections_with_child_injections(cx: &mut App) {
    let (buffer, syntax_map) = test_edit_sequence(
        "Rust",
        &[
            "
                fn a() {
                    b!(
                        c![one.two.three],
                        d![four.five.six],
                    );
                    e!(
                        f![seven.eight],
                    );
                }
            ",
            "
                fn a() {
                    b!(
                        c![one.two.three],
                        d![four.five.six],
                    ˇ    f![seven.eight],
                    );
                }
            ",
        ],
        cx,
    );

    assert_capture_ranges(
        &syntax_map,
        &buffer,
        &["property"],
        "
        fn a() {
            b!(
                c![one.«two».«three»],
                d![four.«five».«six»],
                f![seven.«eight»],
            );
        }
        ",
    );
}

#[gpui::test]
fn test_editing_edges_of_injection(cx: &mut App) {
    test_edit_sequence(
        "Rust",
        &[
            "
                fn a() {
                    b!(c!())
                }
            ",
            "
                fn a() {
                    «d»!(c!())
                }
            ",
            "
                fn a() {
                    «e»d!(c!())
                }
            ",
            "
                fn a() {
                    ed!«[»c!()«]»
                }
            ",
        ],
        cx,
    );
}

#[gpui::test]
fn test_edits_preceding_and_intersecting_injection(cx: &mut App) {
    test_edit_sequence(
        "Rust",
        &[
            //
            "const aaaaaaaaaaaa: B = c!(d(e.f));",
            "const aˇa: B = c!(d(eˇ));",
        ],
        cx,
    );
}

#[gpui::test]
fn test_non_local_changes_create_injections(cx: &mut App) {
    test_edit_sequence(
        "Rust",
        &[
            "
                // a! {
                    static B: C = d;
                // }
            ",
            "
                ˇa! {
                    static B: C = d;
                ˇ}
            ",
        ],
        cx,
    );
}

#[gpui::test]
fn test_creating_many_injections_in_one_edit(cx: &mut App) {
    test_edit_sequence(
        "Rust",
        &[
            "
                fn a() {
                    one(Two::three(3));
                    four(Five::six(6));
                    seven(Eight::nine(9));
                }
            ",
            "
                fn a() {
                    one«!»(Two::three(3));
                    four«!»(Five::six(6));
                    seven«!»(Eight::nine(9));
                }
            ",
            "
                fn a() {
                    one!(Two::three«!»(3));
                    four!(Five::six«!»(6));
                    seven!(Eight::nine«!»(9));
                }
            ",
        ],
        cx,
    );
}

#[gpui::test]
fn test_editing_across_injection_boundary(cx: &mut App) {
    test_edit_sequence(
        "Rust",
        &[
            "
                fn one() {
                    two();
                    three!(
                        three.four,
                        five.six,
                    );
                }
            ",
            "
                fn one() {
                    two();
                    th«irty_five![»
                        three.four,
                        five.six,
                    «   seven.eight,
                    ];»
                }
            ",
        ],
        cx,
    );
}

#[gpui::test]
fn test_removing_injection_by_replacing_across_boundary(cx: &mut App) {
    test_edit_sequence(
        "Rust",
        &[
            "
                fn one() {
                    two!(
                        three.four,
                    );
                }
            ",
            "
                fn one() {
                    t«en
                        .eleven(
                        twelve,
                    »
                        three.four,
                    );
                }
            ",
        ],
        cx,
    );
}

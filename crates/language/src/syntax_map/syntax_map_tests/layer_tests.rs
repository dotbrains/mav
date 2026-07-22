use super::*;

#[gpui::test]
fn test_syntax_map_layers_for_range(cx: &mut App) {
    let registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
    let language = rust_lang();
    registry.add(language.clone());

    let mut buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        r#"
            fn a() {
                assert_eq!(
                    b(vec![C {}]),
                    vec![d.e],
                );
                println!("{}", f(|_| true));
            }
        "#
        .unindent(),
    );

    let mut syntax_map = SyntaxMap::new(&buffer);
    syntax_map.set_language_registry(registry);
    syntax_map.reparse(language.clone(), &buffer);

    assert_layers_for_range(
        &syntax_map,
        &buffer,
        Point::new(2, 0)..Point::new(2, 0),
        &[
            "...(function_item ... (block (expression_statement (macro_invocation...",
            "...(tuple_expression (call_expression ... arguments: (arguments (macro_invocation...",
        ],
    );
    assert_layers_for_range(
        &syntax_map,
        &buffer,
        Point::new(2, 14)..Point::new(2, 16),
        &[
            "...(function_item ...",
            "...(tuple_expression (call_expression ... arguments: (arguments (macro_invocation...",
            "...(array_expression (struct_expression ...",
        ],
    );
    assert_layers_for_range(
        &syntax_map,
        &buffer,
        Point::new(3, 14)..Point::new(3, 16),
        &[
            "...(function_item ...",
            "...(tuple_expression (call_expression ... arguments: (arguments (macro_invocation...",
            "...(array_expression (field_expression ...",
        ],
    );
    assert_layers_for_range(
        &syntax_map,
        &buffer,
        Point::new(5, 12)..Point::new(5, 16),
        &[
            "...(function_item ...",
            "...(call_expression ... (arguments (closure_expression ...",
        ],
    );

    // Replace a vec! macro invocation with a plain slice, removing a syntactic layer.
    let macro_name_range = range_for_text(&buffer, "vec!");
    buffer.edit([(macro_name_range, "&")]);
    syntax_map.interpolate(&buffer);
    syntax_map.reparse(language.clone(), &buffer);

    assert_layers_for_range(
        &syntax_map,
        &buffer,
        Point::new(2, 14)..Point::new(2, 16),
        &[
            "...(function_item ...",
            "...(tuple_expression (call_expression ... arguments: (arguments (reference_expression value: (array_expression...",
        ],
    );

    // Put the vec! macro back, adding back the syntactic layer.
    buffer.undo();
    syntax_map.interpolate(&buffer);
    syntax_map.reparse(language, &buffer);

    assert_layers_for_range(
        &syntax_map,
        &buffer,
        Point::new(2, 14)..Point::new(2, 16),
        &[
            "...(function_item ...",
            "...(tuple_expression (call_expression ... arguments: (arguments (macro_invocation...",
            "...(array_expression (struct_expression ...",
        ],
    );
}

#[gpui::test]
fn test_syntax_map_languages_match_layers_for_range(cx: &mut App) {
    let registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
    let markdown = markdown_lang();
    let markdown_inline = Arc::new(markdown_inline_lang());
    registry.add(markdown.clone());
    registry.add(markdown_inline);
    registry.add(rust_lang());

    let buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        r#"
            This is `inline`.

            ```rs
            fn a() {}
            ```
        "#
        .unindent(),
    );

    let mut syntax_map = SyntaxMap::new(&buffer);
    syntax_map.set_language_registry(registry);
    syntax_map.reparse(markdown, &buffer);

    let all_language_names = syntax_map
        .languages(&buffer, true)
        .map(|language| language.name().to_string())
        .collect::<Vec<_>>();
    let all_layer_language_names = syntax_map
        .layers_for_range(0..buffer.len(), &buffer, true)
        .map(|layer| layer.language.name().to_string())
        .collect::<Vec<_>>();

    assert_eq!(all_language_names, all_layer_language_names);
    assert!(
        all_language_names
            .iter()
            .any(|language_name| language_name == "Markdown-Inline"),
        "expected hidden languages to be included when include_hidden is true"
    );
    assert!(
        all_language_names
            .iter()
            .any(|language_name| language_name == "Markdown")
    );
    assert!(
        all_language_names
            .iter()
            .any(|language_name| language_name == "Rust")
    );

    let visible_language_names = syntax_map
        .languages(&buffer, false)
        .map(|language| language.name().to_string())
        .collect::<Vec<_>>();
    let visible_layer_language_names = syntax_map
        .layers_for_range(0..buffer.len(), &buffer, false)
        .map(|layer| layer.language.name().to_string())
        .collect::<Vec<_>>();

    assert_eq!(visible_language_names, visible_layer_language_names);
    assert!(
        !visible_language_names
            .iter()
            .any(|language_name| language_name == "Markdown-Inline"),
        "expected hidden languages to be excluded when include_hidden is false"
    );
    assert!(
        visible_language_names
            .iter()
            .any(|language_name| language_name == "Markdown")
    );
    assert!(
        visible_language_names
            .iter()
            .any(|language_name| language_name == "Rust")
    );
}

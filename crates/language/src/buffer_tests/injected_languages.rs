use super::*;

#[gpui::test]
fn test_language_scope_at_with_combined_injections(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let text = r#"
            <ol>
            <% people.each do |person| %>
                <li>
                    <%= person.name %>
                </li>
            <% end %>
            </ol>
        "#
        .unindent();

        let language_registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
        language_registry.add(Arc::new(ruby_lang()));
        language_registry.add(Arc::new(html_lang()));
        language_registry.add(Arc::new(erb_lang()));

        let mut buffer = Buffer::local(text, cx);
        buffer.set_language_registry(language_registry.clone());
        let language = language_registry
            .language_for_name("HTML+ERB")
            .now_or_never()
            .and_then(Result::ok);
        buffer.set_language(language, cx);

        let snapshot = buffer.snapshot();
        let html_config = snapshot.language_scope_at(Point::new(2, 4)).unwrap();
        assert_eq!(html_config.line_comment_prefixes(), &[]);
        assert_eq!(
            html_config.block_comment(),
            Some(&BlockCommentConfig {
                start: "<!--".into(),
                end: "-->".into(),
                prefix: "".into(),
                tab_size: 0,
            })
        );

        let ruby_config = snapshot.language_scope_at(Point::new(3, 12)).unwrap();
        assert_eq!(ruby_config.line_comment_prefixes(), &[Arc::from("# ")]);
        assert_eq!(ruby_config.block_comment(), None);

        buffer
    });
}

#[gpui::test]
fn test_language_at_with_hidden_languages(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let text = r#"
            this is an *emphasized* word.
        "#
        .unindent();

        let language_registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
        language_registry.add(markdown_lang());
        language_registry.add(Arc::new(markdown_inline_lang()));

        let mut buffer = Buffer::local(text, cx);
        buffer.set_language_registry(language_registry.clone());
        buffer.set_language(
            language_registry
                .language_for_name("Markdown")
                .now_or_never()
                .unwrap()
                .ok(),
            cx,
        );

        let snapshot = buffer.snapshot();

        for point in [Point::new(0, 4), Point::new(0, 16)] {
            let config = snapshot.language_scope_at(point).unwrap();
            assert_eq!(config.language_name(), "Markdown");

            let language = snapshot.language_at(point).unwrap();
            assert_eq!(language.name().as_ref(), "Markdown");
        }

        buffer
    });
}

#[gpui::test]
fn test_language_at_for_markdown_code_block(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let text = r#"
            ```rs
            let a = 2;
            // let b = 3;
            ```
        "#
        .unindent();

        let language_registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
        language_registry.add(markdown_lang());
        language_registry.add(Arc::new(markdown_inline_lang()));
        language_registry.add(rust_lang());

        let mut buffer = Buffer::local(text, cx);
        buffer.set_language_registry(language_registry.clone());
        buffer.set_language(
            language_registry
                .language_for_name("Markdown")
                .now_or_never()
                .unwrap()
                .ok(),
            cx,
        );

        let snapshot = buffer.snapshot();

        // Test points in the code line
        for point in [Point::new(1, 4), Point::new(1, 6)] {
            let config = snapshot.language_scope_at(point).unwrap();
            assert_eq!(config.language_name(), "Rust");

            let language = snapshot.language_at(point).unwrap();
            assert_eq!(language.name().as_ref(), "Rust");
        }

        // Test points in the comment line to verify it's still detected as Rust
        for point in [Point::new(2, 4), Point::new(2, 6)] {
            let config = snapshot.language_scope_at(point).unwrap();
            assert_eq!(config.language_name(), "Rust");

            let language = snapshot.language_at(point).unwrap();
            assert_eq!(language.name().as_ref(), "Rust");
        }

        buffer
    });
}

#[gpui::test]
fn test_syntax_layer_at_for_combined_injections(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        // ERB template with HTML and Ruby content
        let text = r#"
<div>Hello</div>
<%= link_to "Click", url %>
<p>World</p>
        "#
        .unindent();

        let language_registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
        language_registry.add(Arc::new(erb_lang()));
        language_registry.add(Arc::new(html_lang()));
        language_registry.add(Arc::new(ruby_lang()));

        let mut buffer = Buffer::local(text, cx);
        buffer.set_language_registry(language_registry.clone());
        let language = language_registry
            .language_for_name("HTML+ERB")
            .now_or_never()
            .and_then(Result::ok);
        buffer.set_language(language, cx);

        let snapshot = buffer.snapshot();

        // Test language_at for HTML content (line 0: "<div>Hello</div>")
        let html_point = Point::new(0, 4);
        let language = snapshot.language_at(html_point).unwrap();
        assert_eq!(
            language.name().as_ref(),
            "HTML",
            "Expected HTML at {:?}, got {}",
            html_point,
            language.name()
        );

        // Test language_at for Ruby code (line 1: "<%= link_to ... %>")
        let ruby_point = Point::new(1, 6);
        let language = snapshot.language_at(ruby_point).unwrap();
        assert_eq!(
            language.name().as_ref(),
            "Ruby",
            "Expected Ruby at {:?}, got {}",
            ruby_point,
            language.name()
        );

        // Test language_at for HTML after Ruby (line 2: "<p>World</p>")
        let html_after_ruby = Point::new(2, 2);
        let language = snapshot.language_at(html_after_ruby).unwrap();
        assert_eq!(
            language.name().as_ref(),
            "HTML",
            "Expected HTML at {:?}, got {}",
            html_after_ruby,
            language.name()
        );

        buffer
    });
}

#[gpui::test]
fn test_languages_at_for_combined_injections(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        // ERB template with HTML and Ruby content
        let text = r#"
<div>Hello</div>
<%= yield %>
<p>World</p>
        "#
        .unindent();

        let language_registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
        language_registry.add(Arc::new(erb_lang()));
        language_registry.add(Arc::new(html_lang()));
        language_registry.add(Arc::new(ruby_lang()));

        let mut buffer = Buffer::local(text, cx);
        buffer.set_language_registry(language_registry.clone());
        buffer.set_language(
            language_registry
                .language_for_name("HTML+ERB")
                .now_or_never()
                .unwrap()
                .ok(),
            cx,
        );

        // Test languages_at for HTML content - should NOT include Ruby
        let html_point = Point::new(0, 4);
        let languages = buffer.languages_at(html_point);
        let language_names: Vec<_> = languages.iter().map(|language| language.name()).collect();
        assert!(
            language_names
                .iter()
                .any(|language_name| language_name.as_ref() == "HTML"),
            "Expected HTML in languages at {:?}, got {:?}",
            html_point,
            language_names
        );
        assert!(
            !language_names
                .iter()
                .any(|language_name| language_name.as_ref() == "Ruby"),
            "Did not expect Ruby in languages at {:?}, got {:?}",
            html_point,
            language_names
        );

        // Test languages_at for Ruby code - should NOT include HTML
        let ruby_point = Point::new(1, 6);
        let languages = buffer.languages_at(ruby_point);
        let language_names: Vec<_> = languages.iter().map(|language| language.name()).collect();
        assert!(
            language_names
                .iter()
                .any(|language_name| language_name.as_ref() == "Ruby"),
            "Expected Ruby in languages at {:?}, got {:?}",
            ruby_point,
            language_names
        );
        assert!(
            !language_names
                .iter()
                .any(|language_name| language_name.as_ref() == "HTML"),
            "Did not expect HTML in languages at {:?}, got {:?}",
            ruby_point,
            language_names
        );

        buffer
    });
}

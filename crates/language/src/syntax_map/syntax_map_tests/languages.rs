use super::*;

fn html_lang() -> Language {
    Language::new(
        LanguageConfig {
            name: "HTML".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["html".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_html::LANGUAGE.into()),
    )
    .with_highlights_query(
        r#"
            (tag_name) @tag
            (erroneous_end_tag_name) @tag
            (attribute_name) @property
        "#,
    )
    .unwrap()
}

fn ruby_lang() -> Language {
    Language::new(
        LanguageConfig {
            name: "Ruby".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rb".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_ruby::LANGUAGE.into()),
    )
    .with_highlights_query(
        r#"
            ["if" "do" "else" "end"] @keyword
            (instance_variable) @ivar
            (call method: (identifier) @method)
        "#,
    )
    .unwrap()
}

fn erb_lang() -> Language {
    Language::new(
        LanguageConfig {
            name: "ERB".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["erb".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_embedded_template::LANGUAGE.into()),
    )
    .with_highlights_query(
        r#"
            ["<%" "%>"] @keyword
        "#,
    )
    .unwrap()
    .with_injection_query(
        r#"
            (
                (code) @injection.content
                (#set! injection.language "ruby")
                (#set! injection.combined)
            )

            (
                (content) @injection.content
                (#set! injection.language "html")
                (#set! injection.combined)
            )
        "#,
    )
    .unwrap()
}

fn elixir_lang() -> Language {
    Language::new(
        LanguageConfig {
            name: "Elixir".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ex".into()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_elixir::LANGUAGE.into()),
    )
    .with_highlights_query(
        r#"

        "#,
    )
    .unwrap()
}

fn heex_lang() -> Language {
    Language::new(
        LanguageConfig {
            name: "HEEx".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["heex".into()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_heex::LANGUAGE.into()),
    )
    .with_injection_query(
        r#"
        (
          (directive
            [
              (partial_expression_value)
              (expression_value)
              (ending_expression_value)
            ] @injection.content)
          (#set! injection.language "elixir")
          (#set! injection.combined)
        )

        ((expression (expression_value) @injection.content)
         (#set! injection.language "elixir"))
        "#,
    )
    .unwrap()
}

fn python_lang() -> Language {
    Language::new(
        LanguageConfig {
            name: "Python".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["py".to_string()],
                ..Default::default()
            },
            line_comments: vec!["# ".into()],
            ..Default::default()
        },
        Some(tree_sitter_python::LANGUAGE.into()),
    )
    .with_queries(LanguageQueries {
        injections: Some(Cow::from(include_str!(
            "../../../grammars/src/python/injections.scm"
        ))),
        ..Default::default()
    })
    .expect("Could not parse Python queries")
}

fn comment_lang() -> Language {
    // Mock "comment" language to satisfy Python's comment injection.
    // Uses JSON grammar as a stand-in since we just need it to be registered.
    Language::new(
        LanguageConfig {
            name: "comment".into(),
            ..Default::default()
        },
        Some(tree_sitter_json::LANGUAGE.into()),
    )
}

fn range_for_text(buffer: &Buffer, text: &str) -> Range<usize> {
    let start = buffer.as_rope().to_string().find(text).unwrap();
    start..start + text.len()
}

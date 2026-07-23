use std::borrow::Cow;

use collections::HashSet;
use indoc::indoc;
use language::{BlockCommentConfig, Language, LanguageConfig, LanguageMatcher, LanguageQueries};

fn word_characters() -> HashSet<char> {
    let mut word_characters: HashSet<char> = Default::default();
    word_characters.insert('$');
    word_characters.insert('#');
    word_characters
}

fn braces() -> language::BracketPairConfig {
    language::BracketPairConfig {
        pairs: vec![language::BracketPair {
            start: "{".to_string(),
            end: "}".to_string(),
            close: true,
            surround: true,
            newline: true,
        }],
        disabled_scopes_by_bracket_ix: Default::default(),
    }
}

fn common_ts_queries() -> LanguageQueries {
    LanguageQueries {
        indents: Some(Cow::from(indoc! {r#"
            [
                (call_expression)
                (assignment_expression)
                (member_expression)
                (lexical_declaration)
                (variable_declaration)
                (assignment_expression)
                (if_statement)
                (for_statement)
            ] @indent

            (_ "[" "]" @end) @indent
            (_ "<" ">" @end) @indent
            (_ "{" "}" @end) @indent
            (_ "(" ")" @end) @indent
            "#})),
        text_objects: Some(Cow::from(indoc! {r#"
            (function_declaration
                body: (_
                    "{"
                    (_)* @function.inside
                    "}")) @function.around

            (method_definition
                body: (_
                    "{"
                    (_)* @function.inside
                    "}")) @function.around

            ; Arrow function in variable declaration - capture the full declaration
            ([
                (lexical_declaration
                    (variable_declarator
                        value: (arrow_function
                            body: (statement_block
                                "{"
                                (_)* @function.inside
                                "}"))))
                (variable_declaration
                    (variable_declarator
                        value: (arrow_function
                            body: (statement_block
                                "{"
                                (_)* @function.inside
                                "}"))))
            ]) @function.around

            ([
                (lexical_declaration
                    (variable_declarator
                        value: (arrow_function)))
                (variable_declaration
                    (variable_declarator
                        value: (arrow_function)))
            ]) @function.around

            ; Catch-all for arrow functions in other contexts (callbacks, etc.)
            ((arrow_function) @function.around (#not-has-parent? @function.around variable_declarator))
            "#})),
        ..Default::default()
    }
}

pub(super) fn typescript() -> Language {
    Language::new(
        LanguageConfig {
            name: "Typescript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..Default::default()
            },
            brackets: braces(),
            word_characters: word_characters(),
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    )
    .with_queries(LanguageQueries {
        brackets: Some(Cow::from(indoc! {r#"
            ("(" @open ")" @close)
            ("[" @open "]" @close)
            ("{" @open "}" @close)
            ("<" @open ">" @close)
            ("'" @open "'" @close)
            ("`" @open "`" @close)
            ("\"" @open "\"" @close)"#})),
        ..common_ts_queries()
    })
    .expect("Could not parse queries")
}

pub(super) fn tsx() -> Language {
    Language::new(
        LanguageConfig {
            name: "TSX".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["tsx".to_string()],
                ..Default::default()
            },
            brackets: braces(),
            word_characters: word_characters(),
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
    )
    .with_queries(LanguageQueries {
        brackets: Some(Cow::from(indoc! {r#"
            ("(" @open ")" @close)
            ("[" @open "]" @close)
            ("{" @open "}" @close)
            ("<" @open ">" @close)
            ("<" @open "/>" @close)
            ("</" @open ">" @close)
            ("\"" @open "\"" @close)
            ("'" @open "'" @close)
            ("`" @open "`" @close)
            ((jsx_element (jsx_opening_element) @open (jsx_closing_element) @close) (#set! newline.only))"#})),
        indents: Some(Cow::from(indoc! {r#"
            [
                (call_expression)
                (assignment_expression)
                (member_expression)
                (lexical_declaration)
                (variable_declaration)
                (assignment_expression)
                (if_statement)
                (for_statement)
            ] @indent

            (_ "[" "]" @end) @indent
            (_ "<" ">" @end) @indent
            (_ "{" "}" @end) @indent
            (_ "(" ")" @end) @indent

            (jsx_opening_element ">" @end) @indent

            (jsx_element
              (jsx_opening_element) @start
              (jsx_closing_element)? @end) @indent
            "#})),
        ..common_ts_queries()
    })
    .expect("Could not parse queries")
}

pub(super) fn html() -> Language {
    Language::new(
        LanguageConfig {
            name: "HTML".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["html".into()],
                ..Default::default()
            },
            block_comment: Some(BlockCommentConfig {
                start: "<!--".into(),
                prefix: "".into(),
                end: "-->".into(),
                tab_size: 0,
            }),
            completion_query_characters: ['-'].into_iter().collect(),
            ..Default::default()
        },
        Some(tree_sitter_html::LANGUAGE.into()),
    )
    .with_queries(LanguageQueries {
        brackets: Some(Cow::from(indoc! {r#"
            ("<" @open "/>" @close)
            ("</" @open ">" @close)
            ("<" @open ">" @close)
            ("\"" @open "\"" @close)"#})),
        ..Default::default()
    })
    .expect("Could not parse queries")
}

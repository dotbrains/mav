use super::*;

#[doc(hidden)]
#[cfg(any(test, feature = "test-support"))]
pub fn rust_lang() -> Arc<Language> {
    use std::borrow::Cow;

    let language = Language::new(
        LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".to_string()],
                ..Default::default()
            },
            line_comments: vec!["// ".into(), "/// ".into(), "//! ".into()],
            brackets: BracketPairConfig {
                pairs: vec![
                    BracketPair {
                        start: "{".into(),
                        end: "}".into(),
                        close: true,
                        surround: false,
                        newline: true,
                    },
                    BracketPair {
                        start: "[".into(),
                        end: "]".into(),
                        close: true,
                        surround: false,
                        newline: true,
                    },
                    BracketPair {
                        start: "(".into(),
                        end: ")".into(),
                        close: true,
                        surround: false,
                        newline: true,
                    },
                    BracketPair {
                        start: "<".into(),
                        end: ">".into(),
                        close: false,
                        surround: false,
                        newline: true,
                    },
                    BracketPair {
                        start: "\"".into(),
                        end: "\"".into(),
                        close: true,
                        surround: false,
                        newline: false,
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    )
    .with_queries(LanguageQueries {
        outline: Some(Cow::from(include_str!(
            "../../grammars/src/rust/outline.scm"
        ))),
        indents: Some(Cow::from(include_str!(
            "../../grammars/src/rust/indents.scm"
        ))),
        brackets: Some(Cow::from(include_str!(
            "../../grammars/src/rust/brackets.scm"
        ))),
        text_objects: Some(Cow::from(include_str!(
            "../../grammars/src/rust/textobjects.scm"
        ))),
        highlights: Some(Cow::from(include_str!(
            "../../grammars/src/rust/highlights.scm"
        ))),
        injections: Some(Cow::from(include_str!(
            "../../grammars/src/rust/injections.scm"
        ))),
        overrides: Some(Cow::from(include_str!(
            "../../grammars/src/rust/overrides.scm"
        ))),
        redactions: None,
        runnables: Some(Cow::from(include_str!(
            "../../grammars/src/rust/runnables.scm"
        ))),
        debugger: Some(Cow::from(include_str!(
            "../../grammars/src/rust/debugger.scm"
        ))),
    })
    .expect("Could not parse queries");
    Arc::new(language)
}

#[doc(hidden)]
#[cfg(any(test, feature = "test-support"))]
pub fn markdown_lang() -> Arc<Language> {
    use std::borrow::Cow;

    let language = Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["md".into()],
                ..Default::default()
            },
            ..LanguageConfig::default()
        },
        Some(tree_sitter_md::LANGUAGE.into()),
    )
    .with_queries(LanguageQueries {
        brackets: Some(Cow::from(include_str!(
            "../../grammars/src/markdown/brackets.scm"
        ))),
        injections: Some(Cow::from(include_str!(
            "../../grammars/src/markdown/injections.scm"
        ))),
        highlights: Some(Cow::from(include_str!(
            "../../grammars/src/markdown/highlights.scm"
        ))),
        indents: Some(Cow::from(include_str!(
            "../../grammars/src/markdown/indents.scm"
        ))),
        outline: Some(Cow::from(include_str!(
            "../../grammars/src/markdown/outline.scm"
        ))),
        ..LanguageQueries::default()
    })
    .expect("Could not parse markdown queries");
    Arc::new(language)
}

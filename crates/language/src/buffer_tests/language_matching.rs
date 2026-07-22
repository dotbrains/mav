use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
fn test_select_language(cx: &mut App) {
    init_settings(cx, |_| {});

    let registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
    registry.add(Arc::new(Language::new(
        LanguageConfig {
            name: LanguageName::new_static("Rust"),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    )));
    registry.add(Arc::new(Language::new(
        LanguageConfig {
            name: "Rust with longer extension".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["longer.rs".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    )));
    registry.add(Arc::new(Language::new(
        LanguageConfig {
            name: LanguageName::new_static("Make"),
            matcher: LanguageMatcher {
                path_suffixes: vec!["Makefile".to_string(), "mk".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    )));

    // matching file extension
    assert_eq!(
        registry
            .language_for_file(&file("src/lib.rs"), None, cx)
            .map(|l| l.name()),
        Some("Rust".into())
    );
    assert_eq!(
        registry
            .language_for_file(&file("src/lib.mk"), None, cx)
            .map(|l| l.name()),
        Some("Make".into())
    );

    // matching longer, compound extension, part of which could also match another lang
    assert_eq!(
        registry
            .language_for_file(&file("src/lib.longer.rs"), None, cx)
            .map(|l| l.name()),
        Some("Rust with longer extension".into())
    );

    // matching filename
    assert_eq!(
        registry
            .language_for_file(&file("src/Makefile"), None, cx)
            .map(|l| l.name()),
        Some("Make".into())
    );

    // matching suffix that is not the full file extension or filename
    assert_eq!(
        registry
            .language_for_file(&file("mav/cars"), None, cx)
            .map(|l| l.name()),
        None
    );
    assert_eq!(
        registry
            .language_for_file(&file("mav/a.cars"), None, cx)
            .map(|l| l.name()),
        None
    );
    assert_eq!(
        registry
            .language_for_file(&file("mav/sumk"), None, cx)
            .map(|l| l.name()),
        None
    );
}

#[gpui::test]
fn test_mav_config_files_use_jsonc(cx: &mut App) {
    init_settings(cx, |_| {});

    let registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
    for config in [
        LanguageConfig {
            name: "JSON".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["json".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        LanguageConfig {
            name: "JSONC".into(),
            ..Default::default()
        },
    ] {
        registry.add(Arc::new(Language::new(config, None)));
    }

    for path in [
        ".config/mav/settings.json",
        ".config/Mav/settings.json",
        ".config/mav/keymap.json",
        "AppData/Roaming/Mav/tasks.json",
        "AppData/Roaming/Mav/debug.json",
    ] {
        assert_eq!(
            registry
                .language_for_file(&file(path), None, cx)
                .map(|language| language.name()),
            Some("JSONC".into()),
            "{path}"
        );
    }
}

#[gpui::test(iterations = 10)]
async fn test_first_line_pattern(cx: &mut TestAppContext) {
    cx.update(|cx| init_settings(cx, |_| {}));

    let languages = LanguageRegistry::test(cx.executor());
    let languages = Arc::new(languages);

    languages.register_test_language(LanguageConfig {
        name: "JavaScript".into(),
        matcher: LanguageMatcher {
            path_suffixes: vec!["js".into()],
            first_line_pattern: Some(Regex::new(r"\bnode\b").unwrap()),
            ..LanguageMatcher::default()
        },
        ..Default::default()
    });

    assert!(
        cx.read(|cx| languages.language_for_file(&file("the/script"), None, cx))
            .is_none()
    );
    assert!(
        cx.read(|cx| languages.language_for_file(&file("the/script"), Some(&"nothing".into()), cx))
            .is_none()
    );

    assert_eq!(
        cx.read(|cx| languages.language_for_file(
            &file("the/script"),
            Some(&"#!/bin/env node".into()),
            cx
        ))
        .unwrap()
        .name(),
        "JavaScript"
    );
}

#[gpui::test]
async fn test_language_for_file_with_custom_file_types(cx: &mut TestAppContext) {
    cx.update(|cx| {
        init_settings(cx, |settings| {
            settings.file_types.get_or_insert_default().extend([
                ("TypeScript".into(), vec!["js".into()].into()),
                (
                    "JavaScript".into(),
                    vec!["*longer.ts".into(), "ecmascript".into()].into(),
                ),
                ("C++".into(), vec!["c".into(), "*.dev".into()].into()),
                (
                    "Dockerfile".into(),
                    vec!["Dockerfile".into(), "Dockerfile.*".into()].into(),
                ),
            ]);
        })
    });

    let languages = Arc::new(LanguageRegistry::test(cx.executor()));

    for config in [
        LanguageConfig {
            name: "JavaScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["js".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string(), "ts.ecmascript".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        LanguageConfig {
            name: "C++".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["cpp".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        LanguageConfig {
            name: "C".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["c".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        LanguageConfig {
            name: "Dockerfile".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["Dockerfile".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
    ] {
        languages.add(Arc::new(Language::new(config, None)));
    }

    // matches system-provided lang extension
    let language = cx
        .read(|cx| languages.language_for_file(&file("foo.ts"), None, cx))
        .unwrap();
    assert_eq!(language.name(), "TypeScript");
    let language = cx
        .read(|cx| languages.language_for_file(&file("foo.ts.ecmascript"), None, cx))
        .unwrap();
    assert_eq!(language.name(), "TypeScript");
    let language = cx
        .read(|cx| languages.language_for_file(&file("foo.cpp"), None, cx))
        .unwrap();
    assert_eq!(language.name(), "C++");

    // user configured lang extension, same length as system-provided
    let language = cx
        .read(|cx| languages.language_for_file(&file("foo.js"), None, cx))
        .unwrap();
    assert_eq!(language.name(), "TypeScript");
    let language = cx
        .read(|cx| languages.language_for_file(&file("foo.c"), None, cx))
        .unwrap();
    assert_eq!(language.name(), "C++");

    // user configured lang extension, longer than system-provided
    let language = cx
        .read(|cx| languages.language_for_file(&file("foo.longer.ts"), None, cx))
        .unwrap();
    assert_eq!(language.name(), "JavaScript");

    // user configured lang extension, shorter than system-provided
    let language = cx
        .read(|cx| languages.language_for_file(&file("foo.ecmascript"), None, cx))
        .unwrap();
    assert_eq!(language.name(), "JavaScript");

    // user configured glob matches
    let language = cx
        .read(|cx| languages.language_for_file(&file("c-plus-plus.dev"), None, cx))
        .unwrap();
    assert_eq!(language.name(), "C++");
    // should match Dockerfile.* => Dockerfile, not *.dev => C++
    let language = cx
        .read(|cx| languages.language_for_file(&file("Dockerfile.dev"), None, cx))
        .unwrap();
    assert_eq!(language.name(), "Dockerfile");
}

fn file(path: &str) -> Arc<dyn File> {
    Arc::new(TestFile {
        path: Arc::from(rel_path(path)),
        root_name: "mav".into(),
        local_root: None,
    })
}

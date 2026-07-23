use super::*;

fn remote_sync_entry(id: &str, manifest_body: &str) -> ExtensionIndexEntry {
    let manifest = format!(
        r#"
        id = "{id}"
        name = "{id}"
        version = "1.0.0"
        schema_version = 0

        {manifest_body}
        "#
    );

    ExtensionIndexEntry {
        manifest: Arc::new(toml::from_str(&manifest).unwrap()),
        dev: false,
    }
}

fn remote_sync_language_entry(extension: &str, path: &str) -> ExtensionIndexLanguageEntry {
    ExtensionIndexLanguageEntry {
        extension: extension.into(),
        path: path.into(),
        matcher: LanguageMatcher::default(),
        hidden: false,
        grammar: None,
    }
}

fn remote_sync_extension_ids(index: &ExtensionIndex) -> Vec<String> {
    let mut extensions = index
        .extensions_to_sync_to_remote()
        .into_entries()
        .map(|(id, _)| id.to_string())
        .collect::<Vec<_>>();

    extensions.sort();

    extensions
}

#[test]
fn remote_sync_includes_language_dependencies() {
    let index = ExtensionIndex {
        extensions: [
            (
                "bar-language".into(),
                remote_sync_entry("bar-language", r#"languages = ["languages/bar"]"#),
            ),
            (
                "foo-lsp".into(),
                remote_sync_entry(
                    "foo-lsp",
                    r#"
                    [language_servers.foo]
                    language = "Foo"
                    "#,
                ),
            ),
            (
                "foo-language".into(),
                remote_sync_entry("foo-language", r#"languages = ["languages/foo"]"#),
            ),
        ]
        .into_iter()
        .collect(),
        languages: [
            (
                "Bar".into(),
                remote_sync_language_entry("bar-language", "languages/bar"),
            ),
            (
                "Foo".into(),
                remote_sync_language_entry("foo-language", "languages/foo"),
            ),
        ]
        .into_iter()
        .collect(),
        themes: BTreeMap::default(),
        icon_themes: BTreeMap::default(),
    };

    assert_eq!(
        remote_sync_extension_ids(&index),
        ["foo-language", "foo-lsp"]
    );
}

#[test]
fn remote_sync_keeps_shared_language_dependency_once() {
    let index = ExtensionIndex {
        extensions: [
            (
                "aaa-lsp".into(),
                remote_sync_entry(
                    "aaa-lsp",
                    r#"
                    [language_servers.aaa]
                    language = "Foo"
                    "#,
                ),
            ),
            (
                "bbb-lsp".into(),
                remote_sync_entry(
                    "bbb-lsp",
                    r#"
                    [language_servers.bbb]
                    language = "Foo"
                    "#,
                ),
            ),
            (
                "zzz-language".into(),
                remote_sync_entry("zzz-language", r#"languages = ["languages/foo"]"#),
            ),
        ]
        .into_iter()
        .collect(),
        languages: [(
            "Foo".into(),
            remote_sync_language_entry("zzz-language", "languages/foo"),
        )]
        .into_iter()
        .collect(),
        themes: BTreeMap::default(),
        icon_themes: BTreeMap::default(),
    };

    assert_eq!(
        remote_sync_extension_ids(&index),
        ["aaa-lsp", "bbb-lsp", "zzz-language"]
    );
}

#[test]
fn remote_sync_keeps_remote_loadable_extensions_without_language_dependency() {
    let index = ExtensionIndex {
        extensions: [(
            "foo".into(),
            remote_sync_entry(
                "foo",
                r#"
                [language_servers.foo]
                language = "Foo"
                "#,
            ),
        )]
        .into_iter()
        .collect(),
        languages: BTreeMap::default(),
        themes: BTreeMap::default(),
        icon_themes: BTreeMap::default(),
    };

    assert_eq!(remote_sync_extension_ids(&index), ["foo"]);
}

#[test]
fn remote_sync_keeps_debug_adapters() {
    let index = ExtensionIndex {
        extensions: [(
            "foo".into(),
            remote_sync_entry(
                "foo",
                r#"
                [debug_adapters.foo]
                "#,
            ),
        )]
        .into_iter()
        .collect(),
        languages: BTreeMap::default(),
        themes: BTreeMap::default(),
        icon_themes: BTreeMap::default(),
    };

    assert_eq!(remote_sync_extension_ids(&index), ["foo"]);
}

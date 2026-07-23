use super::test_support::*;
use super::*;
#[gpui::test]
async fn test_extension_store(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let http_client = FakeHttpClient::with_200_response();
    fs.insert_tree(
        "/the-extension-dir",
        json!({
            "installed": {
                "mav-monokai": {
                    "extension.json": r#"{
                        "id": "mav-monokai",
                        "name": "Mav Monokai",
                        "version": "2.0.0",
                        "themes": {
                            "Monokai Dark": "themes/monokai.json",
                            "Monokai Light": "themes/monokai.json",
                            "Monokai Pro Dark": "themes/monokai-pro.json",
                            "Monokai Pro Light": "themes/monokai-pro.json"
                        }
                    }"#,
                    "themes": {
                        "monokai.json": r#"{
                            "name": "Monokai",
                            "author": "Someone",
                            "themes": [
                                {
                                    "name": "Monokai Dark",
                                    "appearance": "dark",
                                    "style": {}
                                },
                                {
                                    "name": "Monokai Light",
                                    "appearance": "light",
                                    "style": {}
                                }
                            ]
                        }"#,
                        "monokai-pro.json": r#"{
                            "name": "Monokai Pro",
                            "author": "Someone",
                            "themes": [
                                {
                                    "name": "Monokai Pro Dark",
                                    "appearance": "dark",
                                    "style": {}
                                },
                                {
                                    "name": "Monokai Pro Light",
                                    "appearance": "light",
                                    "style": {}
                                }
                            ]
                        }"#,
                    }
                },
                "mav-ruby": {
                    "extension.json": r#"{
                        "id": "mav-ruby",
                        "name": "Mav Ruby",
                        "version": "1.0.0",
                        "grammars": {
                            "ruby": "grammars/ruby.wasm",
                            "embedded_template": "grammars/embedded_template.wasm"
                        },
                        "languages": {
                            "ruby": "languages/ruby",
                            "erb": "languages/erb"
                        }
                    }"#,
                    "grammars": {
                        "ruby.wasm": "",
                        "embedded_template.wasm": "",
                    },
                    "languages": {
                        "ruby": {
                            "config.toml": r#"
                                name = "Ruby"
                                grammar = "ruby"
                                path_suffixes = ["rb"]
                            "#,
                            "highlights.scm": "",
                        },
                        "erb": {
                            "config.toml": r#"
                                name = "ERB"
                                grammar = "embedded_template"
                                path_suffixes = ["erb"]
                            "#,
                            "highlights.scm": "",
                        }
                    },
                }
            }
        }),
    )
    .await;
    let mut expected_index = ExtensionIndex {
        extensions: [
            (
                "mav-ruby".into(),
                ExtensionIndexEntry {
                    manifest: Arc::new(ExtensionManifest {
                        id: "mav-ruby".into(),
                        name: "Mav Ruby".into(),
                        version: "1.0.0".into(),
                        schema_version: SchemaVersion::ZERO,
                        description: None,
                        authors: Vec::new(),
                        repository: None,
                        themes: Default::default(),
                        icon_themes: Vec::new(),
                        lib: Default::default(),
                        languages: vec![
                            rel_path_buf("languages/erb"),
                            rel_path_buf("languages/ruby"),
                        ],
                        grammars: [
                            ("embedded_template".into(), GrammarManifestEntry::default()),
                            ("ruby".into(), GrammarManifestEntry::default()),
                        ]
                        .into_iter()
                        .collect(),
                        language_servers: BTreeMap::default(),
                        context_servers: BTreeMap::default(),
                        slash_commands: BTreeMap::default(),
                        snippets: None,
                        capabilities: Vec::new(),
                        debug_adapters: Default::default(),
                        debug_locators: Default::default(),
                        language_model_providers: BTreeMap::default(),
                    }),
                    dev: false,
                },
            ),
            (
                "mav-monokai".into(),
                ExtensionIndexEntry {
                    manifest: Arc::new(ExtensionManifest {
                        id: "mav-monokai".into(),
                        name: "Mav Monokai".into(),
                        version: "2.0.0".into(),
                        schema_version: SchemaVersion::ZERO,
                        description: None,
                        authors: vec![],
                        repository: None,
                        themes: vec![
                            rel_path_buf("themes/monokai-pro.json"),
                            rel_path_buf("themes/monokai.json"),
                        ],
                        icon_themes: Vec::new(),
                        lib: Default::default(),
                        languages: Default::default(),
                        grammars: BTreeMap::default(),
                        language_servers: BTreeMap::default(),
                        context_servers: BTreeMap::default(),
                        slash_commands: BTreeMap::default(),
                        snippets: None,
                        capabilities: Vec::new(),
                        debug_adapters: Default::default(),
                        debug_locators: Default::default(),
                        language_model_providers: BTreeMap::default(),
                    }),
                    dev: false,
                },
            ),
        ]
        .into_iter()
        .collect(),
        languages: [
            (
                "ERB".into(),
                ExtensionIndexLanguageEntry {
                    extension: "mav-ruby".into(),
                    path: "languages/erb".into(),
                    grammar: Some("embedded_template".into()),
                    hidden: false,
                    matcher: LanguageMatcher {
                        path_suffixes: vec!["erb".into()],
                        first_line_pattern: None,
                        ..LanguageMatcher::default()
                    },
                },
            ),
            (
                "Ruby".into(),
                ExtensionIndexLanguageEntry {
                    extension: "mav-ruby".into(),
                    path: "languages/ruby".into(),
                    grammar: Some("ruby".into()),
                    hidden: false,
                    matcher: LanguageMatcher {
                        path_suffixes: vec!["rb".into()],
                        first_line_pattern: None,
                        ..LanguageMatcher::default()
                    },
                },
            ),
        ]
        .into_iter()
        .collect(),
        themes: [
            (
                "Monokai Dark".into(),
                ExtensionIndexThemeEntry {
                    extension: "mav-monokai".into(),
                    path: "themes/monokai.json".into(),
                },
            ),
            (
                "Monokai Light".into(),
                ExtensionIndexThemeEntry {
                    extension: "mav-monokai".into(),
                    path: "themes/monokai.json".into(),
                },
            ),
            (
                "Monokai Pro Dark".into(),
                ExtensionIndexThemeEntry {
                    extension: "mav-monokai".into(),
                    path: "themes/monokai-pro.json".into(),
                },
            ),
            (
                "Monokai Pro Light".into(),
                ExtensionIndexThemeEntry {
                    extension: "mav-monokai".into(),
                    path: "themes/monokai-pro.json".into(),
                },
            ),
        ]
        .into_iter()
        .collect(),
        icon_themes: BTreeMap::default(),
    };
    let proxy = Arc::new(ExtensionHostProxy::new());
    let theme_registry = Arc::new(ThemeRegistry::new(Box::new(())));
    theme_extension::init(proxy.clone(), theme_registry.clone(), cx.executor());
    let language_registry = Arc::new(LanguageRegistry::test(cx.executor()));
    language_extension::init(LspAccess::Noop, proxy.clone(), language_registry.clone());
    let node_runtime = NodeRuntime::unavailable();
    let store = cx.new(|cx| {
        ExtensionStore::new(
            PathBuf::from("/the-extension-dir"),
            None,
            proxy.clone(),
            fs.clone(),
            http_client.clone(),
            http_client.clone(),
            None,
            node_runtime.clone(),
            cx,
        )
    });
    cx.executor().advance_clock(RELOAD_DEBOUNCE_DURATION);
    store.read_with(cx, |store, _| {
        let index = &store.extension_index;
        assert_eq!(index.extensions, expected_index.extensions);
        for ((actual_key, actual_language), (expected_key, expected_language)) in
            index.languages.iter().zip(expected_index.languages.iter())
        {
            assert_eq!(actual_key, expected_key);
            assert_eq!(actual_language.grammar, expected_language.grammar);
            assert_eq!(actual_language.matcher, expected_language.matcher);
            assert_eq!(actual_language.hidden, expected_language.hidden);
        }
        assert_eq!(index.themes, expected_index.themes);

        assert_eq!(
            language_registry.language_names(),
            [
                LanguageName::new_static("ERB"),
                LanguageName::new_static("Plain Text"),
                LanguageName::new_static("Ruby"),
            ]
        );
        assert_eq!(
            theme_registry.list_names(),
            [
                "Monokai Dark",
                "Monokai Light",
                "Monokai Pro Dark",
                "Monokai Pro Light",
                "One Dark",
            ]
        );
    });

    fs.insert_tree(
        "/the-extension-dir/installed/mav-gruvbox",
        json!({
            "extension.json": r#"{
                "id": "mav-gruvbox",
                "name": "Mav Gruvbox",
                "version": "1.0.0",
                "themes": {
                    "Gruvbox": "themes/gruvbox.json"
                }
            }"#,
            "themes": {
                "gruvbox.json": r#"{
                    "name": "Gruvbox",
                    "author": "Someone Else",
                    "themes": [
                        {
                            "name": "Gruvbox",
                            "appearance": "dark",
                            "style": {}
                        }
                    ]
                }"#,
            }
        }),
    )
    .await;

    expected_index.extensions.insert(
        "mav-gruvbox".into(),
        ExtensionIndexEntry {
            manifest: Arc::new(ExtensionManifest {
                id: "mav-gruvbox".into(),
                name: "Mav Gruvbox".into(),
                version: "1.0.0".into(),
                schema_version: SchemaVersion::ZERO,
                description: None,
                authors: vec![],
                repository: None,
                themes: vec![rel_path_buf("themes/gruvbox.json")],
                icon_themes: Vec::new(),
                lib: Default::default(),
                languages: Default::default(),
                grammars: BTreeMap::default(),
                language_servers: BTreeMap::default(),
                context_servers: BTreeMap::default(),
                slash_commands: BTreeMap::default(),
                snippets: None,
                capabilities: Vec::new(),
                debug_adapters: Default::default(),
                debug_locators: Default::default(),
                language_model_providers: BTreeMap::default(),
            }),
            dev: false,
        },
    );
    expected_index.themes.insert(
        "Gruvbox".into(),
        ExtensionIndexThemeEntry {
            extension: "mav-gruvbox".into(),
            path: "themes/gruvbox.json".into(),
        },
    );

    #[allow(clippy::let_underscore_future)]
    let _ = store.update(cx, |store, cx| store.reload(None, cx));

    cx.executor().advance_clock(RELOAD_DEBOUNCE_DURATION);
    store.read_with(cx, |store, _| {
        let index = &store.extension_index;

        for ((actual_key, actual_language), (expected_key, expected_language)) in
            index.languages.iter().zip(expected_index.languages.iter())
        {
            assert_eq!(actual_key, expected_key);
            assert_eq!(actual_language.grammar, expected_language.grammar);
            assert_eq!(actual_language.matcher, expected_language.matcher);
            assert_eq!(actual_language.hidden, expected_language.hidden);
        }

        assert_eq!(index.extensions, expected_index.extensions);
        assert_eq!(index.themes, expected_index.themes);

        assert_eq!(
            theme_registry.list_names(),
            [
                "Gruvbox",
                "Monokai Dark",
                "Monokai Light",
                "Monokai Pro Dark",
                "Monokai Pro Light",
                "One Dark",
            ]
        );
    });

    let prev_fs_metadata_call_count = fs.metadata_call_count();
    let prev_fs_read_dir_call_count = fs.read_dir_call_count();

    // Create new extension store, as if Mav were restarting.
    drop(store);
    let store = cx.new(|cx| {
        ExtensionStore::new(
            PathBuf::from("/the-extension-dir"),
            None,
            proxy,
            fs.clone(),
            http_client.clone(),
            http_client.clone(),
            None,
            node_runtime.clone(),
            cx,
        )
    });

    cx.executor().run_until_parked();
    store.read_with(cx, |store, _| {
        assert_eq!(store.extension_index.extensions, expected_index.extensions);
        assert_eq!(store.extension_index.themes, expected_index.themes);
        assert_eq!(
            store.extension_index.icon_themes,
            expected_index.icon_themes
        );

        for ((actual_key, actual_language), (expected_key, expected_language)) in store
            .extension_index
            .languages
            .iter()
            .zip(expected_index.languages.iter())
        {
            assert_eq!(actual_key, expected_key);
            assert_eq!(actual_language.grammar, expected_language.grammar);
            assert_eq!(actual_language.matcher, expected_language.matcher);
            assert_eq!(actual_language.hidden, expected_language.hidden);
        }

        assert_eq!(
            language_registry.language_names(),
            [
                LanguageName::new_static("ERB"),
                LanguageName::new_static("Plain Text"),
                LanguageName::new_static("Ruby"),
            ]
        );
        assert_eq!(
            language_registry.grammar_names(),
            ["embedded_template".into(), "ruby".into()]
        );
        assert_eq!(
            theme_registry.list_names(),
            [
                "Gruvbox",
                "Monokai Dark",
                "Monokai Light",
                "Monokai Pro Dark",
                "Monokai Pro Light",
                "One Dark",
            ]
        );

        // The on-disk manifest limits the number of FS calls that need to be made
        // on startup.
        assert_eq!(fs.read_dir_call_count(), prev_fs_read_dir_call_count);
        assert_eq!(fs.metadata_call_count(), prev_fs_metadata_call_count + 2);
    });

    store.update(cx, |store, cx| {
        store
            .uninstall_extension("mav-ruby".into(), cx)
            .detach_and_log_err(cx);
    });

    cx.executor().advance_clock(RELOAD_DEBOUNCE_DURATION);
    expected_index.extensions.remove("mav-ruby");
    expected_index.languages.remove("Ruby");
    expected_index.languages.remove("ERB");

    store.read_with(cx, |store, _| {
        assert_eq!(store.extension_index.extensions, expected_index.extensions);
        assert_eq!(store.extension_index.themes, expected_index.themes);
        assert_eq!(
            store.extension_index.icon_themes,
            expected_index.icon_themes
        );

        for ((actual_key, actual_language), (expected_key, expected_language)) in store
            .extension_index
            .languages
            .iter()
            .zip(expected_index.languages.iter())
        {
            assert_eq!(actual_key, expected_key);
            assert_eq!(actual_language.grammar, expected_language.grammar);
            assert_eq!(actual_language.matcher, expected_language.matcher);
            assert_eq!(actual_language.hidden, expected_language.hidden);
        }

        assert_eq!(
            language_registry.language_names(),
            [LanguageName::new_static("Plain Text")]
        );
        assert_eq!(language_registry.grammar_names(), []);
    });
}

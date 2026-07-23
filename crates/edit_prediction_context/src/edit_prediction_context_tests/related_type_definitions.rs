use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_type_definitions_in_related_files(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "config.rs": indoc! {r#"
                    pub struct Config {
                        debug: bool,
                        verbose: bool,
                    }
                "#},
                "widget.rs": indoc! {r#"
                    use super::config::Config;

                    pub struct Widget {
                        config: Config,
                        name: String,
                    }

                    impl Widget {
                        pub fn render(&self) {
                            if self.config.debug {
                                println!("debug mode");
                            }
                        }
                    }
                "#},
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let mut servers = setup_fake_lsp(&project, cx);

    let (buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/root/src/widget.rs"), cx)
        })
        .await
        .unwrap();

    let _server = servers.next().await.unwrap();
    cx.run_until_parked();

    let related_excerpt_store = cx.new(|cx| RelatedExcerptStore::new(&project, cx));
    related_excerpt_store.update(cx, |store, cx| {
        let position = {
            let buffer = buffer.read(cx);
            let offset = buffer
                .text()
                .find("self.config.debug")
                .expect("self.config.debug not found");
            buffer.anchor_before(offset)
        };

        store.set_identifier_line_count(0);
        store.refresh(buffer.clone(), position, cx);
    });

    cx.executor().advance_clock(DEBOUNCE_DURATION);
    // config.rs appears ONLY because the fake LSP resolves the type annotation
    // `config: Config` to `pub struct Config` via GotoTypeDefinition.
    // widget.rs appears from regular definitions of Widget / render.
    related_excerpt_store.update(cx, |store, cx| {
        let excerpts = store.related_files(cx);
        assert_related_files(
            &excerpts,
            &[
                (
                    "root/src/config.rs",
                    &[indoc! {"
                        pub struct Config {
                            debug: bool,
                            verbose: bool,
                        }"}],
                ),
                (
                    "root/src/widget.rs",
                    &[
                        indoc! {"
                        pub struct Widget {
                            config: Config,
                            name: String,
                        }

                        impl Widget {
                            pub fn render(&self) {"},
                        indoc! {"
                            }
                        }"},
                    ],
                ),
            ],
        );
    });
}

#[gpui::test]
async fn test_type_definition_deduplication(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    // In this project the only identifier near the cursor whose type definition
    // resolves is `TypeA`, and its GotoTypeDefinition returns the exact same
    // location as GotoDefinition. After deduplication the CacheEntry for `TypeA`
    // should have an empty `type_definitions` vec, meaning the type-definition
    // path contributes nothing extra to the related-file output.
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "types.rs": indoc! {r#"
                    pub struct TypeA {
                        value: i32,
                    }

                    pub struct TypeB {
                        label: String,
                    }
                "#},
                "main.rs": indoc! {r#"
                    use super::types::TypeA;

                    fn work() {
                        let item: TypeA = unimplemented!();
                        println!("{}", item.value);
                    }
                "#},
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let mut servers = setup_fake_lsp(&project, cx);

    let (buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/root/src/main.rs"), cx)
        })
        .await
        .unwrap();

    let _server = servers.next().await.unwrap();
    cx.run_until_parked();

    let related_excerpt_store = cx.new(|cx| RelatedExcerptStore::new(&project, cx));
    related_excerpt_store.update(cx, |store, cx| {
        let position = {
            let buffer = buffer.read(cx);
            let offset = buffer.text().find("let item").expect("let item not found");
            buffer.anchor_before(offset)
        };

        store.set_identifier_line_count(0);
        store.refresh(buffer.clone(), position, cx);
    });

    cx.executor().advance_clock(DEBOUNCE_DURATION);
    // types.rs appears because `TypeA` has a regular definition there.
    // `item`'s type definition also resolves to TypeA in types.rs, but
    // deduplication removes it since it points to the same location.
    // TypeB should NOT appear because nothing references it.
    related_excerpt_store.update(cx, |store, cx| {
        let excerpts = store.related_files(cx);
        assert_related_files(
            &excerpts,
            &[
                (
                    "root/src/types.rs",
                    &[indoc! {"
                        pub struct TypeA {
                            value: i32,
                        }"}],
                ),
                ("root/src/main.rs", &["fn work() {", "}"]),
            ],
        );
    });
}

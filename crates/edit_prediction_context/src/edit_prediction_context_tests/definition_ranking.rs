use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_definitions_ranked_by_cursor_proximity(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    // helpers.rs has an impl block whose body exceeds the test
    // MAX_OUTLINE_ITEM_BODY_SIZE (24 bytes), so assemble_excerpt_ranges
    // splits it into header + individual children + closing brace. main.rs
    // references two of the three methods on separate lines at varying
    // distances from the cursor. This exercises:
    //   1. File ordering by closest identifier rank.
    //   2. Per-excerpt ordering within a file — child excerpts carry the rank
    //      of the identifier that discovered them.
    //   3. Parent excerpt (impl header / closing brace) inheriting the minimum
    //      order of its children.
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "helpers.rs": indoc! {r#"
                    pub struct Helpers {
                        value: i32,
                    }

                    impl Helpers {
                        pub fn alpha(&self) -> i32 {
                            let intermediate = self.value;
                            intermediate + 1
                        }

                        pub fn beta(&self) -> i32 {
                            let intermediate = self.value;
                            intermediate + 2
                        }

                        pub fn gamma(&self) -> i32 {
                            let intermediate = self.value;
                            intermediate + 3
                        }
                    }
                "#},
                "main.rs": indoc! {r#"
                    use super::helpers::Helpers;

                    fn process(h: Helpers) {
                        let a = h.alpha();
                        let b = h.gamma();
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

    // Place cursor on "h.alpha()". `alpha` is at distance 0, `gamma` is
    // farther below. Both resolve to methods inside `impl Helpers` in
    // helpers.rs. The impl header and closing brace excerpts should inherit
    // the min order of their children (alpha's order).
    let related_excerpt_store = cx.new(|cx| RelatedExcerptStore::new(&project, cx));
    related_excerpt_store.update(cx, |store, cx| {
        let position = {
            let buffer = buffer.read(cx);
            let offset = buffer.text().find("h.alpha()").unwrap();
            buffer.anchor_before(offset)
        };

        store.set_identifier_line_count(1);
        store.refresh(buffer.clone(), position, cx);
    });

    cx.executor().advance_clock(DEBOUNCE_DURATION);
    related_excerpt_store.update(cx, |store, cx| {
        let files = store.related_files(cx);

        // helpers.rs has 4 excerpts: the struct+impl header merged with
        // the alpha method header (order 1 from alpha), alpha's closing
        // brace (order 1), gamma's method header (order 6), and the
        // gamma+impl closing brace (order 1, inherited from alpha which
        // is also a child of the impl).
        let alpha_order = 1;
        let gamma_order = 6;
        assert_related_files_with_orders(
            &files,
            &[
                (
                    "root/src/helpers.rs",
                    &[
                        (
                            indoc! {"
                            pub struct Helpers {
                                value: i32,
                            }

                            impl Helpers {
                                pub fn alpha(&self) -> i32 {"},
                            alpha_order,
                        ),
                        ("    }", alpha_order),
                        ("    pub fn gamma(&self) -> i32 {", gamma_order),
                        (
                            indoc! {"
                                }
                            }"},
                            alpha_order,
                        ),
                    ],
                ),
                (
                    "root/src/main.rs",
                    &[("fn process(h: Helpers) {", 8), ("}", 8)],
                ),
            ],
        );
    });

    // Now move cursor to "h.gamma()" — gamma becomes closest, reranking the
    // excerpts so that the gamma method excerpt has the best order and the
    // alpha method excerpt has a worse order.
    related_excerpt_store.update(cx, |store, cx| {
        let position = {
            let buffer = buffer.read(cx);
            let offset = buffer.text().find("h.gamma()").unwrap();
            buffer.anchor_before(offset)
        };

        store.set_identifier_line_count(1);
        store.refresh(buffer.clone(), position, cx);
    });

    cx.executor().advance_clock(DEBOUNCE_DURATION);
    related_excerpt_store.update(cx, |store, cx| {
        let files = store.related_files(cx);

        // Now gamma is closest. The alpha method excerpts carry alpha's
        // rank (3), and the gamma method excerpts carry gamma's rank (1).
        // The impl closing brace merges with gamma's closing brace and
        // inherits gamma's order (the best child).
        let alpha_order = 3;
        let gamma_order = 1;
        assert_related_files_with_orders(
            &files,
            &[
                (
                    "root/src/helpers.rs",
                    &[
                        (
                            indoc! {"
                            pub struct Helpers {
                                value: i32,
                            }

                            impl Helpers {
                                pub fn alpha(&self) -> i32 {"},
                            alpha_order,
                        ),
                        ("    }", alpha_order),
                        ("    pub fn gamma(&self) -> i32 {", gamma_order),
                        (
                            indoc! {"
                                }
                            }"},
                            gamma_order,
                        ),
                    ],
                ),
                (
                    "root/src/main.rs",
                    &[("fn process(h: Helpers) {", 8), ("}", 8)],
                ),
            ],
        );
    });
}

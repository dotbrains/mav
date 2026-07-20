use super::*;

#[gpui::test]
async fn test_sticky_scroll(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let buffer = indoc! {"
            ˇfn foo() {
                let abc = 123;
            }
            struct Bar;
            impl Bar {
                fn new() -> Self {
                    Self
                }
            }
            fn baz() {
            }
        "};
    cx.set_state(&buffer);

    cx.update_editor(|e, _, cx| {
        e.buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| {
                buffer.set_language(Some(rust_lang()), cx);
            })
    });

    let mut sticky_headers = |offset: ScrollOffset| {
        cx.update_editor(|e, window, cx| {
            e.scroll(gpui::Point { x: 0., y: offset }, None, window, cx);
        });
        cx.run_until_parked();
        cx.update_editor(|e, window, cx| {
            EditorElement::sticky_headers(&e, &e.snapshot(window, cx))
                .into_iter()
                .map(
                    |StickyHeader {
                         start_point,
                         offset,
                         ..
                     }| { (start_point, offset) },
                )
                .collect::<Vec<_>>()
        })
    };

    let fn_foo = Point { row: 0, column: 0 };
    let impl_bar = Point { row: 4, column: 0 };
    let fn_new = Point { row: 5, column: 4 };

    assert_eq!(sticky_headers(0.0), vec![]);
    assert_eq!(sticky_headers(0.5), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(1.0), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(1.5), vec![(fn_foo, -0.5)]);
    assert_eq!(sticky_headers(2.0), vec![]);
    assert_eq!(sticky_headers(2.5), vec![]);
    assert_eq!(sticky_headers(3.0), vec![]);
    assert_eq!(sticky_headers(3.5), vec![]);
    assert_eq!(sticky_headers(4.0), vec![]);
    assert_eq!(sticky_headers(4.5), vec![(impl_bar, 0.0), (fn_new, 1.0)]);
    assert_eq!(sticky_headers(5.0), vec![(impl_bar, 0.0), (fn_new, 1.0)]);
    assert_eq!(sticky_headers(5.5), vec![(impl_bar, 0.0), (fn_new, 0.5)]);
    assert_eq!(sticky_headers(6.0), vec![(impl_bar, 0.0)]);
    assert_eq!(sticky_headers(6.5), vec![(impl_bar, 0.0)]);
    assert_eq!(sticky_headers(7.0), vec![(impl_bar, 0.0)]);
    assert_eq!(sticky_headers(7.5), vec![(impl_bar, -0.5)]);
    assert_eq!(sticky_headers(8.0), vec![]);
    assert_eq!(sticky_headers(8.5), vec![]);
    assert_eq!(sticky_headers(9.0), vec![]);
    assert_eq!(sticky_headers(9.5), vec![]);
    assert_eq!(sticky_headers(10.0), vec![]);
}

#[gpui::test]
async fn test_sticky_scroll_with_decoration_prefix_in_item(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "TypeScript".into(),
                ..Default::default()
            },
            Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        )
        .with_outline_query(
            r#"
            (class_declaration
                "class" @context
                name: (_) @name) @item
            "#,
        )
        .expect("TypeScript outline query"),
    );

    let buffer = indoc! {"
        ˇ@Decorator
        class Foo {
            x = 1;
            y = 2;
            z = 3;
            w = 4;
        }
    "};
    cx.set_state(buffer);
    cx.update_editor(|e, _, cx| {
        e.buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| {
                buffer.set_language(Some(language), cx);
            })
    });

    let mut sticky_headers = |offset: ScrollOffset| {
        cx.update_editor(|e, window, cx| {
            e.scroll(gpui::Point { x: 0., y: offset }, None, window, cx);
        });
        cx.run_until_parked();
        cx.update_editor(|e, window, cx| {
            EditorElement::sticky_headers(&e, &e.snapshot(window, cx))
                .into_iter()
                .map(
                    |StickyHeader {
                         start_point,
                         offset,
                         ..
                     }| { (start_point, offset) },
                )
                .collect::<Vec<_>>()
        })
    };

    let class_foo = Point { row: 1, column: 0 };

    assert_eq!(sticky_headers(0.0), vec![]);
    assert_eq!(sticky_headers(1.5), vec![(class_foo, 0.0)]);
    assert_eq!(sticky_headers(2.5), vec![(class_foo, 0.0)]);
    assert_eq!(sticky_headers(5.5), vec![(class_foo, -0.5)]);
    assert_eq!(sticky_headers(6.0), vec![]);
    assert_eq!(sticky_headers(7.0), vec![]);
}

#[gpui::test]
async fn test_sticky_scroll_anchors_multiline_c_signature_on_name_row(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let buffer = indoc! {"
        ˇvoid
        evdev_post_scroll(struct evdev_device *device,
                  usec_t time,
                  enum libinput_pointer_axis_source source,
                  const struct normalized_coords *delta)
        {
            const struct normalized_coords tilt_rot = {
                cos(SCROLL_DELTA_TILT_ANGLE),
                sin(SCROLL_DELTA_TILT_ANGLE),
            };
        }
    "};
    cx.set_state(buffer);

    cx.update_editor(|editor, _, cx| {
        editor
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| {
                buffer.set_language(
                    Some(languages::language("c", tree_sitter_c::LANGUAGE.into())),
                    cx,
                );
            })
    });

    let mut sticky_headers = |offset: ScrollOffset| {
        cx.update_editor(|editor, window, cx| {
            editor.scroll(gpui::Point { x: 0., y: offset }, None, window, cx);
        });
        cx.run_until_parked();
        cx.update_editor(|editor, window, cx| {
            EditorElement::sticky_headers(&editor, &editor.snapshot(window, cx))
                .into_iter()
                .map(
                    |StickyHeader {
                         start_point,
                         offset,
                         ..
                     }| { (start_point, offset) },
                )
                .collect::<Vec<_>>()
        })
    };

    let function_name_row = Point { row: 1, column: 0 };

    assert_eq!(sticky_headers(1.0), vec![]);
    assert_eq!(sticky_headers(1.5), vec![(function_name_row, 0.0)]);
    assert_eq!(sticky_headers(5.0), vec![(function_name_row, 0.0)]);
}

#[gpui::test]
async fn test_sticky_scroll_with_expanded_deleted_diff_hunks(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = indoc! {"
        fn foo() {
            let a = 1;
            let b = 2;
            let c = 3;
            let d = 4;
            let e = 5;
        }
    "};

    let buffer = indoc! {"
        ˇfn foo() {
        }
    "};

    cx.set_state(&buffer);

    cx.update_editor(|e, _, cx| {
        e.buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| {
                buffer.set_language(Some(rust_lang()), cx);
            })
    });

    cx.set_head_text(diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    // After expanding, the display should look like:
    //   row 0: fn foo() {
    //   row 1: -    let a = 1;   (deleted)
    //   row 2: -    let b = 2;   (deleted)
    //   row 3: -    let c = 3;   (deleted)
    //   row 4: -    let d = 4;   (deleted)
    //   row 5: -    let e = 5;   (deleted)
    //   row 6: }
    //
    // fn foo() spans display rows 0-6. Scrolling into the deleted region
    // (rows 1-5) should still show fn foo() as a sticky header.

    let fn_foo = Point { row: 0, column: 0 };

    let mut sticky_headers = |offset: ScrollOffset| {
        cx.update_editor(|e, window, cx| {
            e.scroll(gpui::Point { x: 0., y: offset }, None, window, cx);
        });
        cx.run_until_parked();
        cx.update_editor(|e, window, cx| {
            EditorElement::sticky_headers(&e, &e.snapshot(window, cx))
                .into_iter()
                .map(
                    |StickyHeader {
                         start_point,
                         offset,
                         ..
                     }| { (start_point, offset) },
                )
                .collect::<Vec<_>>()
        })
    };

    assert_eq!(sticky_headers(0.0), vec![]);
    assert_eq!(sticky_headers(0.5), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(1.0), vec![(fn_foo, 0.0)]);
    // Scrolling into deleted lines: fn foo() should still be a sticky header.
    assert_eq!(sticky_headers(2.0), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(3.0), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(4.0), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(5.0), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(5.5), vec![(fn_foo, -0.5)]);
    // Past the closing brace: no more sticky header.
    assert_eq!(sticky_headers(6.0), vec![]);
}

use super::*;

#[test]
fn test_split_words() {
    fn split(text: &str) -> Vec<&str> {
        split_words(text).collect()
    }

    assert_eq!(split("HelloWorld"), &["Hello", "World"]);
    assert_eq!(split("hello_world"), &["hello_", "world"]);
    assert_eq!(split("_hello_world_"), &["_", "hello_", "world_"]);
    assert_eq!(split("Hello_World"), &["Hello_", "World"]);
    assert_eq!(split("helloWOrld"), &["hello", "WOrld"]);
    assert_eq!(split("helloworld"), &["helloworld"]);

    assert_eq!(split(":do_the_thing"), &[":", "do_", "the_", "thing"]);
}

#[test]
fn test_split_words_for_snippet_prefix() {
    fn split(text: &str) -> Vec<&str> {
        snippet_candidate_suffixes(text, &|c| c.is_alphanumeric() || c == '_').collect()
    }

    assert_eq!(split("HelloWorld"), &["HelloWorld"]);
    assert_eq!(split("hello_world"), &["hello_world"]);
    assert_eq!(split("_hello_world_"), &["_hello_world_"]);
    assert_eq!(split("Hello_World"), &["Hello_World"]);
    assert_eq!(split("helloWOrld"), &["helloWOrld"]);
    assert_eq!(split("helloworld"), &["helloworld"]);
    assert_eq!(
        split("this@is!@#$^many   . symbols"),
        &[
            "symbols",
            " symbols",
            ". symbols",
            " . symbols",
            "  . symbols",
            "   . symbols",
            "many   . symbols",
            "^many   . symbols",
            "$^many   . symbols",
            "#$^many   . symbols",
            "@#$^many   . symbols",
            "!@#$^many   . symbols",
            "is!@#$^many   . symbols",
            "@is!@#$^many   . symbols",
            "this@is!@#$^many   . symbols",
        ],
    );
    assert_eq!(split("a.s"), &["s", ".s", "a.s"]);
}

#[gpui::test]
async fn test_move_to_syntax_node_relative_jumps(tcx: &mut TestAppContext) {
    init_test(tcx, |_| {});

    let mut cx = EditorLspTestContext::new(
        Arc::into_inner(markdown_lang()).unwrap(),
        Default::default(),
        tcx,
    )
    .await;

    async fn assert(offset: i8, before: &str, after: &str, cx: &mut EditorLspTestContext) {
        let _state_context = cx.set_state(before);
        cx.run_until_parked();
        cx.update_editor(|editor, window, cx| editor.go_to_symbol_by_offset(window, cx, offset))
            .await
            .unwrap();
        cx.run_until_parked();
        cx.assert_editor_state(after);
    }

    const ABOVE: i8 = -1;
    const BELOW: i8 = 1;

    assert(
        ABOVE,
        indoc! {"
        # Foo

        ˇFoo foo foo

        # Bar

        Bar bar bar
    "},
        indoc! {"
        ˇ# Foo

        Foo foo foo

        # Bar

        Bar bar bar
    "},
        &mut cx,
    )
    .await;

    assert(
        ABOVE,
        indoc! {"
        ˇ# Foo

        Foo foo foo

        # Bar

        Bar bar bar
    "},
        indoc! {"
        ˇ# Foo

        Foo foo foo

        # Bar

        Bar bar bar
    "},
        &mut cx,
    )
    .await;

    assert(
        BELOW,
        indoc! {"
        ˇ# Foo

        Foo foo foo

        # Bar

        Bar bar bar
    "},
        indoc! {"
        # Foo

        Foo foo foo

        ˇ# Bar

        Bar bar bar
    "},
        &mut cx,
    )
    .await;

    assert(
        BELOW,
        indoc! {"
        # Foo

        ˇFoo foo foo

        # Bar

        Bar bar bar
    "},
        indoc! {"
        # Foo

        Foo foo foo

        ˇ# Bar

        Bar bar bar
    "},
        &mut cx,
    )
    .await;

    assert(
        BELOW,
        indoc! {"
        # Foo

        Foo foo foo

        ˇ# Bar

        Bar bar bar
    "},
        indoc! {"
        # Foo

        Foo foo foo

        ˇ# Bar

        Bar bar bar
    "},
        &mut cx,
    )
    .await;

    assert(
        BELOW,
        indoc! {"
        # Foo

        Foo foo foo

        # Bar
        ˇ
        Bar bar bar
    "},
        indoc! {"
        # Foo

        Foo foo foo

        # Bar
        ˇ
        Bar bar bar
    "},
        &mut cx,
    )
    .await;
}

#[gpui::test]
async fn test_move_to_syntax_node_relative_dead_zone(tcx: &mut TestAppContext) {
    init_test(tcx, |_| {});

    let mut cx = EditorLspTestContext::new(
        Arc::into_inner(rust_lang()).unwrap(),
        Default::default(),
        tcx,
    )
    .await;

    async fn assert(offset: i8, before: &str, after: &str, cx: &mut EditorLspTestContext) {
        let _state_context = cx.set_state(before);
        cx.run_until_parked();
        cx.update_editor(|editor, window, cx| editor.go_to_symbol_by_offset(window, cx, offset))
            .await
            .unwrap();
        cx.run_until_parked();
        cx.assert_editor_state(after);
    }

    const ABOVE: i8 = -1;
    const BELOW: i8 = 1;

    assert(
        ABOVE,
        indoc! {"
        fn foo() {
            // foo fn
        }

        ˇ// this zone is not inside any top level outline node

        fn bar() {
            // bar fn
            let _ = 2;
        }
    "},
        indoc! {"
        ˇfn foo() {
            // foo fn
        }

        // this zone is not inside any top level outline node

        fn bar() {
            // bar fn
            let _ = 2;
        }
    "},
        &mut cx,
    )
    .await;

    assert(
        BELOW,
        indoc! {"
        fn foo() {
            // foo fn
        }

        ˇ// this zone is not inside any top level outline node

        fn bar() {
            // bar fn
            let _ = 2;
        }
    "},
        indoc! {"
        fn foo() {
            // foo fn
        }

        // this zone is not inside any top level outline node

        ˇfn bar() {
            // bar fn
            let _ = 2;
        }
    "},
        &mut cx,
    )
    .await;
}

#[gpui::test]
async fn test_move_to_enclosing_bracket(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_typescript(Default::default(), cx).await;

    #[track_caller]
    fn assert(before: &str, after: &str, cx: &mut EditorLspTestContext) {
        let _state_context = cx.set_state(before);
        cx.run_until_parked();
        cx.update_editor(|editor, window, cx| {
            editor.move_to_enclosing_bracket(&MoveToEnclosingBracket, window, cx)
        });
        cx.run_until_parked();
        cx.assert_editor_state(after);
    }

    // Outside bracket jumps to outside of matching bracket
    assert("console.logˇ(var);", "console.log(var)ˇ;", &mut cx);
    assert("console.log(var)ˇ;", "console.logˇ(var);", &mut cx);

    // Inside bracket jumps to inside of matching bracket
    assert("console.log(ˇvar);", "console.log(varˇ);", &mut cx);
    assert("console.log(varˇ);", "console.log(ˇvar);", &mut cx);

    // When outside a bracket and inside, favor jumping to the inside bracket
    assert(
        "console.log('foo', [1, 2, 3]ˇ);",
        "console.log('foo', ˇ[1, 2, 3]);",
        &mut cx,
    );
    assert(
        "console.log(ˇ'foo', [1, 2, 3]);",
        "console.log('foo'ˇ, [1, 2, 3]);",
        &mut cx,
    );

    // Bias forward if two options are equally likely
    assert(
        "let result = curried_fun()ˇ();",
        "let result = curried_fun()()ˇ;",
        &mut cx,
    );

    // If directly adjacent to a smaller pair but inside a larger (not adjacent), pick the smaller
    assert(
        indoc! {"
            function test() {
                console.log('test')ˇ
            }"},
        indoc! {"
            function test() {
                console.logˇ('test')
            }"},
        &mut cx,
    );
}

#[gpui::test]
async fn test_move_to_enclosing_bracket_in_markdown_code_block(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let language_registry = Arc::new(language::LanguageRegistry::test(cx.executor()));
    language_registry.add(markdown_lang());
    language_registry.add(rust_lang());
    let buffer = cx.new(|cx| {
        let mut buffer = language::Buffer::local(
            indoc! {"
            ```rs
            impl Worktree {
                pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
                }
            }
            ```
        "},
            cx,
        );
        buffer.set_language_registry(language_registry.clone());
        buffer.set_language(Some(markdown_lang()), cx);
        buffer
    });
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let editor = cx.add_window(|window, cx| build_editor(buffer.clone(), window, cx));
    cx.executor().run_until_parked();
    _ = editor.update(cx, |editor, window, cx| {
        // Case 1: Test outer enclosing brackets
        select_ranges(
            editor,
            &indoc! {"
                ```rs
                impl Worktree {
                    pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
                    }
                }ˇ
                ```
            "},
            window,
            cx,
        );
        editor.move_to_enclosing_bracket(&MoveToEnclosingBracket, window, cx);
        assert_text_with_selections(
            editor,
            &indoc! {"
                ```rs
                impl Worktree ˇ{
                    pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
                    }
                }
                ```
            "},
            cx,
        );
        // Case 2: Test inner enclosing brackets
        select_ranges(
            editor,
            &indoc! {"
                ```rs
                impl Worktree {
                    pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
                    }ˇ
                }
                ```
            "},
            window,
            cx,
        );
        editor.move_to_enclosing_bracket(&MoveToEnclosingBracket, window, cx);
        assert_text_with_selections(
            editor,
            &indoc! {"
                ```rs
                impl Worktree {
                    pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> ˇ{
                    }
                }
                ```
            "},
            cx,
        );
    });
}

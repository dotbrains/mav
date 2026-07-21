use super::*;

#[gpui::test]
async fn test_unmatched_forward(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // test it works with curly braces
    cx.set_shared_state(indoc! {r"func (a string) {
                do(something(with<Types>.anˇd_arrays[0, 2]))
            }"})
        .await;
    cx.simulate_shared_keystrokes("] }").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"func (a string) {
                do(something(with<Types>.and_arrays[0, 2]))
            ˇ}"});

    // test it works with brackets
    cx.set_shared_state(indoc! {r"func (a string) {
                do(somethiˇng(with<Types>.and_arrays[0, 2]))
            }"})
        .await;
    cx.simulate_shared_keystrokes("] )").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"func (a string) {
                do(something(with<Types>.and_arrays[0, 2])ˇ)
            }"});

    cx.set_shared_state(indoc! {r"func (a string) { a((b, cˇ))}"})
        .await;
    cx.simulate_shared_keystrokes("] )").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"func (a string) { a((b, c)ˇ)}"});

    // test it works on immediate nesting
    cx.set_shared_state("{ˇ {}{}}").await;
    cx.simulate_shared_keystrokes("] }").await;
    cx.shared_state().await.assert_eq("{ {}{}ˇ}");
    cx.set_shared_state("(ˇ ()())").await;
    cx.simulate_shared_keystrokes("] )").await;
    cx.shared_state().await.assert_eq("( ()()ˇ)");

    // test it works on immediate nesting inside braces
    cx.set_shared_state("{\n    ˇ {()}\n}").await;
    cx.simulate_shared_keystrokes("] }").await;
    cx.shared_state().await.assert_eq("{\n     {()}\nˇ}");
    cx.set_shared_state("(\n    ˇ {()}\n)").await;
    cx.simulate_shared_keystrokes("] )").await;
    cx.shared_state().await.assert_eq("(\n     {()}\nˇ)");
}

#[gpui::test]
async fn test_unmatched_backward(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // test it works with curly braces
    cx.set_shared_state(indoc! {r"func (a string) {
                do(something(with<Types>.anˇd_arrays[0, 2]))
            }"})
        .await;
    cx.simulate_shared_keystrokes("[ {").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"func (a string) ˇ{
                do(something(with<Types>.and_arrays[0, 2]))
            }"});

    // test it works with brackets
    cx.set_shared_state(indoc! {r"func (a string) {
                do(somethiˇng(with<Types>.and_arrays[0, 2]))
            }"})
        .await;
    cx.simulate_shared_keystrokes("[ (").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"func (a string) {
                doˇ(something(with<Types>.and_arrays[0, 2]))
            }"});

    // test it works on immediate nesting
    cx.set_shared_state("{{}{} ˇ }").await;
    cx.simulate_shared_keystrokes("[ {").await;
    cx.shared_state().await.assert_eq("ˇ{{}{}  }");
    cx.set_shared_state("(()() ˇ )").await;
    cx.simulate_shared_keystrokes("[ (").await;
    cx.shared_state().await.assert_eq("ˇ(()()  )");

    // test it works on immediate nesting inside braces
    cx.set_shared_state("{\n    {()} ˇ\n}").await;
    cx.simulate_shared_keystrokes("[ {").await;
    cx.shared_state().await.assert_eq("ˇ{\n    {()} \n}");
    cx.set_shared_state("(\n    {()} ˇ\n)").await;
    cx.simulate_shared_keystrokes("[ (").await;
    cx.shared_state().await.assert_eq("ˇ(\n    {()} \n)");
}

#[gpui::test]
async fn test_unmatched_forward_markdown(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new_markdown_with_rust(cx).await;

    cx.neovim.exec("set filetype=markdown").await;

    cx.set_shared_state(indoc! {r"
            ```rs
            impl Worktree {
                pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
            ˇ    }
            }
            ```
        "})
        .await;
    cx.simulate_shared_keystrokes("] }").await;
    cx.shared_state().await.assert_eq(indoc! {r"
            ```rs
            impl Worktree {
                pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
                ˇ}
            }
            ```
        "});

    cx.set_shared_state(indoc! {r"
            ```rs
            impl Worktree {
                pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
                }   ˇ
            }
            ```
        "})
        .await;
    cx.simulate_shared_keystrokes("] }").await;
    cx.shared_state().await.assert_eq(indoc! {r"
            ```rs
            impl Worktree {
                pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
                }  •
            ˇ}
            ```
        "});
}

#[gpui::test]
async fn test_unmatched_backward_markdown(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new_markdown_with_rust(cx).await;

    cx.neovim.exec("set filetype=markdown").await;

    cx.set_shared_state(indoc! {r"
            ```rs
            impl Worktree {
                pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
            ˇ    }
            }
            ```
        "})
        .await;
    cx.simulate_shared_keystrokes("[ {").await;
    cx.shared_state().await.assert_eq(indoc! {r"
            ```rs
            impl Worktree {
                pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> ˇ{
                }
            }
            ```
        "});

    cx.set_shared_state(indoc! {r"
            ```rs
            impl Worktree {
                pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
                }   ˇ
            }
            ```
        "})
        .await;
    cx.simulate_shared_keystrokes("[ {").await;
    cx.shared_state().await.assert_eq(indoc! {r"
            ```rs
            impl Worktree ˇ{
                pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
                }  •
            }
            ```
        "});
}

#[gpui::test]
async fn test_matching_tags(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new_html(cx).await;

    cx.neovim.exec("set filetype=html").await;

    cx.set_shared_state(indoc! {r"<bˇody></body>"}).await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"<body><ˇ/body>"});
    cx.simulate_shared_keystrokes("%").await;

    // test jumping backwards
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"<ˇbody></body>"});

    // test self-closing tags
    cx.set_shared_state(indoc! {r"<a><bˇr/></a>"}).await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(indoc! {r"<a><bˇr/></a>"});

    // test tag with attributes
    cx.set_shared_state(indoc! {r"<div class='test' ˇid='main'>
            </div>
            "})
        .await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"<div class='test' id='main'>
            <ˇ/div>
            "});

    // test multi-line self-closing tag
    cx.set_shared_state(indoc! {r#"<a>
            <br
                test = "test"
            /ˇ>
        </a>"#})
        .await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(indoc! {r#"<a>
            ˇ<br
                test = "test"
            />
        </a>"#});

    // test nested closing tag
    cx.set_shared_state(indoc! {r#"<html>
            <bˇody>
            </body>
        </html>"#})
        .await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(indoc! {r#"<html>
            <body>
            <ˇ/body>
        </html>"#});
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(indoc! {r#"<html>
            <ˇbody>
            </body>
        </html>"#});
}

#[gpui::test]
async fn test_matching_tag_with_quotes(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new_html(cx).await;
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "%",
            Matching {
                match_quotes: false,
            },
            None,
        )]);
    });

    cx.neovim.exec("set filetype=html").await;
    cx.set_shared_state(indoc! {r"<div class='teˇst' id='main'>
            </div>
            "})
        .await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"<div class='test' id='main'>
            <ˇ/div>
            "});

    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new("%", Matching { match_quotes: true }, None)]);
    });

    cx.set_shared_state(indoc! {r"<div class='teˇst' id='main'>
            </div>
            "})
        .await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"<div class='test' id='main'>
            <ˇ/div>
            "});
}
#[gpui::test]
async fn test_matching_braces_in_tag(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new_typescript(cx).await;

    // test brackets within tags
    cx.set_shared_state(indoc! {r"function f() {
            return (
                <div rules={ˇ[{ a: 1 }]}>
                    <h1>test</h1>
                </div>
            );
        }"})
        .await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(indoc! {r"function f() {
            return (
                <div rules={[{ a: 1 }ˇ]}>
                    <h1>test</h1>
                </div>
            );
        }"});
}

#[gpui::test]
async fn test_matching_nested_brackets(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new_tsx(cx).await;

    cx.set_shared_state(indoc! {r"<Button onClick=ˇ{() => {}}></Button>"})
        .await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"<Button onClick={() => {}ˇ}></Button>"});
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"<Button onClick=ˇ{() => {}}></Button>"});
}

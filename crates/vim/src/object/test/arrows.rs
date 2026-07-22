use super::*;

#[gpui::test]
async fn test_arrow_function_text_object(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_typescript(cx).await;

    cx.set_state(
        indoc! {"
                const foo = () => {
                    return ˇ1;
                };
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {"
                «const foo = () => {
                    return 1;
                };ˇ»
            "},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {"
                arr.map(() => {
                    return ˇ1;
                });
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {"
                arr.map(«() => {
                    return 1;
                }ˇ»);
            "},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {"
                const foo = () => {
                    return ˇ1;
                };
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i f");
    cx.assert_state(
        indoc! {"
                const foo = () => {
                    «return 1;ˇ»
                };
            "},
        Mode::Visual,
    );

    cx.set_state(
        indoc! {"
                (() => {
                    console.log(ˇ1);
                })();
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {"
                («() => {
                    console.log(1);
                }ˇ»)();
            "},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {"
                const foo = () => {
                    return ˇ1;
                };
                export { foo };
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {"
                «const foo = () => {
                    return 1;
                };ˇ»
                export { foo };
            "},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {"
                let bar = () => {
                    return ˇ2;
                };
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {"
                «let bar = () => {
                    return 2;
                };ˇ»
            "},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {"
                var baz = () => {
                    return ˇ3;
                };
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {"
                «var baz = () => {
                    return 3;
                };ˇ»
            "},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {"
                const add = (a, b) => a + ˇb;
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {"
                «const add = (a, b) => a + b;ˇ»
            "},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {"
                const add = ˇ(a, b) => a + b;
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {"
                «const add = (a, b) => a + b;ˇ»
            "},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {"
                const add = (a, b) => a + bˇ;
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {"
                «const add = (a, b) => a + b;ˇ»
            "},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {"
                const add = (a, b) =ˇ> a + b;
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {"
                «const add = (a, b) => a + b;ˇ»
            "},
        Mode::VisualLine,
    );
}

#[gpui::test]
async fn test_arrow_function_in_jsx(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_tsx(cx).await;

    cx.set_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={() => {
                        alert("Hello world!");
                        console.log(ˇ"clicked");
                      }}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={«() => {
                        alert("Hello world!");
                        console.log("clicked");
                      }ˇ»}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={() => console.log("clickˇed")}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={«() => console.log("clicked")ˇ»}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={ˇ() => console.log("clicked")}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={«() => console.log("clicked")ˇ»}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={() => console.log("clicked"ˇ)}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={«() => console.log("clicked")ˇ»}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={() =ˇ> console.log("clicked")}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={«() => console.log("clicked")ˇ»}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={() => {
                        console.log("cliˇcked");
                      }}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={«() => {
                        console.log("clicked");
                      }ˇ»}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::VisualLine,
    );

    cx.set_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={() => fˇoo()}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a f");
    cx.assert_state(
        indoc! {r#"
                export const MyComponent = () => {
                  return (
                    <div>
                      <div onClick={«() => foo()ˇ»}>Hello world!</div>
                    </div>
                  );
                };
            "#},
        Mode::VisualLine,
    );
}

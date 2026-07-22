use super::*;

#[gpui::test]
async fn test_go_inline_values(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    let variables = [("x", "42"), ("y", "hello")];

    let before = r#"
package main

var globalCounter int = 100

func main() {
    x := 42
    y := "hello"
    z := x + 10
    println(x, y, z)
}
"#
    .unindent();

    let after = r#"
package main

var globalCounter: 100 int = 100

func main() {
    x: 42 := 42
    y := "hello"
    z := x + 10
    println(x, y, z)
}
"#
    .unindent();

    test_inline_values_util(
        &variables,
        &[("globalCounter", "100")],
        &before,
        &after,
        None,
        go_lang(),
        executor,
        cx,
    )
    .await;
}

#[gpui::test]
async fn test_trim_multi_line_inline_value(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    let variables = [("y", "hello\n world")];

    let before = r#"
fn main() {
    let y = "hello\n world";
}
"#
    .unindent();

    let after = r#"
fn main() {
    let y: hello… = "hello\n world";
}
"#
    .unindent();

    test_inline_values_util(
        &variables,
        &[],
        &before,
        &after,
        None,
        rust_lang(),
        executor,
        cx,
    )
    .await;
}

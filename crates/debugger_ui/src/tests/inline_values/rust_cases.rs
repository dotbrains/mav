use super::*;
async fn test_inline_values_example(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    let variables = [("x", "10"), ("y", "20"), ("result", "30")];

    let before = r#"
fn main() {
    let x = 10;
    let y = 20;
    let result = x + y;
    println!("Result: {}", result);
}
"#
    .unindent();

    let after = r#"
fn main() {
    let x: 10 = 10;
    let y: 20 = 20;
    let result: 30 = x: 10 + y: 20;
    println!("Result: {}", result: 30);
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
async fn test_inline_values_with_globals(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    let variables = [("x", "5"), ("y", "10")];

    let before = r#"
static mut GLOBAL_COUNTER: usize = 42;

fn main() {
    let x = 5;
    let y = 10;
    unsafe {
        GLOBAL_COUNTER += 1;
    }
    println!("x={}, y={}, global={}", x, y, unsafe { GLOBAL_COUNTER });
}
"#
    .unindent();

    let after = r#"
static mut GLOBAL_COUNTER: 42: usize = 42;

fn main() {
    let x: 5 = 5;
    let y: 10 = 10;
    unsafe {
        GLOBAL_COUNTER += 1;
    }
    println!("x={}, y={}, global={}", x, y, unsafe { GLOBAL_COUNTER });
}
"#
    .unindent();

    test_inline_values_util(
        &variables,
        &[("GLOBAL_COUNTER", "42")],
        &before,
        &after,
        None,
        rust_lang(),
        executor,
        cx,
    )
    .await;
}

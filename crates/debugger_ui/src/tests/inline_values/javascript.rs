use super::*;

fn javascript_lang() -> Arc<Language> {
    let debug_variables_query = include_str!("../../../grammars/src/javascript/debugger.scm");
    Arc::new(
        Language::new(
            LanguageConfig {
                name: "JavaScript".into(),
                matcher: LanguageMatcher {
                    path_suffixes: vec!["js".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        )
        .with_debug_variables_query(debug_variables_query)
        .unwrap(),
    )
}

fn typescript_lang() -> Arc<Language> {
    let debug_variables_query = include_str!("../../../grammars/src/typescript/debugger.scm");
    Arc::new(
        Language::new(
            LanguageConfig {
                name: "TypeScript".into(),
                matcher: LanguageMatcher {
                    path_suffixes: vec!["ts".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        )
        .with_debug_variables_query(debug_variables_query)
        .unwrap(),
    )
}

fn tsx_lang() -> Arc<Language> {
    let debug_variables_query = include_str!("../../../grammars/src/tsx/debugger.scm");
    Arc::new(
        Language::new(
            LanguageConfig {
                name: "TSX".into(),
                matcher: LanguageMatcher {
                    path_suffixes: vec!["tsx".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        )
        .with_debug_variables_query(debug_variables_query)
        .unwrap(),
    )
}

#[gpui::test]
async fn test_javascript_inline_values(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    let variables = [
        ("x", "10"),
        ("y", "20"),
        ("sum", "30"),
        ("message", "Hello"),
    ];

    let before = r#"
function calculate() {
    const x = 10;
    const y = 20;
    const sum = x + y;
    const message = "Hello";
    console.log(message, "Sum:", sum);
}
"#
    .unindent();

    let after = r#"
function calculate() {
    const x: 10 = 10;
    const y: 20 = 20;
    const sum: 30 = x: 10 + y: 20;
    const message: Hello = "Hello";
    console.log(message, "Sum:", sum);
}
"#
    .unindent();

    test_inline_values_util(
        &variables,
        &[],
        &before,
        &after,
        None,
        javascript_lang(),
        executor,
        cx,
    )
    .await;
}

#[gpui::test]
async fn test_typescript_inline_values(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    let variables = [
        ("count", "42"),
        ("name", "Alice"),
        ("result", "84"),
        ("i", "3"),
    ];

    let before = r#"
function processData(count: number, name: string): number {
    let result = count * 2;
    for (let i = 0; i < 5; i++) {
        console.log(i);
    }
    return result;
}
"#
    .unindent();

    let after = r#"
function processData(count: number, name: string): number {
    let result: 84 = count: 42 * 2;
    for (let i: 3 = 0; i: 3 < 5; i: 3++) {
        console.log(i);
    }
    return result: 84;
}
"#
    .unindent();

    test_inline_values_util(
        &variables,
        &[],
        &before,
        &after,
        None,
        typescript_lang(),
        executor,
        cx,
    )
    .await;
}

#[gpui::test]
async fn test_tsx_inline_values(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    let variables = [("count", "5"), ("message", "Hello React")];

    let before = r#"
const Counter = () => {
    const count = 5;
    const message = "Hello React";
    return (
        <div>
            <p>{message}</p>
            <span>{count}</span>
        </div>
    );
};
"#
    .unindent();

    let after = r#"
const Counter = () => {
    const count: 5 = 5;
    const message: Hello React = "Hello React";
    return (
        <div>
            <p>{message: Hello React}</p>
            <span>{count}</span>
        </div>
    );
};
"#
    .unindent();

    test_inline_values_util(
        &variables,
        &[],
        &before,
        &after,
        None,
        tsx_lang(),
        executor,
        cx,
    )
    .await;
}

#[gpui::test]
async fn test_javascript_arrow_functions(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    let variables = [("x", "42"), ("result", "84")];

    let before = r#"
const double = (x) => {
    const result = x * 2;
    return result;
};
"#
    .unindent();

    let after = r#"
const double = (x) => {
    const result: 84 = x: 42 * 2;
    return result: 84;
};
"#
    .unindent();

    test_inline_values_util(
        &variables,
        &[],
        &before,
        &after,
        None,
        javascript_lang(),
        executor,
        cx,
    )
    .await;
}

#[gpui::test]
async fn test_typescript_for_in_loop(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    let variables = [("key", "name"), ("obj", "{name: 'test'}")];

    let before = r#"
function iterate() {
    const obj = {name: 'test'};
    for (const key in obj) {
        console.log(key);
    }
}
"#
    .unindent();

    let after = r#"
function iterate() {
    const obj: {name: 'test'} = {name: 'test'};
    for (const key: name in obj) {
        console.log(key);
    }
}
"#
    .unindent();

    test_inline_values_util(
        &variables,
        &[],
        &before,
        &after,
        None,
        typescript_lang(),
        executor,
        cx,
    )
    .await;
}

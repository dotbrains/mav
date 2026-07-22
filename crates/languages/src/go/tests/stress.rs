use super::*;

#[gpui::test]
fn test_go_table_test_stress(cx: &mut TestAppContext) {
    let language = go_language();

    let mut entries = String::new();
    for i in 0..100 {
        entries.push_str(&format!(
            "                {{ name: \"case {}\", value: {} }},\n",
            i, i
        ));
    }
    let table_test = format!(
        r#"
    package main

    import "testing"

    func TestStress(t *testing.T) {{
        testCases := []struct{{
            name  string
            value int
        }}{{
{entries}            }}

        for _, tc := range testCases {{
            t.Run(tc.name, func(t *testing.T) {{
                _ = tc.value
            }})
        }}
    }}
    "#,
        entries = entries
    );

    let buffer = cx
        .new(|cx| crate::Buffer::local(table_test.clone(), cx).with_language(language.clone(), cx));
    cx.executor().run_until_parked();

    let runnables: Vec<_> = buffer.update(cx, |buffer, _| {
        let snapshot = buffer.snapshot();
        snapshot.runnable_ranges(0..table_test.len()).collect()
    });

    let tag_strings: Vec<String> = runnables
        .iter()
        .flat_map(|r| &r.runnable.tags)
        .map(|tag| tag.0.to_string())
        .collect();

    let go_table_test_count = tag_strings
        .iter()
        .filter(|&tag| tag == "go-table-test-case")
        .count();

    assert_eq!(
        go_table_test_count, 100,
        "Should emit one go-table-test-case per row (got {}); tree-sitter match_limit overflow has regressed",
        go_table_test_count
    );
}

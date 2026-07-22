use super::*;

#[gpui::test]
fn test_go_table_test_mismatched_field(cx: &mut TestAppContext) {
    let language = go_language();

    let table_test = r#"
    package main

    import "testing"

    func TestMismatchedField(t *testing.T) {
        testCases := []struct{
            name string
        }{
            { name: "test case 1" },
            { name: "test case 2" },
        }

        for _, tc := range testCases {
            t.Run(tc.desc, func(t *testing.T) {
                // test code here
            })
        }
    }
    "#;

    let buffer =
        cx.new(|cx| crate::Buffer::local(table_test, cx).with_language(language.clone(), cx));
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
        go_table_test_count, 0,
        "Should not emit table-test runnables when t.Run uses a missing row field"
    );
}

#[gpui::test]
fn test_go_table_test_slice_picks_correct_field_when_not_first(cx: &mut TestAppContext) {
    // The subtest-name field `name` is declared AFTER `anotherStr`, but `t.Run(tc.name, ...)`
    // still selects on `name`. The resolver must match `@_field_check` text to the right
    // `@_field_name` regardless of source order; "first string field wins" would be a bug.
    let language = go_language();

    let table_test = r#"
    package main

    import "testing"

    func TestExample(t *testing.T) {
        testCases := []struct{
            anotherStr string
            name       string
        }{
            {
                anotherStr: "alpha",
                name:       "case alpha",
            },
            {
                anotherStr: "beta",
                name:       "case beta",
            },
        }

        for _, tc := range testCases {
            t.Run(tc.name, func(t *testing.T) {
                _ = tc.anotherStr
            })
        }
    }
    "#;

    let buffer =
        cx.new(|cx| crate::Buffer::local(table_test, cx).with_language(language.clone(), cx));
    cx.executor().run_until_parked();

    let case_offset = table_test
        .find("anotherStr: \"alpha\"")
        .expect("source should contain the first case body");
    let first_case_runnables: Vec<_> = buffer.update(cx, |buffer, _| {
        let snapshot = buffer.snapshot();
        snapshot.runnable_ranges(case_offset..case_offset).collect()
    });

    let case_names: Vec<_> = first_case_runnables
        .iter()
        .filter(|runnable| {
            runnable
                .runnable
                .tags
                .iter()
                .any(|tag| tag.0 == "go-table-test-case")
        })
        .filter_map(|runnable| runnable.extra_captures.get("_table_test_case_name"))
        .collect();

    assert_eq!(
        case_names,
        vec!["\"case alpha\""],
        "Resolver should pick the field matching `tc.name`, not the first string field"
    );
}

#[gpui::test]
fn test_go_table_test_map_extras_include_case_name(cx: &mut TestAppContext) {
    let language = go_language();

    let table_test = r#"
    package main

    import "testing"

    func TestExample(t *testing.T) {
        testCases := map[string]struct {
            fail bool
        }{
            "test failure": {fail: true},
            "test success": {fail: false},
        }

        for name, tc := range testCases {
            t.Run(name, func(t *testing.T) {
                _ = tc.fail
            })
        }
    }
    "#;

    let buffer =
        cx.new(|cx| crate::Buffer::local(table_test, cx).with_language(language.clone(), cx));
    cx.executor().run_until_parked();

    let all_runnables: Vec<_> = buffer.update(cx, |buffer, _| {
        let snapshot = buffer.snapshot();
        snapshot.runnable_ranges(0..table_test.len()).collect()
    });
    let all_case_names: Vec<_> = all_runnables
        .iter()
        .filter(|runnable| {
            runnable
                .runnable
                .tags
                .iter()
                .any(|tag| tag.0 == "go-table-test-case")
        })
        .filter_map(|runnable| runnable.extra_captures.get("_table_test_case_name"))
        .cloned()
        .collect();
    assert_eq!(
        all_case_names,
        vec!["\"test failure\"", "\"test success\""],
        "Map-based table tests should surface each row's key as `_table_test_case_name`"
    );
}

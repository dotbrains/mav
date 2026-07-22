use super::*;

#[gpui::test]
fn test_go_example_test_detection(cx: &mut TestAppContext) {
    let language = go_language();

    let example_test = r#"
    package main

    import "fmt"

    func Example() {
        fmt.Println("Hello, world!")
        // Output: Hello, world!
    }
    "#;

    let buffer =
        cx.new(|cx| crate::Buffer::local(example_test, cx).with_language(language.clone(), cx));
    cx.executor().run_until_parked();

    let runnables: Vec<_> = buffer.update(cx, |buffer, _| {
        let snapshot = buffer.snapshot();
        snapshot.runnable_ranges(0..example_test.len()).collect()
    });

    let tag_strings: Vec<String> = runnables
        .iter()
        .flat_map(|r| &r.runnable.tags)
        .map(|tag| tag.0.to_string())
        .collect();

    assert!(
        tag_strings.contains(&"go-example".to_string()),
        "Should find go-example tag, found: {:?}",
        tag_strings
    );
}

#[gpui::test]
fn test_go_table_test_slice_detection(cx: &mut TestAppContext) {
    let language = go_language();

    let table_test = r#"
    package main

    import "testing"

    func TestExample(t *testing.T) {
        _ = "some random string"

        testCases := []struct{
            name string
            anotherStr string
        }{
            {
                name: "test case 1",
                anotherStr: "foo",
            },
            {
                name: "test case 2",
                anotherStr: "bar",
            },
            {
                name: "test case 3",
                anotherStr: "baz",
            },
        }

        notATableTest := []struct{
            name string
        }{
            {
                name: "some string",
            },
            {
                name: "some other string",
            },
        }

        for _, tc := range testCases {
            t.Run(tc.name, func(t *testing.T) {
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

    assert!(
        tag_strings.contains(&"go-test".to_string()),
        "Should find go-test tag, found: {:?}",
        tag_strings
    );
    assert!(
        tag_strings.contains(&"go-table-test-case".to_string()),
        "Should find go-table-test-case tag, found: {:?}",
        tag_strings
    );

    let go_test_count = tag_strings.iter().filter(|&tag| tag == "go-test").count();
    let go_table_test_count = tag_strings
        .iter()
        .filter(|&tag| tag == "go-table-test-case")
        .count();

    assert!(
        go_test_count == 1,
        "Should find exactly 1 go-test, found: {}",
        go_test_count
    );
    assert!(
        go_table_test_count == 3,
        "Should find exactly 3 go-table-test-case, found: {}",
        go_table_test_count
    );

    let Some(first_case_offset) = table_test.find("anotherStr: \"foo\"") else {
        panic!("missing first table test case");
    };
    let first_case_offset = first_case_offset + "anotherStr".len();
    let first_case_runnables: Vec<_> = buffer.update(cx, |buffer, _| {
        let snapshot = buffer.snapshot();
        snapshot
            .runnable_ranges(first_case_offset..first_case_offset)
            .collect()
    });
    let table_test_case_names: Vec<_> = first_case_runnables
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
        table_test_case_names,
        vec!["\"test case 1\""],
        "Should only return the table test case containing the requested range"
    );
}

#[gpui::test]
fn test_go_table_test_slice_without_explicit_variable_detection(cx: &mut TestAppContext) {
    let language = go_language();

    let table_test = r#"
    package main

    import "testing"

    func TestExample(t *testing.T) {
        for _, tc := range []struct{
            name string
            anotherStr string
        }{
            {
                name: "test case 1",
                anotherStr: "foo",
            },
            {
                name: "test case 2",
                anotherStr: "bar",
            },
            {
                name: "test case 3",
                anotherStr: "baz",
            },
        } {
            t.Run(tc.name, func(t *testing.T) {
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

    assert!(
        tag_strings.contains(&"go-test".to_string()),
        "Should find go-test tag, found: {:?}",
        tag_strings
    );
    assert!(
        tag_strings.contains(&"go-table-test-case-without-explicit-variable".to_string()),
        "Should find go-table-test-case-without-explicit-variable tag, found: {:?}",
        tag_strings
    );

    let go_test_count = tag_strings.iter().filter(|&tag| tag == "go-test").count();
    let go_table_test_count = tag_strings
        .iter()
        .filter(|&tag| tag == "go-table-test-case-without-explicit-variable")
        .count();

    assert!(
        go_test_count == 1,
        "Should find exactly 1 go-test, found: {}",
        go_test_count
    );
    assert!(
        go_table_test_count == 3,
        "Should find exactly 3 go-table-test-case-without-explicit-variable, found: {}",
        go_table_test_count
    );
}

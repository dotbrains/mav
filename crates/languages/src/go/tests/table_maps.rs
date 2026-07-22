use super::*;

#[gpui::test]
fn test_go_table_test_map_without_explicit_variable_detection(cx: &mut TestAppContext) {
    let language = go_language();

    let table_test = r#"
    package main

    import "testing"

    func TestExample(t *testing.T) {
        for name, tc := range map[string]struct {
      		someStr string
      		fail    bool
       	}{
      		"test failure": {
     			someStr: "foo",
     			fail:    true,
      		},
      		"test success": {
     			someStr: "bar",
     			fail:    false,
      		},
       	} {
            t.Run(name, func(t *testing.T) {
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
        go_table_test_count == 2,
        "Should find exactly 2 go-table-test-case-without-explicit-variable, found: {}",
        go_table_test_count
    );
}

#[gpui::test]
fn test_go_table_test_slice_ignored(cx: &mut TestAppContext) {
    let language = go_language();

    let table_test = r#"
    package main

    func Example() {
        _ = "some random string"

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
        !tag_strings.contains(&"go-test".to_string()),
        "Should find go-test tag, found: {:?}",
        tag_strings
    );
    assert!(
        !tag_strings.contains(&"go-table-test-case".to_string()),
        "Should find go-table-test-case tag, found: {:?}",
        tag_strings
    );
}

#[gpui::test]
fn test_go_table_test_map_detection(cx: &mut TestAppContext) {
    let language = go_language();

    let table_test = r#"
    package main

    import "testing"

    func TestExample(t *testing.T) {
        _ = "some random string"

       	testCases := map[string]struct {
      		someStr string
      		fail    bool
       	}{
      		"test failure": {
     			someStr: "foo",
     			fail:    true,
      		},
      		"test success": {
     			someStr: "bar",
     			fail:    false,
      		},
       	}

       	notATableTest := map[string]struct {
      		someStr string
       	}{
      		"some string": {
     			someStr: "foo",
      		},
      		"some other string": {
     			someStr: "bar",
      		},
       	}

        for name, tc := range testCases {
            t.Run(name, func(t *testing.T) {
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
        go_table_test_count == 2,
        "Should find exactly 2 go-table-test-case, found: {}",
        go_table_test_count
    );
}

#[gpui::test]
fn test_go_table_test_map_ignored(cx: &mut TestAppContext) {
    let language = go_language();

    let table_test = r#"
    package main

    func Example() {
        _ = "some random string"

       	notATableTest := map[string]struct {
      		someStr string
       	}{
      		"some string": {
     			someStr: "foo",
      		},
      		"some other string": {
     			someStr: "bar",
      		},
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
        !tag_strings.contains(&"go-test".to_string()),
        "Should find go-test tag, found: {:?}",
        tag_strings
    );
    assert!(
        !tag_strings.contains(&"go-table-test-case".to_string()),
        "Should find go-table-test-case tag, found: {:?}",
        tag_strings
    );
}

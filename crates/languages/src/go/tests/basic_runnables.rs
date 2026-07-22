use super::*;

#[gpui::test]
fn test_go_test_main_ignored(cx: &mut TestAppContext) {
    let language = go_language();

    let example_test = r#"
    package main

    func TestMain(m *testing.M) {
        os.Exit(m.Run())
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
        !tag_strings.contains(&"go-test".to_string()),
        "Should NOT find go-test tag, found: {:?}",
        tag_strings
    );
}

#[gpui::test]
fn test_testify_suite_detection(cx: &mut TestAppContext) {
    let language = go_language();

    let testify_suite = r#"
    package main

    import (
        "testing"

        "github.com/stretchr/testify/suite"
    )

    type ExampleSuite struct {
        suite.Suite
    }

    func TestExampleSuite(t *testing.T) {
        suite.Run(t, new(ExampleSuite))
    }

    func (s *ExampleSuite) TestSomething_Success() {
        // test code
    }
    "#;

    let buffer =
        cx.new(|cx| crate::Buffer::local(testify_suite, cx).with_language(language.clone(), cx));
    cx.executor().run_until_parked();

    let runnables: Vec<_> = buffer.update(cx, |buffer, _| {
        let snapshot = buffer.snapshot();
        snapshot.runnable_ranges(0..testify_suite.len()).collect()
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
        tag_strings.contains(&"go-testify-suite".to_string()),
        "Should find go-testify-suite tag, found: {:?}",
        tag_strings
    );
}

#[gpui::test]
fn test_go_runnable_detection(cx: &mut TestAppContext) {
    let language = go_language();

    let interpreted_string_subtest = r#"
    package main

    import "testing"

    func TestExample(t *testing.T) {
        t.Run("subtest with double quotes", func(t *testing.T) {
            // test code
        })
    }
    "#;

    let raw_string_subtest = r#"
    package main

    import "testing"

    func TestExample(t *testing.T) {
        t.Run(`subtest with
        multiline
        backticks`, func(t *testing.T) {
            // test code
        })
    }
    "#;

    let buffer = cx.new(|cx| {
        crate::Buffer::local(interpreted_string_subtest, cx).with_language(language.clone(), cx)
    });
    cx.executor().run_until_parked();

    let runnables: Vec<_> = buffer.update(cx, |buffer, _| {
        let snapshot = buffer.snapshot();
        snapshot
            .runnable_ranges(0..interpreted_string_subtest.len())
            .collect()
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
        tag_strings.contains(&"go-subtest".to_string()),
        "Should find go-subtest tag, found: {:?}",
        tag_strings
    );

    let buffer = cx
        .new(|cx| crate::Buffer::local(raw_string_subtest, cx).with_language(language.clone(), cx));
    cx.executor().run_until_parked();

    let runnables: Vec<_> = buffer.update(cx, |buffer, _| {
        let snapshot = buffer.snapshot();
        snapshot
            .runnable_ranges(0..raw_string_subtest.len())
            .collect()
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
        tag_strings.contains(&"go-subtest".to_string()),
        "Should find go-subtest tag, found: {:?}",
        tag_strings
    );
}

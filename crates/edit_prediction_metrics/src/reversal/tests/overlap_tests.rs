use super::*;

#[test]
fn test_reversal_overlap() {
    struct Case {
        name: &'static str,
        original: &'static str,
        current: &'static str,
        predicted: &'static str,
        expected_reversal_chars: usize,
        expected_total_chars: usize,
    }

    let cases = [
        Case {
            name: "user_adds_line_prediction_removes_it",
            original: indoc! {"
                     a
                     b
                     c"},
            current: indoc! {"
                     a
                     new line
                     b
                     c"},
            predicted: indoc! {"
                     a
                     b
                     c"},
            expected_reversal_chars: 9,
            expected_total_chars: 9,
        },
        Case {
            name: "user_deletes_line_prediction_restores_it",
            original: indoc! {"
                     a
                     deleted
                     b"},
            current: indoc! {"
                     a
                     b"},
            predicted: indoc! {"
                     a
                     deleted
                     b"},
            expected_reversal_chars: 8,
            expected_total_chars: 8,
        },
        Case {
            name: "user_deletes_text_prediction_restores_partial",
            original: "hello beautiful world",
            current: "hello world",
            predicted: "hello beautiful world",
            expected_reversal_chars: 10,
            expected_total_chars: 10,
        },
        Case {
            name: "user_deletes_foo_prediction_adds_bar",
            original: "foo",
            current: "",
            predicted: "bar",
            expected_reversal_chars: 0,
            expected_total_chars: 3,
        },
        Case {
            name: "independent_edits_different_locations",
            original: indoc! {"
                     line1
                     line2
                     line3"},
            current: indoc! {"
                     LINE1
                     line2
                     line3"},
            predicted: indoc! {"
                     LINE1
                     line2
                     LINE3"},
            expected_reversal_chars: 0,
            expected_total_chars: 10,
        },
        Case {
            name: "no_history_edits",
            original: "same",
            current: "same",
            predicted: "different",
            expected_reversal_chars: 0,
            expected_total_chars: 13,
        },
        Case {
            name: "user_replaces_text_prediction_reverses",
            original: indoc! {"
                     keep
                     delete_me
                     keep2"},
            current: indoc! {"
                     keep
                     added
                     keep2"},
            predicted: indoc! {"
                     keep
                     delete_me
                     keep2"},
            expected_reversal_chars: 14,
            expected_total_chars: 14,
        },
        Case {
            name: "user_modifies_word_prediction_modifies_differently",
            original: "the quick brown fox",
            current: "the slow brown fox",
            predicted: "the fast brown fox",
            expected_reversal_chars: 4,
            expected_total_chars: 8,
        },
        Case {
            name: "user finishes function name (suffix)",
            original: "",
            current: "epr",
            predicted: "eprintln!()",
            expected_reversal_chars: 0,
            expected_total_chars: 8,
        },
        Case {
            name: "user starts function name (prefix)",
            original: "",
            current: "my_function()",
            predicted: "test_my_function()",
            expected_reversal_chars: 0,
            expected_total_chars: 5,
        },
        Case {
            name: "user types partial, prediction extends in multiple places",
            original: "",
            current: "test_my_function",
            predicted: "a_test_for_my_special_function_plz",
            expected_reversal_chars: 0,
            expected_total_chars: 18,
        },
        // Edge cases for subsequence matching
        Case {
            name: "subsequence with interleaved underscores",
            original: "",
            current: "a_b_c",
            predicted: "_a__b__c__",
            expected_reversal_chars: 0,
            expected_total_chars: 5,
        },
        Case {
            name: "not a subsequence - different characters",
            original: "",
            current: "abc",
            predicted: "xyz",
            expected_reversal_chars: 3,
            expected_total_chars: 6,
        },
        Case {
            name: "not a subsequence - wrong order",
            original: "",
            current: "abc",
            predicted: "cba",
            expected_reversal_chars: 3,
            expected_total_chars: 6,
        },
        Case {
            name: "partial subsequence - only some chars match",
            original: "",
            current: "abcd",
            predicted: "axbx",
            expected_reversal_chars: 4,
            expected_total_chars: 8,
        },
        // Common completion patterns
        Case {
            name: "completing a method call",
            original: "",
            current: "vec.pu",
            predicted: "vec.push(item)",
            expected_reversal_chars: 0,
            expected_total_chars: 8,
        },
        Case {
            name: "completing an import statement",
            original: "",
            current: "use std::col",
            predicted: "use std::collections::HashMap",
            expected_reversal_chars: 0,
            expected_total_chars: 17,
        },
        Case {
            name: "completing a struct field",
            original: "",
            current: "name: St",
            predicted: "name: String",
            expected_reversal_chars: 0,
            expected_total_chars: 4,
        },
        Case {
            name: "prediction replaces with completely different text",
            original: "",
            current: "hello",
            predicted: "world",
            expected_reversal_chars: 5,
            expected_total_chars: 10,
        },
        Case {
            name: "empty prediction removes user text",
            original: "",
            current: "mistake",
            predicted: "",
            expected_reversal_chars: 7,
            expected_total_chars: 7,
        },
        Case {
            name: "fixing typo is not reversal",
            original: "",
            current: "<dv",
            predicted: "<div>",
            expected_reversal_chars: 0,
            expected_total_chars: 2,
        },
        Case {
            name: "infix insertion not reversal",
            original: indoc! {"
                     from my_project import Foo
                 "},
            current: indoc! {"
                     ifrom my_project import Foo
                 "},
            predicted: indoc! {"
                     import
                     from my_project import Foo
                 "},
            expected_reversal_chars: 0,
            expected_total_chars: 6,
        },
        Case {
            name: "non-word based reversal",
            original: "from",
            current: "ifrom",
            predicted: "from",
            expected_reversal_chars: 1,
            expected_total_chars: 1,
        },
        Case {
            name: "multiple insertions no reversal",
            original: "print(\"Hello, World!\")",
            current: "sys.(\"Hello, World!\")",
            predicted: "sys.stdout.write(\"Hello, World!\\n\")",
            expected_reversal_chars: 0,
            expected_total_chars: 14,
        },
    ];

    for case in &cases {
        let overlap = compute_reversal_overlap(case.original, case.current, case.predicted);
        assert_eq!(
            overlap.chars_reversing_user_edits, case.expected_reversal_chars,
            "Test '{}': expected {} reversal chars, got {}",
            case.name, case.expected_reversal_chars, overlap.chars_reversing_user_edits
        );
        assert_eq!(
            overlap.total_chars_in_prediction, case.expected_total_chars,
            "Test '{}': expected {} total chars, got {}",
            case.name, case.expected_total_chars, overlap.total_chars_in_prediction
        );
    }
}

use super::*;

#[test]
fn test_unicode_reversal_overlap() {
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
            name: "unicode_extension_cjk",
            original: "",
            current: "日",       // 1 char
            predicted: "日本語", // 3 chars, adds 2 chars
            expected_reversal_chars: 0,
            expected_total_chars: 2, // "本語" = 2 chars added
        },
        Case {
            name: "unicode_extension_emoji",
            original: "",
            current: "🎉",       // 1 char
            predicted: "🎉🎊🎈", // 3 chars, adds 2 chars
            expected_reversal_chars: 0,
            expected_total_chars: 2, // "🎊🎈" = 2 chars added
        },
        Case {
            name: "unicode_deletion_restored",
            original: "héllo wörld",    // 11 chars
            current: "héllo",           // 5 chars
            predicted: "héllo wörld",   // restores " wörld" = 6 chars
            expected_reversal_chars: 6, // LCS(" wörld", " wörld") = 6 chars
            expected_total_chars: 6,
        },
        Case {
            name: "unicode_addition_reversed",
            original: "café",           // 4 chars
            current: "café latté",      // 10 chars, added " latté" = 6 chars
            predicted: "café",          // removes " latté"
            expected_reversal_chars: 6, // 6 chars removed
            expected_total_chars: 6,
        },
        Case {
            name: "mixed_ascii_unicode",
            original: "",
            current: "test日本",         // 6 chars
            predicted: "test日本語です", // 9 chars
            expected_reversal_chars: 0,
            expected_total_chars: 3, // 3 new chars after subsequence normalization
        },
        Case {
            name: "unicode_replacement_not_subsequence",
            original: "",
            current: "日本",            // 2 chars
            predicted: "中国",          // 2 chars, different
            expected_reversal_chars: 2, // removes "日本" = 2 chars
            expected_total_chars: 4,    // 2 removed + 2 added
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

#[test]
fn test_compute_lcs_length() {
    assert_eq!(compute_lcs_length("", ""), 0);
    assert_eq!(compute_lcs_length("abc", ""), 0);
    assert_eq!(compute_lcs_length("", "abc"), 0);
    assert_eq!(compute_lcs_length("abc", "abc"), 3);
    assert_eq!(compute_lcs_length("abc", "def"), 0);
    assert_eq!(compute_lcs_length("abcdef", "ace"), 3);
    assert_eq!(compute_lcs_length("AGGTAB", "GXTXAYB"), 4);
    assert_eq!(compute_lcs_length("日本語", "日語"), 2);
}

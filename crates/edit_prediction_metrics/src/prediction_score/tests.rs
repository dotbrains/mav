use super::*;

#[test]
fn test_kept_rate_is_computed_when_best_delta_chr_f_score_is_zero() {
    let original_text = "";
    let actual_patch = "--- a/file.txt\n+++ b/file.txt\n@@ -0,0 +1 @@\n+bbbbbb\n";
    let expected_patch = "--- a/file.txt\n+++ b/file.txt\n@@ -0,0 +1 @@\n+cccccc\n";
    let expected_patches = [PreparedExpectedPatch {
        patch: expected_patch.to_string(),
        text: "cccccc".to_string(),
        cursor_editable_region_offset: None,
    }];

    let score = score_prediction(PredictionScoringInput {
        original_text,
        expected_patches: &expected_patches,
        actual_patch: Some(actual_patch),
        actual_cursor: None,
        reversal_context: None,
        cumulative_logprob: None,
        avg_logprob: None,
        context: None,
    });

    assert_eq!(score.delta_chr_f, 0.0);
    assert_eq!(score.kept_rate, Some(0.0));
}

#[test]
fn test_scores_related_file_patch_against_context_document() {
    let original_text = "fn main() {}\n";
    let expected_patch = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -11,3 +11,3 @@\n fn value() {\n-    1\n+    2\n }\n";
    let actual_patch = "--- a/project/src/lib.rs\n+++ b/project/src/lib.rs\n@@ -11,3 +11,3 @@\n fn value() {\n-    1\n+    2\n }\n";
    let expected_patches = [PreparedExpectedPatch {
        patch: expected_patch.to_string(),
        text: original_text.to_string(),
        cursor_editable_region_offset: None,
    }];
    let context = [
        Excerpt {
            path: "src/main.rs".to_string(),
            row_range: 0..1,
            content: original_text.to_string(),
        },
        Excerpt {
            path: "src/lib.rs".to_string(),
            row_range: 10..13,
            content: "fn value() {\n    1\n}\n".to_string(),
        },
    ];

    let score = score_prediction(PredictionScoringInput {
        original_text,
        expected_patches: &expected_patches,
        actual_patch: Some(actual_patch),
        actual_cursor: None,
        reversal_context: None,
        cumulative_logprob: None,
        avg_logprob: None,
        context: Some(&context),
    });

    assert_eq!(score.delta_chr_f, 100.0);
    assert_eq!(score.exact_lines_tp, 2);
    assert_eq!(score.jump_location.unwrap().files_f1, 1.0);
}

#[test]
fn test_missing_related_file_prediction_counts_as_false_negative() {
    let original_text = "fn main() {}\n";
    let expected_patch = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -11,3 +11,3 @@\n fn value() {\n-    1\n+    2\n }\n";
    let expected_patches = [PreparedExpectedPatch {
        patch: expected_patch.to_string(),
        text: original_text.to_string(),
        cursor_editable_region_offset: None,
    }];
    let context = [Excerpt {
        path: "src/lib.rs".to_string(),
        row_range: 10..13,
        content: "fn value() {\n    1\n}\n".to_string(),
    }];

    let score = score_prediction(PredictionScoringInput {
        original_text,
        expected_patches: &expected_patches,
        actual_patch: None,
        actual_cursor: None,
        reversal_context: None,
        cumulative_logprob: None,
        avg_logprob: None,
        context: Some(&context),
    });

    assert!(score.delta_chr_f < 100.0);
    assert_eq!(score.exact_lines_fn, 2);
    let location = score.jump_location.unwrap();
    assert_eq!(location.files_fn, 1);
    assert_eq!(location.lines_fn, 1);
}

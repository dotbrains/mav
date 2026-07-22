use super::*;

#[test]
fn test_compute_prediction_reversal_ratio_full_file() {
    let prompt_inputs = make_test_prompt_inputs(
        indoc! {"
                 line1
                 user_added
                 line2
             "},
        vec![Arc::new(zeta_prompt::Event::BufferChange {
            path: Arc::from(Path::new("src/test.rs")),
            old_path: Arc::from(Path::new("src/test.rs")),
            diff: indoc! {"
                     @@ -1,2 +1,3 @@
                      line1
                     +user_added
                      line2
                 "}
            .into(),
            old_range: 0..0,
            new_range: 0..0,
            predicted: false,
            in_open_source_repo: false,
        })],
        None,
    );

    let predicted = indoc! {"
             line1
             line2
         "};
    let ratio =
        compute_prediction_reversal_ratio(&prompt_inputs, predicted, Path::new("src/test.rs"));

    assert!(
        ratio > 0.9,
        "Expected high reversal ratio when prediction removes user addition, got {}",
        ratio
    );
}

#[test]
fn test_compute_prediction_reversal_ratio_with_excerpt() {
    let prompt_inputs = make_test_prompt_inputs(
        indoc! {"
                 line10
                 user_added
                 line11
             "},
        vec![Arc::new(zeta_prompt::Event::BufferChange {
            path: Arc::from(Path::new("src/test.rs")),
            old_path: Arc::from(Path::new("src/test.rs")),
            diff: indoc! {"
                     @@ -10,2 +10,3 @@
                      line10
                     +user_added
                      line11
                 "}
            .into(),
            old_range: 0..0,
            new_range: 0..0,
            predicted: false,
            in_open_source_repo: false,
        })],
        Some(10),
    );

    let predicted = indoc! {"
             line10
             line11
         "};
    let ratio =
        compute_prediction_reversal_ratio(&prompt_inputs, predicted, Path::new("src/test.rs"));

    assert!(
        ratio > 0.9,
        "Expected high reversal ratio for excerpt-aware computation, got {}",
        ratio
    );
}

#[test]
fn test_compute_prediction_reversal_ratio_no_history() {
    let prompt_inputs = make_test_prompt_inputs(
        indoc! {"
                 original content
             "},
        vec![],
        None,
    );

    let predicted = indoc! {"
             completely different
         "};
    let ratio =
        compute_prediction_reversal_ratio(&prompt_inputs, predicted, Path::new("src/test.rs"));

    assert_eq!(
        ratio, 0.0,
        "Expected zero reversal ratio with no edit history"
    );
}

#[test]
fn test_compute_prediction_reversal_ratio_path_filtering() {
    let prompt_inputs = make_test_prompt_inputs(
        indoc! {"
                 line1
                 user_added
                 line2
             "},
        vec![Arc::new(zeta_prompt::Event::BufferChange {
            path: Arc::from(Path::new("src/other.rs")),
            old_path: Arc::from(Path::new("src/other.rs")),
            diff: indoc! {"
                     @@ -1,2 +1,3 @@
                      line1
                     +user_added
                      line2
                 "}
            .into(),
            old_range: 0..0,
            new_range: 0..0,
            predicted: false,
            in_open_source_repo: false,
        })],
        None,
    );

    let predicted = indoc! {"
             line1
             line2
         "};
    let ratio =
        compute_prediction_reversal_ratio(&prompt_inputs, predicted, Path::new("src/test.rs"));

    assert_eq!(
        ratio, 0.0,
        "Expected zero reversal when edit history is for different file"
    );
}

#[test]
fn test_compute_prediction_reversal_ratio_lenient_fallback() {
    let prompt_inputs = make_test_prompt_inputs(
        indoc! {"
                 actual_line1
                 user_added
                 actual_line2
             "},
        vec![Arc::new(zeta_prompt::Event::BufferChange {
            path: Arc::from(Path::new("src/test.rs")),
            old_path: Arc::from(Path::new("src/test.rs")),
            diff: indoc! {"
                     @@ -1,2 +1,3 @@
                      wrong_context
                     +user_added
                      more_wrong
                 "}
            .into(),
            old_range: 0..0,
            new_range: 0..0,
            predicted: false,
            in_open_source_repo: false,
        })],
        None,
    );

    let predicted = indoc! {"
             actual_line1
             actual_line2
         "};
    let ratio =
        compute_prediction_reversal_ratio(&prompt_inputs, predicted, Path::new("src/test.rs"));

    assert!(
        ratio >= 0.0 && ratio <= 1.0,
        "Ratio should be valid even with lenient fallback, got {}",
        ratio
    );
}

#[test]
fn test_excerpt_aware_reversal_error_recovery() {
    let diffs = vec![indoc! {"
             @@ -1,2 +1,3 @@
              nonexistent_context
             +added
              more_nonexistent
         "}];
    let excerpt_content = indoc! {"
             completely
             different
             content
         "};
    let predicted_content = indoc! {"
             completely
             modified
             content
         "};

    let overlap =
        compute_excerpt_aware_reversal_overlap(&diffs, excerpt_content, 0, predicted_content);

    assert!(
        overlap.ratio() >= 0.0 && overlap.ratio() <= 1.0,
        "Should handle failed diff application gracefully"
    );
}

#[test]
fn test_only_most_recent_edit_tracked() {
    let prompt_inputs = make_test_prompt_inputs(
        indoc! {"
                 line1
                 first_add
                 second_add
                 line2
             "},
        vec![
            Arc::new(zeta_prompt::Event::BufferChange {
                path: Arc::from(Path::new("src/test.rs")),
                old_path: Arc::from(Path::new("src/test.rs")),
                diff: indoc! {"
                         @@ -1,2 +1,3 @@
                          line1
                         +first_add
                          line2
                     "}
                .into(),
                old_range: 0..0,
                new_range: 0..0,
                predicted: false,
                in_open_source_repo: false,
            }),
            Arc::new(zeta_prompt::Event::BufferChange {
                path: Arc::from(Path::new("src/test.rs")),
                old_path: Arc::from(Path::new("src/test.rs")),
                diff: indoc! {"
                         @@ -2,2 +2,3 @@
                          first_add
                         +second_add
                          line2
                     "}
                .into(),
                old_range: 0..0,
                new_range: 0..0,
                predicted: false,
                in_open_source_repo: false,
            }),
        ],
        None,
    );

    let predicted = indoc! {"
             line1
             first_add
             line2
         "};
    let ratio =
        compute_prediction_reversal_ratio(&prompt_inputs, predicted, Path::new("src/test.rs"));

    assert!(
        ratio > 0.9,
        "Expected high reversal ratio when prediction exactly reverses the most recent edit, got {}",
        ratio
    );
}

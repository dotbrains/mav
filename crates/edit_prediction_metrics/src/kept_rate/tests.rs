
use super::*;
use indoc::indoc;

#[test]
fn test_lcs_keep_masks() {
    let (a_mask, b_mask) = lcs_keep_masks(&["a", "b", "c", "d", "e"], &["a", "c", "e"]);
    assert_eq!(a_mask, vec![true, false, true, false, true]);
    assert_eq!(b_mask, vec![true, true, true]);

    let (a_mask, b_mask) = lcs_keep_masks(&[], &["x"]);
    assert!(a_mask.is_empty());
    assert_eq!(b_mask, vec![false]);
}

#[test]
fn test_lcs_keep_masks_matches_historical_one_sided_masks() {
    let a = ["x", "a", "x", "b"];
    let b = ["a", "x", "b", "x"];
    let (a_mask, b_mask) = lcs_keep_masks(&a, &b);
    assert_eq!(a_mask, lcs_keep_mask(&a, &b));
    assert_eq!(b_mask, lcs_keep_mask(&b, &a));
}

#[test]
fn test_rate_extremes() {
    let no_change = compute_kept_rate("foo bar", "foo bar", "foo bar");
    assert!((no_change.kept_rate - 1.0).abs() < 1e-6);
    assert!((no_change.recall_rate - 1.0).abs() < 1e-6);
    assert_eq!(no_change.candidate_new_chars, 0);
    assert!(
        no_change
            .token_annotations
            .iter()
            .all(|&annotation| annotation == TokenAnnotation::Context)
    );

    let accepted = compute_kept_rate("old", "new", "new");
    assert!((accepted.kept_rate - 1.0).abs() < 1e-6);
    assert!((accepted.recall_rate - 1.0).abs() < 1e-6);

    let discarded = compute_kept_rate("old", "old", "new");
    assert!((discarded.kept_rate - 0.0).abs() < 1e-6);
    assert!((discarded.recall_rate - 0.0).abs() < 1e-6);
}

#[test]
fn test_pure_addition() {
    let kept = compute_kept_rate("", "brand new line\n", "brand new line\n");
    assert_eq!(kept.kept_chars, kept.candidate_new_chars);
    assert!(
        kept.token_annotations
            .iter()
            .all(|&annotation| annotation == TokenAnnotation::Kept)
    );

    let discarded = compute_kept_rate("", "brand new line\n", "something completely different\n");
    assert!(discarded.kept_chars < discarded.candidate_new_chars);
}

#[test]
fn test_decoy_when_base_excluded() {
    let base = "    decoy.when(mock_sync_hardware_api.sp()).then_return(SpeedStatus.IDLE)\n";
    let candidate =
        "    decoy.when(mock_sync_module_hardware.speed_status).then_return(SpeedStatus.IDLE)\n";
    let reference =
        "    decoy.when(mock_sync_module_hardware.speed_status).then_return(SpeedStatus.IDLE)\n";
    let result = compute_kept_rate(base, candidate, reference);
    let expected_new = "mock_sync_module_hardware".len() + "speed_status".len();
    assert_eq!(result.candidate_new_chars, expected_new);
    assert!(result.correctly_deleted_chars > 0);
    assert!((result.kept_rate - 1.0).abs() < 1e-6);
    assert!((result.recall_rate - 1.0).abs() < 1e-6);
}

#[test]
fn test_missing_deletion() {
    let base = indoc! {"
            fn example() {
                epr
        "};
    let candidate = indoc! {r#"
            fn example() {
                epr
            eprintln!("");
        "#};
    let reference = indoc! {r#"
            fn example() {
            eprintln!("");
        "#};

    let result = compute_kept_rate(base, candidate, reference);
    assert!((result.kept_rate - (14.0 / 15.0)).abs() < 1e-6);
    assert_eq!(result.kept_chars, 14);
    assert_eq!(result.discarded_chars, 1);
}

#[test]
fn test_empty_prediction() {
    let result = compute_kept_rate("old line\n", "", "new line\n");
    assert_eq!(result.candidate_new_chars, 0);
    assert!(result.candidate_deleted_chars > 0);
    assert!(result.correctly_deleted_chars > 0);
    assert!(result.correctly_deleted_chars < result.candidate_deleted_chars);
    assert!(result.kept_rate > 0.0 && result.kept_rate < 1.0);
    assert!(result.recall_rate > 0.0 && result.recall_rate < 1.0);
}

#[test]
fn test_partial_kept() {
    let result = compute_kept_rate("old\n", "alpha\nbeta\ngamma\n", "alpha\ngamma\n");
    assert!(result.kept_chars > 0);
    assert!(result.discarded_chars > 0);
    assert!(result.kept_rate > 0.0 && result.kept_rate < 1.0);
}

#[test]
fn test_bails_for_dirty_final() {
    let base = indoc! {"
            fn example() {
                work();
            }
        "};
    let candidate = indoc! {"
            fn example() {
                work();
                predicted();
            }
        "};
    let reference = format!(
        "fn example() {{\n    work();\n    {}\n}}\n",
        "settled();\n    ".repeat(MAX_DIRTY_LENGTH_DELTA_CHARS / 8 + 64)
    );

    let result = compute_kept_rate(base, candidate, &reference);
    assert_eq!(result.kept_rate, 0.0);
    assert_eq!(result.recall_rate, 0.0);
    assert_eq!(result.kept_chars, 0);
    assert_eq!(result.discarded_chars, result.candidate_new_chars);
}

#[test]
fn test_eprintln_token_alignment() {
    let base = indoc! {"
            fn example() {
                epr
        "};
    let candidate = indoc! {r#"
            fn example() {
                eprintln!("hello world!");
        "#};
    let reference = indoc! {r#"
            fn example() {
                eprintln!("");
        "#};

    let result = compute_kept_rate(base, candidate, reference);
    assert!(result.discarded_chars > 0);
    assert!(result.kept_chars > 0);
    assert!(result.kept_rate > 0.0 && result.kept_rate < 1.0);
    assert_eq!(result.kept_chars, 14);
    assert_eq!(result.discarded_chars, 12);
}

#[test]
fn test_kept_rate_treats_unchanged_stale_text_as_context() {
    let base = indoc! {"
            a=fomr
            b=old
        "};
    let candidate = indoc! {"
            a=formula;
            b=old
        "};
    let reference = indoc! {"
            a=formula;
            b=new
        "};

    let result = compute_kept_rate(base, candidate, reference);
    let candidate_tokens = tokenize(candidate);

    assert_eq!(result.candidate_new_chars, "formula".len() + ";".len());
    assert_eq!(result.kept_chars, "formula".len() + ";".len());
    assert_eq!(result.discarded_chars, 0);
    assert_eq!(result.candidate_deleted_chars, "fomr".len());
    assert_eq!(result.correctly_deleted_chars, "fomr".len());
    assert!((result.kept_rate - 1.0).abs() < 1e-6);
    assert!((result.recall_rate - (2.0 / 3.0)).abs() < 1e-6);

    let old_index = candidate_tokens
        .iter()
        .position(|&token| token == "old")
        .expect("old token not found");
    assert_eq!(
        result.token_annotations[old_index],
        TokenAnnotation::Context
    );
}

#[test]
fn test_annotations_rename() {
    let base = "    foo(old_name)\n";
    let candidate = "    foo(new_name)\n";
    let reference = "    foo(new_name)\n";
    let result = compute_kept_rate(base, candidate, reference);

    assert_eq!(result.candidate_new_chars, "new_name".len());
    assert_eq!(result.candidate_deleted_chars, "old_name".len());
    assert_eq!(result.reference_deleted_chars, "old_name".len());
    assert_eq!(result.correctly_deleted_chars, "old_name".len());
    assert!((result.recall_rate - 1.0).abs() < 1e-6);
    assert_eq!(result.token_annotations.len(), tokenize(candidate).len());

    for (&token, &annotation) in tokenize(candidate).iter().zip(&result.token_annotations) {
        if matches!(token, "new" | "_" | "name") {
            assert_eq!(annotation, TokenAnnotation::Kept);
        } else {
            assert_eq!(annotation, TokenAnnotation::Context);
        }
    }
}

#[test]
fn test_annotations_eprintln_coloring() {
    let base = indoc! {"
            fn example() {
                epr
        "};
    let candidate = indoc! {r#"
            fn example() {
                eprintln!("hello world!");
        "#};
    let reference = indoc! {r#"
            fn example() {
                eprintln!("");
        "#};
    let result = compute_kept_rate(base, candidate, reference);
    let candidate_tokens = tokenize(candidate);

    let eprintln_index = candidate_tokens
        .iter()
        .position(|&token| token == "eprintln")
        .expect("eprintln token not found");

    for annotation in &result.token_annotations[..eprintln_index] {
        assert_eq!(*annotation, TokenAnnotation::Context);
    }

    assert_eq!(
        &result.token_annotations[eprintln_index..=eprintln_index + 10],
        &[
            TokenAnnotation::Kept,
            TokenAnnotation::Kept,
            TokenAnnotation::Kept,
            TokenAnnotation::Kept,
            TokenAnnotation::Discarded,
            TokenAnnotation::Discarded,
            TokenAnnotation::Discarded,
            TokenAnnotation::Discarded,
            TokenAnnotation::Kept,
            TokenAnnotation::Kept,
            TokenAnnotation::Kept,
        ]
    );
    assert_eq!(
        result.token_annotations.last(),
        Some(&TokenAnnotation::Context)
    );
}

#[test]
fn test_repetitive_tokens_remain_discarded() {
    let base = "foo + foo + foo + foo + foo\n".repeat(16);
    let candidate = "foo + foo + prediction_token + foo + foo\n".repeat(16);
    let reference = "foo + foo + kept_token + foo + foo\n".repeat(16);
    let result = compute_kept_rate(&base, &candidate, &reference);

    assert_eq!(result.kept_chars, 0);
    assert_eq!(result.correctly_deleted_chars, "foo".len() * 16);
    assert_eq!(result.discarded_chars, result.candidate_new_chars);
    assert_eq!(result.candidate_new_chars, "prediction_token".len() * 16);
    assert!(result.kept_rate > 0.0);
    assert!(result.recall_rate > 0.0);
}

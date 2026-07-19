use super::*;
use indoc::indoc;

fn excerpt(path: &str, row_range: Range<u32>) -> Excerpt {
    Excerpt {
        path: path.to_string(),
        row_range,
        content: String::new(),
    }
}

#[test]
fn deletion_counts_deleted_old_line_as_true_positive() {
    let patch = indoc! {"
            --- a/src/main.rs
            +++ b/src/main.rs
            @@ -2,1 +2,0 @@
            -let value = 1;
        "};

    let score = editable_context_coverage(patch, &[excerpt("src/main.rs", 1..2)]);

    assert_eq!(score, EditableContextCoverage::new(1, 0, 0, 1, 0, 0));
}

#[test]
fn retrieved_lines_inside_relevance_window_are_true_positives() {
    let patch = indoc! {"
            --- a/src/main.rs
            +++ b/src/main.rs
            @@ -4,1 +4,0 @@
            -let value = 4;
        "};

    let score = editable_context_coverage(
        patch,
        &[excerpt("src/main.rs", 0..1), excerpt("src/main.rs", 3..4)],
    );

    assert_eq!(score, EditableContextCoverage::new(2, 0, 0, 1, 0, 0));
}

#[test]
fn replacement_counts_deleted_old_line_without_addition_anchor() {
    let patch = indoc! {"
            --- a/src/main.rs
            +++ b/src/main.rs
            @@ -1,3 +1,3 @@
             fn main() {
            -    let value = 1;
            +    let value = 2;
             }
        "};

    let score = editable_context_coverage(patch, &[excerpt("src/main.rs", 1..2)]);

    assert_eq!(score, EditableContextCoverage::new(1, 0, 0, 1, 0, 0));
}

#[test]
fn pure_insertion_counts_previous_and_next_old_lines_as_expected_context() {
    let patch = indoc! {"
            --- a/src/main.rs
            +++ b/src/main.rs
            @@ -1,2 +1,3 @@
             line 1
            +inserted
             line 2
        "};

    let score = editable_context_coverage(patch, &[excerpt("src/main.rs", 0..1)]);

    assert_eq!(score, EditableContextCoverage::new(1, 0, 1, 1, 0, 0));
}

#[test]
fn pure_insertion_at_file_boundary_uses_available_neighboring_context() {
    let patch = indoc! {"
            --- a/src/main.rs
            +++ b/src/main.rs
            @@ -1,1 +1,2 @@
            +inserted
             line 1
        "};

    let score = editable_context_coverage(patch, &[excerpt("src/main.rs", 0..1)]);

    assert_eq!(score, EditableContextCoverage::new(1, 0, 0, 1, 0, 0));
}

#[test]
fn counts_false_negatives_and_file_false_positives() {
    let patch = indoc! {"
            --- a/src/main.rs
            +++ b/src/main.rs
            @@ -1,3 +1,3 @@
            -let first = 1;
            +let first = 2;
             let middle = 3;
            -let last = 4;
            +let last = 5;
        "};

    let score = editable_context_coverage(
        patch,
        &[excerpt("src/main.rs", 0..1), excerpt("src/lib.rs", 0..1)],
    );

    assert_eq!(score, EditableContextCoverage::new(1, 1, 1, 1, 1, 0));
}

#[test]
fn overlapping_excerpts_are_counted_once() {
    let patch = indoc! {"
            --- a/src/main.rs
            +++ b/src/main.rs
            @@ -2,1 +2,0 @@
            -let value = 1;
        "};

    let score = editable_context_coverage(
        patch,
        &[excerpt("src/main.rs", 0..2), excerpt("src/main.rs", 1..3)],
    );

    assert_eq!(score, EditableContextCoverage::new(3, 0, 0, 1, 0, 0));
}

#[test]
fn nearby_lines_do_not_satisfy_line_recall_without_exact_anchor_lines() {
    let patch = indoc! {"
            --- a/src/main.rs
            +++ b/src/main.rs
            @@ -1,2 +1,3 @@
             line 1
            +inserted
             line 2
        "};

    let score = editable_context_coverage(patch, &[excerpt("src/main.rs", 2..3)]);

    assert_eq!(score.lines_tp, 1);
    assert_eq!(score.lines_fp, 0);
    assert_eq!(score.lines_fn, 2);
    assert_eq!(score.lines_precision, 1.0);
    assert_eq!(score.lines_recall, 0.0);
    assert_eq!(score.lines_f1, 0.0);
}

#[test]
fn retrieved_lines_outside_relevance_window_are_false_positives() {
    let patch = indoc! {"
            --- a/src/main.rs
            +++ b/src/main.rs
            @@ -1,1 +1,0 @@
            -line 1
        "};

    let score = editable_context_coverage(patch, &[excerpt("src/main.rs", 21..22)]);

    assert_eq!(score, EditableContextCoverage::new(0, 1, 1, 1, 0, 0));
}

#[test]
fn empty_patch_with_no_context_has_perfect_f1() {
    let score = editable_context_coverage(
        indoc! {"
        "},
        &[],
    );

    assert_eq!(score, EditableContextCoverage::new(0, 0, 0, 0, 0, 0));
}

#[test]
fn patch_location_match_counts_file_and_nearby_line_matches() {
    let expected = indoc! {"
            --- a/src/main.rs
            +++ b/src/main.rs
            @@ -50,1 +50,1 @@
            -let value = 1;
            +let value = 2;
        "};
    let actual = indoc! {"
            --- a/src/main.rs
            +++ b/src/main.rs
            @@ -55,1 +55,1 @@
            -let value = 1;
            +let value = 2;
        "};

    let score = patch_location_match(expected, actual);

    assert_eq!(score, PatchLocationMatch::new(1, 0, 0, 1, 0, 0));
}

#[test]
fn patch_location_match_counts_missing_and_extra_files() {
    let expected = indoc! {"
            --- a/src/main.rs
            +++ b/src/main.rs
            @@ -1,1 +1,1 @@
            -let value = 1;
            +let value = 2;
        "};
    let actual = indoc! {"
            --- a/src/lib.rs
            +++ b/src/lib.rs
            @@ -1,1 +1,1 @@
            -let value = 1;
            +let value = 2;
        "};

    let score = patch_location_match(expected, actual);

    assert_eq!(score, PatchLocationMatch::new(0, 1, 1, 0, 1, 1));
}

#[test]
fn patch_location_match_normalizes_project_prefixed_actual_path() {
    let expected = indoc! {"
            --- a/src/main.rs
            +++ b/src/main.rs
            @@ -1,1 +1,1 @@
            -let value = 1;
            +let value = 2;
        "};
    let actual = indoc! {"
            --- a/project/src/main.rs
            +++ b/project/src/main.rs
            @@ -1,1 +1,1 @@
            -let value = 1;
            +let value = 2;
        "};

    let score = patch_location_match(expected, actual);

    assert_eq!(score, PatchLocationMatch::new(1, 0, 0, 1, 0, 0));
}

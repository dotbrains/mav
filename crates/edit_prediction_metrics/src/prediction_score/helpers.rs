use super::{
    ActualPredictionCursor, ExpectedPatchScore, PredictionScoringInput, PreparedExpectedPatch,
    score_against_expected_patch,
};

pub(super) fn score_against_no_expected_patch(
    input: PredictionScoringInput<'_>,
    actual_patch: &str,
) -> ExpectedPatchScore {
    let expected = PreparedExpectedPatch {
        patch: String::new(),
        text: input.original_text.to_string(),
        cursor_editable_region_offset: None,
    };
    score_against_expected_patch(input, &expected, actual_patch)
}

pub(super) fn compute_cursor_metrics(
    expected_cursor_editable_region_offset: Option<usize>,
    actual_cursor: Option<ActualPredictionCursor>,
) -> (Option<usize>, Option<bool>) {
    match (expected_cursor_editable_region_offset, actual_cursor) {
        (Some(expected), Some(actual)) => {
            let distance = expected.abs_diff(actual.editable_region_offset.unwrap_or_default());
            let exact_match = distance == 0;
            (Some(distance), Some(exact_match))
        }
        (None, None) => (None, None),
        (Some(_), None) | (None, Some(_)) => (None, Some(false)),
    }
}

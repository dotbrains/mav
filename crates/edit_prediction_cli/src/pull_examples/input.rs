use super::*;

/// Parse an input token of the form `captured-after:{timestamp}`.
pub fn parse_captured_after_input(input: &str) -> Option<&str> {
    input.strip_prefix("captured-after:")
}

/// Parse an input token of the form `accepted-after:{timestamp}`.
pub fn parse_accepted_after_input(input: &str) -> Option<&str> {
    input.strip_prefix("accepted-after:")
}

/// Parse an input token of the form `rejected-after:{timestamp}`.
pub fn parse_rejected_after_input(input: &str) -> Option<(bool, &str)> {
    if let Some(timestamp) = input.strip_prefix("rejected-after:") {
        Some((false, timestamp))
    } else if let Some(timestamp) = input.strip_prefix("explicitly-rejected-after:") {
        Some((true, timestamp))
    } else {
        None
    }
}

/// Parse an input token of the form `requested-after:{timestamp}`.
pub fn parse_requested_after_input(input: &str) -> Option<&str> {
    input.strip_prefix("requested-after:")
}

/// Parse an input token of the form `settled-after:{timestamp}`.
pub fn parse_settled_after_input(input: &str) -> Option<&str> {
    input.strip_prefix("settled-after:")
}

/// Parse an input token of the form `rated-after:{timestamp}`, `rated-positive-after:{timestamp}`,
/// or `rated-negative-after:{timestamp}`.
/// Returns `(timestamp, Option<EditPredictionRating>)` where `None` means all ratings.
pub fn parse_rated_after_input(input: &str) -> Option<(&str, Option<EditPredictionRating>)> {
    if let Some(timestamp) = input.strip_prefix("rated-positive-after:") {
        Some((timestamp, Some(EditPredictionRating::Positive)))
    } else if let Some(timestamp) = input.strip_prefix("rated-negative-after:") {
        Some((timestamp, Some(EditPredictionRating::Negative)))
    } else if let Some(timestamp) = input.strip_prefix("rated-after:") {
        Some((timestamp, None))
    } else {
        None
    }
}

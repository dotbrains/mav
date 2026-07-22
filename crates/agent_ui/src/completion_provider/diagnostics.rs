use super::*;

pub(super) fn diagnostics_label(
    summary: DiagnosticSummary,
    include_errors: bool,
    include_warnings: bool,
) -> String {
    let mut parts = Vec::new();

    if include_errors && summary.error_count > 0 {
        parts.push(format!(
            "{} {}",
            summary.error_count,
            pluralize("error", summary.error_count)
        ));
    }

    if include_warnings && summary.warning_count > 0 {
        parts.push(format!(
            "{} {}",
            summary.warning_count,
            pluralize("warning", summary.warning_count)
        ));
    }

    if parts.is_empty() {
        return "Diagnostics".into();
    }

    let body = if parts.len() == 2 {
        format!("{} and {}", parts[0], parts[1])
    } else {
        parts
            .pop()
            .expect("at least one part present after non-empty check")
    };

    format!("Diagnostics: {body}")
}

pub(super) fn diagnostics_submenu_label(
    summary: DiagnosticSummary,
    include_errors: bool,
    include_warnings: bool,
) -> String {
    match (include_errors, include_warnings) {
        (true, true) => format!(
            "{} {} & {} {}",
            summary.error_count,
            pluralize("error", summary.error_count),
            summary.warning_count,
            pluralize("warning", summary.warning_count)
        ),
        (true, _) => format!(
            "{} {}",
            summary.error_count,
            pluralize("error", summary.error_count)
        ),
        (_, true) => format!(
            "{} {}",
            summary.warning_count,
            pluralize("warning", summary.warning_count)
        ),
        _ => "Diagnostics".into(),
    }
}

pub(super) fn diagnostics_crease_label(
    summary: DiagnosticSummary,
    include_errors: bool,
    include_warnings: bool,
) -> SharedString {
    diagnostics_label(summary, include_errors, include_warnings).into()
}

fn pluralize(noun: &str, count: usize) -> String {
    if count == 1 {
        noun.to_string()
    } else {
        format!("{noun}s")
    }
}

use crate::example::Example;

pub(super) fn build_quality_feedback(example: &Example) -> Option<String> {
    build_qa_feedback(example).or_else(|| build_score_feedback(example))
}

/// Build the quality feedback string from QA results.
fn build_qa_feedback(example: &Example) -> Option<String> {
    let qa = example.qa.first()?.as_ref()?;

    let qa_reasoning = qa.reasoning.as_deref().unwrap_or("No reasoning provided");
    let reverts_edits = qa
        .reverts_edits
        .map_or("unknown", |v| if v { "yes" } else { "no" });
    let confidence = qa
        .confidence
        .map_or("unknown".to_string(), |v| v.to_string());

    Some(format!(
        "- **Reverts user edits**: {reverts_edits}\n\
         - **Confidence score**: {confidence}/5\n\
         - **Reasoning**: {qa_reasoning}"
    ))
}

/// Build the quality feedback string from computed scores when QA is unavailable.
fn build_score_feedback(example: &Example) -> Option<String> {
    let score = example.score.first()?;

    let mut issues = Vec::new();

    if score.reversal_ratio > 0.9 {
        issues.push(format!(
            "Automated analysis detected a high reversal ratio ({:.2}), which suggests this \
             prediction may be reverting changes the user intentionally made. Double-check that \
             the prediction doesn't undo the user's recent edits. If the prediction is actually \
             fine and the edits are intentional completions rather than reversals, keep it as-is. \
             If it truly reverts the user's changes, generate an improved prediction that \
             continues the user's intent instead.",
            score.reversal_ratio
        ));
    }

    if score.wrong_editable_region == Some(true) {
        issues.push(
            "Automated analysis detected that the prediction may be modifying code outside \
             the expected editable region, or producing changes misaligned with the editable \
             region boundaries. Make sure the prediction only modifies code within the editable \
             region and is properly aligned."
                .to_string(),
        );
    }

    if score.discarded_chars.unwrap_or(0) > 80 && score.exact_lines_fp > 5 {
        issues.push(
            "Automated analysis detected that this prediction might be too large or speculative. \
            Please review it and think if we should keep it or generate a more focused prediction. \
            Examples of more focused predictions: \
            - Predicting a function outline but not its body. \
            - Predicting only the first logical step and not speculating about further steps.
            In general, the smaller the prediction you make, the higher the chance it will be correct."
                .to_string(),
        );
    }

    if issues.is_empty() {
        return None;
    }

    let mut feedback = String::from(
        "No human quality assessment is available, but automated scoring flagged potential issues:\n\n",
    );
    for issue in &issues {
        feedback.push_str(&format!("- {issue}\n"));
    }
    feedback.push_str(
        "\nRemember: if the previous prediction was actually correct, output `KEEP_PREVIOUS`. \
         If no edits should be made at all and you are unsure how to improve it, output `NO_EDITS`.",
    );

    Some(feedback)
}

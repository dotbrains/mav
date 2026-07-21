use super::*;

fn apply_edit(excerpt: &str, parsed_output: &ParsedOutput) -> String {
    let mut result = excerpt.to_string();
    result.replace_range(
        parsed_output.range_in_excerpt.clone(),
        &parsed_output.new_editable_region,
    );
    result
}

#[test]
fn test_parse_zeta2_model_output() {
    let excerpt = "before ctx\nctx start\neditable old\nctx end\nafter ctx\n";
    let context_start = excerpt.find("ctx start").unwrap();
    let context_end = excerpt.find("after ctx").unwrap();
    let editable_start = excerpt.find("editable old").unwrap();
    let editable_end = editable_start + "editable old\n".len();
    let input = make_input_with_context_range(
        excerpt,
        editable_start..editable_end,
        context_start..context_end,
        editable_start,
    );

    let output = parse_zeta2_model_output(
        "editable new\n>>>>>>> UPDATED\n",
        ZetaFormat::V0131GitMergeMarkersPrefix,
        &input,
    )
    .unwrap();

    assert_eq!(
        apply_edit(excerpt, &output),
        "before ctx\nctx start\neditable new\nctx end\nafter ctx\n"
    );
}

#[test]
fn test_parse_zeta2_model_output_identity() {
    let excerpt = "aaa\nbbb\nccc\nddd\neee\n";
    let editable_start = excerpt.find("bbb").unwrap();
    let editable_end = excerpt.find("ddd").unwrap();
    let input = make_input_with_context_range(
        excerpt,
        editable_start..editable_end,
        0..excerpt.len(),
        editable_start,
    );

    let format = ZetaFormat::V0131GitMergeMarkersPrefix;
    let output = parse_zeta2_model_output("bbb\nccc\n>>>>>>> UPDATED\n", format, &input).unwrap();

    assert_eq!(apply_edit(excerpt, &output), excerpt);
}

#[test]
fn test_parse_zeta2_model_output_strips_end_marker() {
    let excerpt = "hello\nworld\n";
    let input = make_input_with_context_range(excerpt, 0..excerpt.len(), 0..excerpt.len(), 0);

    let format = ZetaFormat::V0131GitMergeMarkersPrefix;
    let output1 =
        parse_zeta2_model_output("new content\n>>>>>>> UPDATED\n", format, &input).unwrap();
    let output2 = parse_zeta2_model_output("new content\n", format, &input).unwrap();

    assert_eq!(apply_edit(excerpt, &output1), apply_edit(excerpt, &output2));
    assert_eq!(apply_edit(excerpt, &output1), "new content\n");
}

#[test]
fn test_parsed_output_to_patch_round_trips_through_udiff_application() {
    let excerpt = "before ctx\nctx start\neditable old\nctx end\nafter ctx\n";
    let context_start = excerpt.find("ctx start").unwrap();
    let context_end = excerpt.find("after ctx").unwrap();
    let editable_start = excerpt.find("editable old").unwrap();
    let editable_end = editable_start + "editable old\n".len();
    let input = make_input_with_context_range(
        excerpt,
        editable_start..editable_end,
        context_start..context_end,
        editable_start,
    );

    let parsed = parse_zeta2_model_output(
        "editable new\n>>>>>>> UPDATED\n",
        ZetaFormat::V0131GitMergeMarkersPrefix,
        &input,
    )
    .unwrap();
    let expected = apply_edit(excerpt, &parsed);
    let patch = parsed_output_to_patch(&input, parsed).unwrap();
    let patched = udiff::apply_diff_to_string(&patch, excerpt).unwrap();

    assert_eq!(patched, expected);
}

#[test]
fn test_special_tokens_not_triggered_by_comment_separator() {
    // Regression test for https://github.com/mav-industries/mav/issues/52489
    let excerpt = "fn main() {\n    // =======\n    println!(\"hello\");\n}\n";
    let input = make_input(excerpt, 0..excerpt.len(), 0, vec![], vec![]);
    assert!(
        !prompt_input_contains_special_tokens(&input, ZetaFormat::V0131GitMergeMarkersPrefix),
        "comment containing ======= should not trigger special token detection"
    );
}

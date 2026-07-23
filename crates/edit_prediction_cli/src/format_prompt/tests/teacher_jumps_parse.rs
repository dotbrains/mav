use super::support::make_example;
use super::*;

#[test]
fn test_teacher_jumps_parse_single_edit_in_cursor_file() {
    let example = make_example("fn main() {\n    let x = 1;\n}\n", 16, &[]);
    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    let cursor_markers = &marker_table[0].markers;
    let start_tag = hashed_regions::marker_tag(&cursor_markers[0].0);
    let end_tag = hashed_regions::marker_tag(&cursor_markers[cursor_markers.len() - 1].0);

    let response = format!(
        "The user is changing x.\n\n`````\n{start_tag}\nfn main() {{\n    let x = 2;<|user_cursor|>\n}}\n{end_tag}\n`````\n"
    );
    let (patch, cursor) = TeacherJumpsPrompt::parse(&example, &response).unwrap();

    assert!(patch.contains("--- a/src/main.rs"), "patch: {patch}");
    assert!(patch.contains("-    let x = 1;"), "patch: {patch}");
    assert!(patch.contains("+    let x = 2;"), "patch: {patch}");
    let cursor = cursor.unwrap();
    assert_eq!(cursor.path, "src/main.rs");
    assert_eq!(cursor.row, 1);
}

#[test]
fn test_teacher_jumps_parse_sequence_across_files() {
    let example = make_example(
        "fn fetch_user_cached() {}\n",
        0,
        &[(
            "src/server.rs",
            &[("fn handle() {\n    fetch_user();\n}\n", 10)],
        )],
    );
    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    assert_eq!(marker_table.len(), 2);
    let related_markers = &marker_table[1].markers;
    let start_tag = hashed_regions::marker_tag(&related_markers[0].0);
    let end_tag = hashed_regions::marker_tag(&related_markers[related_markers.len() - 1].0);

    let response = format!(
        "Updating the call site to use the new name.\n\n\
         `````\n{start_tag}\nfn handle() {{\n    fetch_user_cached();\n}}\n{end_tag}\n`````\n"
    );
    let (patch, cursor) = TeacherJumpsPrompt::parse(&example, &response).unwrap();

    assert!(patch.contains("--- a/src/server.rs"), "patch: {patch}");
    assert!(patch.contains("-    fetch_user();"), "patch: {patch}");
    assert!(
        patch.contains("+    fetch_user_cached();"),
        "patch: {patch}"
    );
    // Hunk rows are file-absolute for related files (1-based in the
    // hunk header, excerpt starts at 0-based row 10).
    assert!(patch.contains("@@ -11,"), "patch: {patch}");
    assert!(cursor.is_none());
}

#[test]
fn test_teacher_jumps_parse_multiple_edits_same_file() {
    let cursor_excerpt = "\
        fn alpha() {\n    one();\n}\n\nfn beta() {\n    two();\n}\n\n\
        fn gamma() {\n    three();\n}\n\nfn delta() {\n    four();\n}\n";
    let example = make_example(cursor_excerpt, 0, &[]);
    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    let markers = &marker_table[0].markers;
    assert!(
        markers.len() >= 3,
        "expected internal markers, got {markers:?}"
    );

    // First edit: between the first two markers; second edit: between the
    // second and last markers.
    let tag = |ix: usize| hashed_regions::marker_tag(&markers[ix].0);
    let old_first_span = &cursor_excerpt[markers[0].1..markers[1].1];
    let old_second_span = &cursor_excerpt[markers[1].1..markers[markers.len() - 1].1];
    let new_first_span = old_first_span.replace("one()", "uno()");
    let new_second_span = old_second_span.replace("four()", "cuatro()");

    let response = format!(
        "Renaming calls.\n\n`````\n{}\n{}{}\n`````\n\n`````\n{}\n{}{}\n`````\n",
        tag(0),
        new_first_span,
        tag(1),
        tag(1),
        new_second_span,
        tag(markers.len() - 1),
    );
    let (patch, _) = TeacherJumpsPrompt::parse(&example, &response).unwrap();

    assert!(patch.contains("+    uno();"), "patch: {patch}");
    assert!(patch.contains("+    cuatro();"), "patch: {patch}");
    assert_eq!(patch.matches("--- a/src/main.rs").count(), 1);
}

#[test]
fn test_teacher_jumps_parse_no_edits() {
    let example = make_example("fn main() {}\n", 0, &[]);
    let (patch, cursor) =
        TeacherJumpsPrompt::parse(&example, "All good.\n\n`````\nNO_EDITS\n`````\n").unwrap();
    assert!(patch.is_empty());
    assert!(cursor.is_none());
}

#[test]
fn test_teacher_jumps_parse_rejects_truncated_span() {
    let cursor_excerpt = "\
        fn alpha() {\n    one();\n}\n\nfn beta() {\n    two();\n}\n\n\
        fn gamma() {\n    three();\n}\n\nfn delta() {\n    four();\n}\n";
    let example = make_example(cursor_excerpt, 0, &[]);
    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    let markers = &marker_table[0].markers;
    assert!(markers.len() >= 3);
    let start_tag = hashed_regions::marker_tag(&markers[0].0);
    let end_tag = hashed_regions::marker_tag(&markers[markers.len() - 1].0);

    // The model reproduces only the head of the span and stops before the
    // end marker; accepting this would silently delete the rest.
    let head = &cursor_excerpt[markers[0].1..markers[1].1];
    let response = format!("Minor cleanup.\n\n`````\n{start_tag}\n{head}{end_tag}\n`````\n");
    let error = TeacherJumpsPrompt::parse(&example, &response).unwrap_err();
    assert!(
        error.to_string().contains("looks truncated"),
        "unexpected error: {error}"
    );
}

#[test]
fn test_teacher_jumps_parse_rejects_tail_deletion_after_head_edit() {
    let cursor_excerpt = "\
        fn alpha() {\n    one();\n}\n\nfn beta() {\n    two();\n}\n\n\
        fn gamma() {\n    three();\n}\n\nfn delta() {\n    four();\n}\n";
    let example = make_example(cursor_excerpt, 0, &[]);
    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    let markers = &marker_table[0].markers;
    assert!(markers.len() >= 3);
    let start_tag = hashed_regions::marker_tag(&markers[0].0);
    let end_tag = hashed_regions::marker_tag(&markers[markers.len() - 1].0);

    // The dropped tail is large enough to trip the trailing-deletion check.
    let tail = &cursor_excerpt[markers[1].1..];
    assert!(tail.lines().filter(|line| !line.trim().is_empty()).count() > 3);

    // The model makes a real edit at the head of the span, reproduces
    // some context, and then stops before the end marker. The replacement
    // is not a verbatim prefix of the span, but the tail is still
    // silently deleted.
    let head = &cursor_excerpt[markers[0].1..markers[1].1];
    assert!(head.contains("fn alpha()"));
    let edited_head = head.replacen("fn alpha()", "fn alpha_renamed()", 1);
    let response =
        format!("Renaming alpha.\n\n`````\n{start_tag}\n{edited_head}{end_tag}\n`````\n");
    let error = TeacherJumpsPrompt::parse(&example, &response).unwrap_err();
    assert!(
        error.to_string().contains("looks truncated"),
        "unexpected error: {error}"
    );
}

#[test]
fn test_teacher_jumps_parse_allows_mid_span_deletion() {
    let cursor_excerpt = "\
        fn alpha() {\n    one();\n}\n\nfn beta() {\n    two();\n}\n\n\
        fn gamma() {\n    three();\n}\n\nfn delta() {\n    four();\n}\n";
    let example = make_example(cursor_excerpt, 0, &[]);
    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    let markers = &marker_table[0].markers;
    assert!(markers.len() >= 3);
    let start_tag = hashed_regions::marker_tag(&markers[0].0);
    let end_tag = hashed_regions::marker_tag(&markers[markers.len() - 1].0);

    // Deleting code in the middle while reproducing the span's tail shows
    // the model kept writing to the end marker, so it must be accepted.
    let head = &cursor_excerpt[markers[0].1..markers[1].1];
    let reproduced_tail = &cursor_excerpt[markers[markers.len() - 2].1..];
    assert!(!reproduced_tail.trim().is_empty());
    let response = format!(
        "Removing the middle.\n\n`````\n{start_tag}\n{head}{reproduced_tail}{end_tag}\n`````\n"
    );
    let (patch, _) = TeacherJumpsPrompt::parse(&example, &response).unwrap();
    assert!(patch.contains("-fn beta() {"), "patch: {patch}");
}

#[test]
fn test_teacher_jumps_parse_allows_small_tail_deletion() {
    let cursor_excerpt = "\
        fn alpha() {\n    one();\n}\n\nfn beta() {\n    two();\n}\n\n\
        fn gamma() {\n    three();\n}\n\nfn delta() {\n    four();\n}\n";
    let example = make_example(cursor_excerpt, 0, &[]);
    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    let markers = &marker_table[0].markers;
    let start_tag = hashed_regions::marker_tag(&markers[0].0);
    let end_tag = hashed_regions::marker_tag(&markers[markers.len() - 1].0);

    // Dropping only the last line of the span may be a genuine
    // end-of-snippet deletion, so it stays below the threshold.
    let new_span = cursor_excerpt.strip_suffix("}\n").unwrap();
    let response =
        format!("Dropping the brace.\n\n`````\n{start_tag}\n{new_span}{end_tag}\n`````\n");
    let (patch, _) = TeacherJumpsPrompt::parse(&example, &response).unwrap();
    assert!(patch.contains("-}"), "patch: {patch}");
}

#[test]
fn test_teacher_jumps_parse_allows_empty_span_deletion() {
    let cursor_excerpt = "\
        fn alpha() {\n    one();\n}\n\nfn beta() {\n    two();\n}\n\n\
        fn gamma() {\n    three();\n}\n\nfn delta() {\n    four();\n}\n";
    let example = make_example(cursor_excerpt, 0, &[]);
    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    let markers = &marker_table[0].markers;
    assert!(markers.len() >= 3);
    let start_tag = hashed_regions::marker_tag(&markers[0].0);
    let end_tag = hashed_regions::marker_tag(&markers[1].0);

    // Deleting an entire span by replacing it with nothing is fine.
    let response = format!("Removing alpha.\n\n`````\n{start_tag}\n{end_tag}\n`````\n");
    let (patch, _) = TeacherJumpsPrompt::parse(&example, &response).unwrap();
    assert!(patch.contains("-fn alpha() {"), "patch: {patch}");
}

#[test]
fn test_teacher_jumps_parse_span_across_contiguous_excerpts() {
    // Two excerpts of src/lib.rs with touching row ranges (5..8 and
    // 8..11) render seamlessly in the prompt, so the model may span a
    // single edit across the excerpt boundary.
    let example = make_example(
        "fn main() {}\n",
        0,
        &[(
            "src/lib.rs",
            &[
                ("fn a() {\n    one();\n}\n", 5),
                ("fn b() {\n    two();\n}\n", 8),
            ],
        )],
    );
    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    assert_eq!(marker_table.len(), 3);
    let start_tag = hashed_regions::marker_tag(&marker_table[1].markers[0].0);
    let last_markers = &marker_table[2].markers;
    let end_tag = hashed_regions::marker_tag(&last_markers[last_markers.len() - 1].0);

    let response = format!(
        "Renaming both.\n\n`````\n{start_tag}\nfn a() {{\n    uno();\n}}\nfn b() {{\n    dos();\n}}\n{end_tag}\n`````\n"
    );
    let (patch, _) = TeacherJumpsPrompt::parse(&example, &response).unwrap();

    assert!(patch.contains("+    uno();"), "patch: {patch}");
    assert!(patch.contains("+    dos();"), "patch: {patch}");
    assert_eq!(patch.matches("--- a/src/lib.rs").count(), 1);
    // Hunk rows are file-absolute (merged region starts at 0-based row 5).
    assert!(patch.contains("@@ -6,"), "patch: {patch}");
}

#[test]
fn test_teacher_jumps_parse_insertion_at_contiguous_excerpt_seam() {
    // The two markers at the seam between contiguous excerpts map to the
    // same merged offset; bracketing them expresses a pure insertion.
    let example = make_example(
        "fn main() {}\n",
        0,
        &[(
            "src/lib.rs",
            &[
                ("fn a() {\n    one();\n}\n", 5),
                ("fn b() {\n    two();\n}\n", 8),
            ],
        )],
    );
    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    assert_eq!(marker_table.len(), 3);
    let first_markers = &marker_table[1].markers;
    let start_tag = hashed_regions::marker_tag(&first_markers[first_markers.len() - 1].0);
    let end_tag = hashed_regions::marker_tag(&marker_table[2].markers[0].0);

    let response = format!(
        "Adding a function between a and b.\n\n`````\n{start_tag}\nfn between() {{}}\n{end_tag}\n`````\n"
    );
    let (patch, _) = TeacherJumpsPrompt::parse(&example, &response).unwrap();

    assert!(patch.contains("+fn between() {}"), "patch: {patch}");
    assert!(
        !patch.contains("-\n"),
        "patch should be pure insertion: {patch}"
    );
    // Insertion lands between the excerpts (after 0-based row 7).
    assert!(patch.contains("@@ -6,"), "patch: {patch}");
}

#[test]
fn test_teacher_jumps_parse_rejects_span_across_gapped_excerpts() {
    // Same file, but the excerpts don't touch (5..8 and 20..23): rows in
    // between were never shown to the model, so a span across them is
    // invalid.
    let example = make_example(
        "fn main() {}\n",
        0,
        &[(
            "src/lib.rs",
            &[
                ("fn a() {\n    one();\n}\n", 5),
                ("fn b() {\n    two();\n}\n", 20),
            ],
        )],
    );
    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    assert_eq!(marker_table.len(), 3);
    let start_tag = hashed_regions::marker_tag(&marker_table[1].markers[0].0);
    let last_markers = &marker_table[2].markers;
    let end_tag = hashed_regions::marker_tag(&last_markers[last_markers.len() - 1].0);

    let response = format!(
        "Renaming both.\n\n`````\n{start_tag}\nfn a() {{\n    uno();\n}}\nfn b() {{\n    dos();\n}}\n{end_tag}\n`````\n"
    );
    let error = TeacherJumpsPrompt::parse(&example, &response).unwrap_err();
    assert!(
        error.to_string().contains("different context snippets"),
        "unexpected error: {error}"
    );
}

#[test]
fn test_teacher_jumps_parse_rejects_unknown_marker() {
    let example = make_example("fn main() {}\n", 0, &[]);
    let response = "`````\n<|marker_zzzz|>\nnew\n<|marker_yyyy|>\n`````\n";
    assert!(TeacherJumpsPrompt::parse(&example, response).is_err());
}

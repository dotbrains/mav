use super::*;

#[test]
fn test_output_has_edit_commands() {
    assert!(hashline::output_has_edit_commands(&format!(
        "{}0:ab\nnew",
        SET_COMMAND_MARKER
    )));
    assert!(hashline::output_has_edit_commands(&format!(
        "{}0:ab\nnew",
        INSERT_COMMAND_MARKER
    )));
    assert!(hashline::output_has_edit_commands(&format!(
        "some text\n{}1:cd\nstuff",
        SET_COMMAND_MARKER
    )));
    assert!(!hashline::output_has_edit_commands("just plain text"));
    assert!(!hashline::output_has_edit_commands("NO_EDITS"));
    assert!(hashline::output_has_edit_commands("<|no_edits|>"));
}

// ---- hashline::patch_to_edit_commands round-trip tests ----

use super::*;

#[gpui::test]
async fn test_clipboard(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state("«one✅ ˇ»two «three ˇ»four «five ˇ»six ");
    cx.update_editor(|e, window, cx| e.cut(&Cut, window, cx));
    cx.assert_editor_state("ˇtwo ˇfour ˇsix ");

    // Paste with three cursors. Each cursor pastes one slice of the clipboard text.
    cx.set_state("two ˇfour ˇsix ˇ");
    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state("two one✅ ˇfour three ˇsix five ˇ");

    // Paste again but with only two cursors. Since the number of cursors doesn't
    // match the number of slices in the clipboard, the entire clipboard text
    // is pasted at each cursor.
    cx.set_state("ˇtwo one✅ four three six five ˇ");
    cx.update_editor(|e, window, cx| {
        e.handle_input("( ", window, cx);
        e.paste(&Paste, window, cx);
        e.handle_input(") ", window, cx);
    });
    cx.assert_editor_state(
        &([
            "( one✅ ",
            "three ",
            "five ) ˇtwo one✅ four three six five ( one✅ ",
            "three ",
            "five ) ˇ",
        ]
        .join("\n")),
    );

    // Cut with three selections, one of which is full-line.
    cx.set_state(indoc! {"
        1«2ˇ»3
        4ˇ567
        «8ˇ»9"});
    cx.update_editor(|e, window, cx| e.cut(&Cut, window, cx));
    cx.assert_editor_state(indoc! {"
        1ˇ3
        ˇ9"});

    // Paste with three selections, noticing how the copied selection that was full-line
    // gets inserted before the second cursor.
    cx.set_state(indoc! {"
        1ˇ3
        9ˇ
        «oˇ»ne"});
    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state(indoc! {"
        12ˇ3
        4567
        9ˇ
        8ˇne"});

    // Copy with a single cursor only, which writes the whole line into the clipboard.
    cx.set_state(indoc! {"
        The quick brown
        fox juˇmps over
        the lazy dog"});
    cx.update_editor(|e, window, cx| e.copy(&Copy, window, cx));
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text().as_deref().map(str::to_string)),
        Some("fox jumps over\n".to_string())
    );

    // Paste with three selections, noticing how the copied full-line selection is inserted
    // before the empty selections but replaces the selection that is non-empty.
    cx.set_state(indoc! {"
        Tˇhe quick brown
        «foˇ»x jumps over
        tˇhe lazy dog"});
    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state(indoc! {"
        fox jumps over
        Tˇhe quick brown
        fox jumps over
        ˇx jumps over
        fox jumps over
        tˇhe lazy dog"});
}

#[gpui::test]
async fn test_copy_trim(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(
        r#"            «for selection in selections.iter() {
            let mut start = selection.start;
            let mut end = selection.end;
            let is_entire_line = selection.is_empty();
            if is_entire_line {
                start = Point::new(start.row, 0);ˇ»
                end = cmp::min(max_point, Point::new(end.row + 1, 0));
            }
        "#,
    );
    cx.update_editor(|e, window, cx| e.copy(&Copy, window, cx));
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text().as_deref().map(str::to_string)),
        Some(
            "for selection in selections.iter() {
            let mut start = selection.start;
            let mut end = selection.end;
            let is_entire_line = selection.is_empty();
            if is_entire_line {
                start = Point::new(start.row, 0);"
                .to_string()
        ),
        "Regular copying preserves all indentation selected",
    );
    cx.update_editor(|e, window, cx| e.copy_and_trim(&CopyAndTrim, window, cx));
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text().as_deref().map(str::to_string)),
        Some(
            "for selection in selections.iter() {
let mut start = selection.start;
let mut end = selection.end;
let is_entire_line = selection.is_empty();
if is_entire_line {
    start = Point::new(start.row, 0);"
                .to_string()
        ),
        "Copying with stripping should strip all leading whitespaces"
    );

    cx.set_state(
        r#"       «     for selection in selections.iter() {
            let mut start = selection.start;
            let mut end = selection.end;
            let is_entire_line = selection.is_empty();
            if is_entire_line {
                start = Point::new(start.row, 0);ˇ»
                end = cmp::min(max_point, Point::new(end.row + 1, 0));
            }
        "#,
    );
    cx.update_editor(|e, window, cx| e.copy(&Copy, window, cx));
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text().as_deref().map(str::to_string)),
        Some(
            "     for selection in selections.iter() {
            let mut start = selection.start;
            let mut end = selection.end;
            let is_entire_line = selection.is_empty();
            if is_entire_line {
                start = Point::new(start.row, 0);"
                .to_string()
        ),
        "Regular copying preserves all indentation selected",
    );
    cx.update_editor(|e, window, cx| e.copy_and_trim(&CopyAndTrim, window, cx));
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text().as_deref().map(str::to_string)),
        Some(
            "for selection in selections.iter() {
let mut start = selection.start;
let mut end = selection.end;
let is_entire_line = selection.is_empty();
if is_entire_line {
    start = Point::new(start.row, 0);"
                .to_string()
        ),
        "Copying with stripping should strip all leading whitespaces, even if some of it was selected"
    );

    cx.set_state(
        r#"       «ˇ     for selection in selections.iter() {
            let mut start = selection.start;
            let mut end = selection.end;
            let is_entire_line = selection.is_empty();
            if is_entire_line {
                start = Point::new(start.row, 0);»
                end = cmp::min(max_point, Point::new(end.row + 1, 0));
            }
        "#,
    );
    cx.update_editor(|e, window, cx| e.copy(&Copy, window, cx));
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text().as_deref().map(str::to_string)),
        Some(
            "     for selection in selections.iter() {
            let mut start = selection.start;
            let mut end = selection.end;
            let is_entire_line = selection.is_empty();
            if is_entire_line {
                start = Point::new(start.row, 0);"
                .to_string()
        ),
        "Regular copying for reverse selection works the same",
    );
    cx.update_editor(|e, window, cx| e.copy_and_trim(&CopyAndTrim, window, cx));
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text().as_deref().map(str::to_string)),
        Some(
            "for selection in selections.iter() {
let mut start = selection.start;
let mut end = selection.end;
let is_entire_line = selection.is_empty();
if is_entire_line {
    start = Point::new(start.row, 0);"
                .to_string()
        ),
        "Copying with stripping for reverse selection works the same"
    );

    cx.set_state(
        r#"            for selection «in selections.iter() {
            let mut start = selection.start;
            let mut end = selection.end;
            let is_entire_line = selection.is_empty();
            if is_entire_line {
                start = Point::new(start.row, 0);ˇ»
                end = cmp::min(max_point, Point::new(end.row + 1, 0));
            }
        "#,
    );
    cx.update_editor(|e, window, cx| e.copy(&Copy, window, cx));
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text().as_deref().map(str::to_string)),
        Some(
            "in selections.iter() {
            let mut start = selection.start;
            let mut end = selection.end;
            let is_entire_line = selection.is_empty();
            if is_entire_line {
                start = Point::new(start.row, 0);"
                .to_string()
        ),
        "When selecting past the indent, the copying works as usual",
    );
    cx.update_editor(|e, window, cx| e.copy_and_trim(&CopyAndTrim, window, cx));
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text().as_deref().map(str::to_string)),
        Some(
            "in selections.iter() {
            let mut start = selection.start;
            let mut end = selection.end;
            let is_entire_line = selection.is_empty();
            if is_entire_line {
                start = Point::new(start.row, 0);"
                .to_string()
        ),
        "When selecting past the indent, nothing is trimmed"
    );

    cx.set_state(
        r#"            «for selection in selections.iter() {
            let mut start = selection.start;

            let mut end = selection.end;
            let is_entire_line = selection.is_empty();
            if is_entire_line {
                start = Point::new(start.row, 0);
ˇ»                end = cmp::min(max_point, Point::new(end.row + 1, 0));
            }
        "#,
    );
    cx.update_editor(|e, window, cx| e.copy_and_trim(&CopyAndTrim, window, cx));
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text().as_deref().map(str::to_string)),
        Some(
            "for selection in selections.iter() {
let mut start = selection.start;

let mut end = selection.end;
let is_entire_line = selection.is_empty();
if is_entire_line {
    start = Point::new(start.row, 0);
"
            .to_string()
        ),
        "Copying with stripping should ignore empty lines"
    );
}

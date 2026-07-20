use super::*;

#[gpui::test]
async fn test_manipulate_immutable_lines_with_multi_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    // Manipulate with multiple selections on a single line
    cx.set_state(indoc! {"
        dd«dd
        cˇ»c«c
        bb
        aaaˇ»aa
    "});
    cx.update_editor(|e, window, cx| {
        e.sort_lines_case_sensitive(&SortLinesCaseSensitive, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «aaaaa
        bb
        ccc
        ddddˇ»
    "});

    // Manipulate with multiple disjoin selections
    cx.set_state(indoc! {"
        5«
        4
        3
        2
        1ˇ»

        dd«dd
        ccc
        bb
        aaaˇ»aa
    "});
    cx.update_editor(|e, window, cx| {
        e.sort_lines_case_sensitive(&SortLinesCaseSensitive, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «1
        2
        3
        4
        5ˇ»

        «aaaaa
        bb
        ccc
        ddddˇ»
    "});

    // Adding lines on each selection
    cx.set_state(indoc! {"
        2«
        1ˇ»

        bb«bb
        aaaˇ»aa
    "});
    cx.update_editor(|e, window, cx| {
        e.manipulate_immutable_lines(window, cx, |lines| lines.push("added line"))
    });
    cx.assert_editor_state(indoc! {"
        «2
        1
        added lineˇ»

        «bbbb
        aaaaa
        added lineˇ»
    "});

    // Removing lines on each selection
    cx.set_state(indoc! {"
        2«
        1ˇ»

        bb«bb
        aaaˇ»aa
    "});
    cx.update_editor(|e, window, cx| {
        e.manipulate_immutable_lines(window, cx, |lines| {
            lines.pop();
        })
    });
    cx.assert_editor_state(indoc! {"
        «2ˇ»

        «bbbbˇ»
    "});
}

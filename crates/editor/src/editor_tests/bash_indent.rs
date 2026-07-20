use super::*;

#[gpui::test]
async fn test_tab_in_leading_whitespace_auto_indents_for_bash(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("bash", tree_sitter_bash::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test cursor move to start of each line on tab
    // for `if`, `elif`, `else`, `while`, `for`, `case` and `function`
    cx.set_state(indoc! {"
        function main() {
        ˇ    for item in $items; do
        ˇ        while [ -n \"$item\" ]; do
        ˇ            if [ \"$value\" -gt 10 ]; then
        ˇ                continue
        ˇ            elif [ \"$value\" -lt 0 ]; then
        ˇ                break
        ˇ            else
        ˇ                echo \"$item\"
        ˇ            fi
        ˇ        done
        ˇ    done
        ˇ}
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        function main() {
            ˇfor item in $items; do
                ˇwhile [ -n \"$item\" ]; do
                    ˇif [ \"$value\" -gt 10 ]; then
                        ˇcontinue
                    ˇelif [ \"$value\" -lt 0 ]; then
                        ˇbreak
                    ˇelse
                        ˇecho \"$item\"
                    ˇfi
                ˇdone
            ˇdone
        ˇ}
    "});
    // test relative indent is preserved when tab
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        function main() {
                ˇfor item in $items; do
                    ˇwhile [ -n \"$item\" ]; do
                        ˇif [ \"$value\" -gt 10 ]; then
                            ˇcontinue
                        ˇelif [ \"$value\" -lt 0 ]; then
                            ˇbreak
                        ˇelse
                            ˇecho \"$item\"
                        ˇfi
                    ˇdone
                ˇdone
            ˇ}
    "});

    // test cursor move to start of each line on tab
    // for `case` statement with patterns
    cx.set_state(indoc! {"
        function handle() {
        ˇ    case \"$1\" in
        ˇ        start)
        ˇ            echo \"a\"
        ˇ            ;;
        ˇ        stop)
        ˇ            echo \"b\"
        ˇ            ;;
        ˇ        *)
        ˇ            echo \"c\"
        ˇ            ;;
        ˇ    esac
        ˇ}
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        function handle() {
            ˇcase \"$1\" in
                ˇstart)
                    ˇecho \"a\"
                    ˇ;;
                ˇstop)
                    ˇecho \"b\"
                    ˇ;;
                ˇ*)
                    ˇecho \"c\"
                    ˇ;;
            ˇesac
        ˇ}
    "});
}

#[gpui::test]
async fn test_indent_after_input_for_bash(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("bash", tree_sitter_bash::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test indents on comment insert
    cx.set_state(indoc! {"
        function main() {
        ˇ    for item in $items; do
        ˇ        while [ -n \"$item\" ]; do
        ˇ            if [ \"$value\" -gt 10 ]; then
        ˇ                continue
        ˇ            elif [ \"$value\" -lt 0 ]; then
        ˇ                break
        ˇ            else
        ˇ                echo \"$item\"
        ˇ            fi
        ˇ        done
        ˇ    done
        ˇ}
    "});
    cx.update_editor(|e, window, cx| e.handle_input("#", window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        function main() {
        #ˇ    for item in $items; do
        #ˇ        while [ -n \"$item\" ]; do
        #ˇ            if [ \"$value\" -gt 10 ]; then
        #ˇ                continue
        #ˇ            elif [ \"$value\" -lt 0 ]; then
        #ˇ                break
        #ˇ            else
        #ˇ                echo \"$item\"
        #ˇ            fi
        #ˇ        done
        #ˇ    done
        #ˇ}
    "});
}

#[gpui::test]
async fn test_outdent_after_input_for_bash(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("bash", tree_sitter_bash::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test `else` auto outdents when typed inside `if` block
    cx.set_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"foo bar\"
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("else", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"foo bar\"
        elseˇ
    "});

    // test `elif` auto outdents when typed inside `if` block
    cx.set_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"foo bar\"
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("elif", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"foo bar\"
        elifˇ
    "});

    // test `fi` auto outdents when typed inside `else` block
    cx.set_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"foo bar\"
        else
            echo \"bar baz\"
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("fi", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"foo bar\"
        else
            echo \"bar baz\"
        fiˇ
    "});

    // test `done` auto outdents when typed inside `while` block
    cx.set_state(indoc! {"
        while read line; do
            echo \"$line\"
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("done", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        while read line; do
            echo \"$line\"
        doneˇ
    "});

    // test `done` auto outdents when typed inside `for` block
    cx.set_state(indoc! {"
        for file in *.txt; do
            cat \"$file\"
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("done", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        for file in *.txt; do
            cat \"$file\"
        doneˇ
    "});

    // test `esac` auto outdents when typed inside `case` block
    cx.set_state(indoc! {"
        case \"$1\" in
            start)
                echo \"foo bar\"
                ;;
            stop)
                echo \"bar baz\"
                ;;
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("esac", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        case \"$1\" in
            start)
                echo \"foo bar\"
                ;;
            stop)
                echo \"bar baz\"
                ;;
        esacˇ
    "});

    // test `*)` auto outdents when typed inside `case` block
    cx.set_state(indoc! {"
        case \"$1\" in
            start)
                echo \"foo bar\"
                ;;
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("*)", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        case \"$1\" in
            start)
                echo \"foo bar\"
                ;;
            *)ˇ
    "});

    // test `fi` outdents to correct level with nested if blocks
    cx.set_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"outer if\"
            if [ \"$2\" = \"debug\" ]; then
                echo \"inner if\"
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("fi", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"outer if\"
            if [ \"$2\" = \"debug\" ]; then
                echo \"inner if\"
            fiˇ
    "});
}

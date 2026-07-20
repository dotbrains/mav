use super::*;

#[gpui::test]
async fn test_tab_in_leading_whitespace_auto_indents_for_python(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("python", tree_sitter_python::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test cursor move to start of each line on tab
    // for `if`, `elif`, `else`, `while`, `with` and `for`
    cx.set_state(indoc! {"
        def main():
        ˇ    for item in items:
        ˇ        while item.active:
        ˇ            if item.value > 10:
        ˇ                continue
        ˇ            elif item.value < 0:
        ˇ                break
        ˇ            else:
        ˇ                with item.context() as ctx:
        ˇ                    yield count
        ˇ        else:
        ˇ            log('while else')
        ˇ    else:
        ˇ        log('for else')
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            ˇfor item in items:
                ˇwhile item.active:
                    ˇif item.value > 10:
                        ˇcontinue
                    ˇelif item.value < 0:
                        ˇbreak
                    ˇelse:
                        ˇwith item.context() as ctx:
                            ˇyield count
                ˇelse:
                    ˇlog('while else')
            ˇelse:
                ˇlog('for else')
    "});
    // test relative indent is preserved when tab
    // for `if`, `elif`, `else`, `while`, `with` and `for`
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
                ˇfor item in items:
                    ˇwhile item.active:
                        ˇif item.value > 10:
                            ˇcontinue
                        ˇelif item.value < 0:
                            ˇbreak
                        ˇelse:
                            ˇwith item.context() as ctx:
                                ˇyield count
                    ˇelse:
                        ˇlog('while else')
                ˇelse:
                    ˇlog('for else')
    "});

    // test cursor move to start of each line on tab
    // for `try`, `except`, `else`, `finally`, `match` and `def`
    cx.set_state(indoc! {"
        def main():
        ˇ    try:
        ˇ        fetch()
        ˇ    except ValueError:
        ˇ        handle_error()
        ˇ    else:
        ˇ        match value:
        ˇ            case _:
        ˇ    finally:
        ˇ        def status():
        ˇ            return 0
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            ˇtry:
                ˇfetch()
            ˇexcept ValueError:
                ˇhandle_error()
            ˇelse:
                ˇmatch value:
                    ˇcase _:
            ˇfinally:
                ˇdef status():
                    ˇreturn 0
    "});
    // test relative indent is preserved when tab
    // for `try`, `except`, `else`, `finally`, `match` and `def`
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
                ˇtry:
                    ˇfetch()
                ˇexcept ValueError:
                    ˇhandle_error()
                ˇelse:
                    ˇmatch value:
                        ˇcase _:
                ˇfinally:
                    ˇdef status():
                        ˇreturn 0
    "});
}

#[gpui::test]
async fn test_outdent_after_input_for_python(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("python", tree_sitter_python::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test `else` auto outdents when typed inside `if` block
    cx.set_state(indoc! {"
        def main():
            if i == 2:
                return
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("else:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            if i == 2:
                return
            else:ˇ
    "});

    // test `except` auto outdents when typed inside `try` block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("except:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
            except:ˇ
    "});

    // test `else` auto outdents when typed inside `except` block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                j = 2
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("else:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                j = 2
            else:ˇ
    "});

    // test `finally` auto outdents when typed inside `else` block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                j = 2
            else:
                k = 2
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("finally:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                j = 2
            else:
                k = 2
            finally:ˇ
    "});

    // test `else` does not outdents when typed inside `except` block right after for block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                for i in range(n):
                    pass
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("else:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                for i in range(n):
                    pass
                else:ˇ
    "});

    // test `finally` auto outdents when typed inside `else` block right after for block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                j = 2
            else:
                for i in range(n):
                    pass
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("finally:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                j = 2
            else:
                for i in range(n):
                    pass
            finally:ˇ
    "});

    // test `except` outdents to inner "try" block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
                if i == 2:
                    try:
                        i = 3
                        ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("except:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
                if i == 2:
                    try:
                        i = 3
                    except:ˇ
    "});

    // test `except` outdents to outer "try" block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
                if i == 2:
                    try:
                        i = 3
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("except:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
                if i == 2:
                    try:
                        i = 3
            except:ˇ
    "});

    // test `else` stays at correct indent when typed after `for` block
    cx.set_state(indoc! {"
        def main():
            for i in range(10):
                if i == 3:
                    break
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("else:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            for i in range(10):
                if i == 3:
                    break
            else:ˇ
    "});

    // test does not outdent on typing after line with square brackets
    cx.set_state(indoc! {"
        def f() -> list[str]:
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("a", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def f() -> list[str]:
            aˇ
    "});

    // test does not outdent on typing : after case keyword
    cx.set_state(indoc! {"
        match 1:
            caseˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(":", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        match 1:
            case:ˇ
    "});
}

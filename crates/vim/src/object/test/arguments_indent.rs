use super::*;

#[gpui::test]
async fn test_argument_object(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Generic arguments
    cx.set_state("fn boop<A: ˇDebug, B>() {}", Mode::Normal);
    cx.simulate_keystrokes("v i a");
    cx.assert_state("fn boop<«A: Debugˇ», B>() {}", Mode::Visual);

    // Function arguments
    cx.set_state(
        "fn boop(ˇarg_a: (Tuple, Of, Types), arg_b: String) {}",
        Mode::Normal,
    );
    cx.simulate_keystrokes("d a a");
    cx.assert_state("fn boop(ˇarg_b: String) {}", Mode::Normal);

    cx.set_state("std::namespace::test(\"strinˇg\", a.b.c())", Mode::Normal);
    cx.simulate_keystrokes("v a a");
    cx.assert_state("std::namespace::test(«\"string\", ˇ»a.b.c())", Mode::Visual);

    // Tuple, vec, and array arguments
    cx.set_state(
        "fn boop(arg_a: (Tuple, Ofˇ, Types), arg_b: String) {}",
        Mode::Normal,
    );
    cx.simulate_keystrokes("c i a");
    cx.assert_state(
        "fn boop(arg_a: (Tuple, ˇ, Types), arg_b: String) {}",
        Mode::Insert,
    );

    // TODO regressed with the up-to-date Rust grammar.
    // cx.set_state("let a = (test::call(), 'p', my_macro!{ˇ});", Mode::Normal);
    // cx.simulate_keystrokes("c a a");
    // cx.assert_state("let a = (test::call(), 'p'ˇ);", Mode::Insert);

    cx.set_state("let a = [test::call(ˇ), 300];", Mode::Normal);
    cx.simulate_keystrokes("c i a");
    cx.assert_state("let a = [ˇ, 300];", Mode::Insert);

    cx.set_state(
        "let a = vec![Vec::new(), vecˇ![test::call(), 300]];",
        Mode::Normal,
    );
    cx.simulate_keystrokes("c a a");
    cx.assert_state("let a = vec![Vec::new()ˇ];", Mode::Insert);

    // Cursor immediately before / after brackets
    cx.set_state("let a = [test::call(first_arg)ˇ]", Mode::Normal);
    cx.simulate_keystrokes("v i a");
    cx.assert_state("let a = [«test::call(first_arg)ˇ»]", Mode::Visual);

    cx.set_state("let a = [test::callˇ(first_arg)]", Mode::Normal);
    cx.simulate_keystrokes("v i a");
    cx.assert_state("let a = [«test::call(first_arg)ˇ»]", Mode::Visual);
}

#[gpui::test]
async fn test_indent_object(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Base use case
    cx.set_state(
        indoc! {"
                fn boop() {
                    // Comment
                    baz();ˇ

                    loop {
                        bar(1);
                        bar(2);
                    }

                    result
                }
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i i");
    cx.assert_state(
        indoc! {"
                fn boop() {
                «    // Comment
                    baz();

                    loop {
                        bar(1);
                        bar(2);
                    }

                    resultˇ»
                }
            "},
        Mode::Visual,
    );

    // Around indent (include line above)
    cx.set_state(
        indoc! {"
                const ABOVE: str = true;
                fn boop() {

                    hello();
                    worˇld()
                }
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a i");
    cx.assert_state(
        indoc! {"
                const ABOVE: str = true;
                «fn boop() {

                    hello();
                    world()ˇ»
                }
            "},
        Mode::Visual,
    );

    // Around indent (include line above & below)
    cx.set_state(
        indoc! {"
                const ABOVE: str = true;
                fn boop() {
                    hellˇo();
                    world()

                }
                const BELOW: str = true;
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c a shift-i");
    cx.assert_state(
        indoc! {"
                const ABOVE: str = true;
                ˇ
                const BELOW: str = true;
            "},
        Mode::Insert,
    );
}

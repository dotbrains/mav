use super::*;

#[gpui::test]
async fn test_word_motions(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    // «
    // ˇ
    // »
    cx.set_state(
        indoc! {"
            Th«e quiˇ»ck brown
            fox jumps over
            the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("w");

    cx.assert_state(
        indoc! {"
            The qu«ick ˇ»brown
            fox jumps over
            the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("w");

    cx.assert_state(
        indoc! {"
            The quick «brownˇ»
            fox jumps over
            the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("2 b");

    cx.assert_state(
        indoc! {"
            The «ˇquick »brown
            fox jumps over
            the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("down e up");

    cx.assert_state(
        indoc! {"
            The quicˇk brown
            fox jumps over
            the lazy dog."},
        Mode::HelixNormal,
    );

    cx.set_state("aa\n  «ˇbb»", Mode::HelixNormal);

    cx.simulate_keystroke("b");

    cx.assert_state("aa\n«ˇ  »bb", Mode::HelixNormal);
}

#[gpui::test]
async fn test_next_subword_start(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Setup custom keybindings for subword motions so we can use the bindings
    // in `simulate_keystroke`.
    cx.update(|_window, cx| {
        cx.bind_keys([KeyBinding::new(
            "w",
            crate::motion::NextSubwordStart {
                ignore_punctuation: false,
            },
            None,
        )]);
    });

    cx.set_state("ˇfoo.bar", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("«fooˇ».bar", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("foo«.ˇ»bar", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("foo.«barˇ»", Mode::HelixNormal);

    cx.set_state("ˇfoo(bar)", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("«fooˇ»(bar)", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("foo«(ˇ»bar)", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("foo(«barˇ»)", Mode::HelixNormal);

    cx.set_state("ˇfoo_bar_baz", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("«foo_ˇ»bar_baz", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("foo_«bar_ˇ»baz", Mode::HelixNormal);

    cx.set_state("ˇfooBarBaz", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("«fooˇ»BarBaz", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("foo«Barˇ»Baz", Mode::HelixNormal);

    cx.set_state("ˇfoo;bar", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("«fooˇ»;bar", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("foo«;ˇ»bar", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("foo;«barˇ»", Mode::HelixNormal);

    cx.set_state("ˇ<?php\n\n$someVariable = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("«<?ˇ»php\n\n$someVariable = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("<?«phpˇ»\n\n$someVariable = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("<?php\n\n«$ˇ»someVariable = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("<?php\n\n$«someˇ»Variable = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("<?php\n\n$some«Variable ˇ»= 2;", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("<?php\n\n$someVariable «= ˇ»2;", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("<?php\n\n$someVariable = «2ˇ»;", Mode::HelixNormal);
    cx.simulate_keystroke("w");
    cx.assert_state("<?php\n\n$someVariable = 2«;ˇ»", Mode::HelixNormal);
}

#[gpui::test]
async fn test_next_subword_end(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Setup custom keybindings for subword motions so we can use the bindings
    // in `simulate_keystroke`.
    cx.update(|_window, cx| {
        cx.bind_keys([KeyBinding::new(
            "e",
            crate::motion::NextSubwordEnd {
                ignore_punctuation: false,
            },
            None,
        )]);
    });

    cx.set_state("ˇfoo.bar", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("«fooˇ».bar", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("foo«.ˇ»bar", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("foo.«barˇ»", Mode::HelixNormal);

    cx.set_state("ˇfoo(bar)", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("«fooˇ»(bar)", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("foo«(ˇ»bar)", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("foo(«barˇ»)", Mode::HelixNormal);

    cx.set_state("ˇfoo_bar_baz", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("«fooˇ»_bar_baz", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("foo«_barˇ»_baz", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("foo_bar«_bazˇ»", Mode::HelixNormal);

    cx.set_state("ˇfooBarBaz", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("«fooˇ»BarBaz", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("foo«Barˇ»Baz", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("fooBar«Bazˇ»", Mode::HelixNormal);

    cx.set_state("ˇfoo;bar", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("«fooˇ»;bar", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("foo«;ˇ»bar", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("foo;«barˇ»", Mode::HelixNormal);

    cx.set_state("ˇ<?php\n\n$someVariable = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("«<?ˇ»php\n\n$someVariable = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("<?«phpˇ»\n\n$someVariable = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("<?php\n\n«$ˇ»someVariable = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("<?php\n\n$«someˇ»Variable = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("<?php\n\n$some«Variableˇ» = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("<?php\n\n$someVariable« =ˇ» 2;", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("<?php\n\n$someVariable =« 2ˇ»;", Mode::HelixNormal);
    cx.simulate_keystroke("e");
    cx.assert_state("<?php\n\n$someVariable = 2«;ˇ»", Mode::HelixNormal);
}

#[gpui::test]
async fn test_previous_subword_start(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Setup custom keybindings for subword motions so we can use the bindings
    // in `simulate_keystroke`.
    cx.update(|_window, cx| {
        cx.bind_keys([KeyBinding::new(
            "b",
            crate::motion::PreviousSubwordStart {
                ignore_punctuation: false,
            },
            None,
        )]);
    });

    cx.set_state("foo.barˇ", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("foo.«ˇbar»", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("foo«ˇ.»bar", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("«ˇfoo».bar", Mode::HelixNormal);

    cx.set_state("foo(bar)ˇ", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("foo(bar«ˇ)»", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("foo(«ˇbar»)", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("foo«ˇ(»bar)", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("«ˇfoo»(bar)", Mode::HelixNormal);

    cx.set_state("foo_bar_bazˇ", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("foo_bar_«ˇbaz»", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("foo_«ˇbar_»baz", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("«ˇfoo_»bar_baz", Mode::HelixNormal);

    cx.set_state("foo;barˇ", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("foo;«ˇbar»", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("foo«ˇ;»bar", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("«ˇfoo»;bar", Mode::HelixNormal);

    cx.set_state("<?php\n\n$someVariable = 2;ˇ", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("<?php\n\n$someVariable = 2«ˇ;»", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("<?php\n\n$someVariable = «ˇ2»;", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("<?php\n\n$someVariable «ˇ= »2;", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("<?php\n\n$some«ˇVariable »= 2;", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("<?php\n\n$«ˇsome»Variable = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("<?php\n\n«ˇ$»someVariable = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("<?«ˇphp»\n\n$someVariable = 2;", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("«ˇ<?»php\n\n$someVariable = 2;", Mode::HelixNormal);

    cx.set_state("fooBarBazˇ", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("fooBar«ˇBaz»", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("foo«ˇBar»Baz", Mode::HelixNormal);
    cx.simulate_keystroke("b");
    cx.assert_state("«ˇfoo»BarBaz", Mode::HelixNormal);
}

#[gpui::test]
async fn test_previous_subword_end(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Setup custom keybindings for subword motions so we can use the bindings
    // in `simulate_keystrokes`.
    cx.update(|_window, cx| {
        cx.bind_keys([KeyBinding::new(
            "g e",
            crate::motion::PreviousSubwordEnd {
                ignore_punctuation: false,
            },
            None,
        )]);
    });

    cx.set_state("foo.barˇ", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foo.«ˇbar»", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foo«ˇ.»bar", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("«ˇfoo».bar", Mode::HelixNormal);

    cx.set_state("foo(bar)ˇ", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foo(bar«ˇ)»", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foo(«ˇbar»)", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foo«ˇ(»bar)", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("«ˇfoo»(bar)", Mode::HelixNormal);

    cx.set_state("foo_bar_bazˇ", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foo_bar«ˇ_baz»", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foo«ˇ_bar»_baz", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("«ˇfoo»_bar_baz", Mode::HelixNormal);

    cx.set_state("foo;barˇ", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foo;«ˇbar»", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foo«ˇ;»bar", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("«ˇfoo»;bar", Mode::HelixNormal);

    cx.set_state("<?php\n\n$someVariable = 2;ˇ", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("<?php\n\n$someVariable = 2«ˇ;»", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("<?php\n\n$someVariable =«ˇ 2»;", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("<?php\n\n$someVariable«ˇ =» 2;", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("<?php\n\n$some«ˇVariable» = 2;", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("<?php\n\n$«ˇsome»Variable = 2;", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("<?php\n\n«ˇ$»someVariable = 2;", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("<?«ˇphp»\n\n$someVariable = 2;", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("«ˇ<?»php\n\n$someVariable = 2;", Mode::HelixNormal);

    cx.set_state("fooBarBazˇ", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("fooBar«ˇBaz»", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foo«ˇBar»Baz", Mode::HelixNormal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("«ˇfoo»BarBaz", Mode::HelixNormal);
}

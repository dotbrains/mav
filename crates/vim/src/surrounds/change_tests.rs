use gpui::KeyBinding;
use indoc::indoc;

use crate::{
    PushAddSurrounds,
    object::{AnyBrackets, AnyQuotes, MiniBrackets, MiniQuotes},
    state::Mode,
    test::VimTestContext,
};

#[gpui::test]
async fn test_change_surrounds(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
            The {quˇick} brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s { [");
    cx.assert_state(
        indoc! {"
            The ˇ[ quick ] brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    // test multi cursor change surrounds
    cx.set_state(
        indoc! {"
            The {quˇick} brown
            fox jumps over
            the {laˇzy} dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s { [");
    cx.assert_state(
        indoc! {"
            The ˇ[ quick ] brown
            fox jumps over
            the ˇ[ lazy ] dog."},
        Mode::Normal,
    );

    // test multi cursor delete different surrounds with after cursor
    cx.set_state(
        indoc! {"
            Thˇe {quick} brown
            fox jumps over
            the {laˇzy} dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s { [");
    cx.assert_state(
        indoc! {"
            The ˇ[ quick ] brown
            fox jumps over
            the ˇ[ lazy ] dog."},
        Mode::Normal,
    );

    // test multi cursor change surrount with not around
    cx.set_state(
        indoc! {"
            Thˇe { quick } brown
            fox jumps over
            the {laˇzy} dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s { ]");
    cx.assert_state(
        indoc! {"
            The ˇ[quick] brown
            fox jumps over
            the ˇ[lazy] dog."},
        Mode::Normal,
    );

    // test multi cursor change with not exist surround
    cx.set_state(
        indoc! {"
            The {quˇick} brown
            fox jumps over
            the [laˇzy] dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s [ '");
    cx.assert_state(
        indoc! {"
            The {quick} brown
            fox jumps over
            the ˇ'lazy' dog."},
        Mode::Normal,
    );

    // test change nesting surrounds
    cx.set_state(
        indoc! {"
            fn test_surround() {
                ifˇ 2 > 1 {
                    ˇprintln!(\"it is fine\");
                }
            };"},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s } ]");
    cx.assert_state(
        indoc! {"
            fn test_surround() ˇ[
                if 2 > 1 ˇ[
                    println!(\"it is fine\");
                ]
            ];"},
        Mode::Normal,
    );

    // test spaces with quote change surrounds
    cx.set_state(
        indoc! {"
            fn test_surround() {
                \"ˇ \"
            };"},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s \" '");
    cx.assert_state(
        indoc! {"
            fn test_surround() {
                ˇ' '
            };"},
        Mode::Normal,
    );

    // Currently, the same test case but using the closing bracket `]`
    // actually removes a whitespace before the closing bracket, something
    // that might need to be fixed?
    cx.set_state(
        indoc! {"
            fn test_surround() {
                ifˇ 2 > 1 {
                    ˇprintln!(\"it is fine\");
                }
            };"},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s { ]");
    cx.assert_state(
        indoc! {"
            fn test_surround() ˇ[
                if 2 > 1 ˇ[
                    println!(\"it is fine\");
                ]
            ];"},
        Mode::Normal,
    );

    // test change quotes.
    cx.set_state(indoc! {"'  ˇstr  '"}, Mode::Normal);
    cx.simulate_keystrokes("c s ' \"");
    cx.assert_state(indoc! {"ˇ\"  str  \""}, Mode::Normal);

    // test multi cursor change quotes
    cx.set_state(
        indoc! {"
            '  ˇstr  '
            some example text here
            ˇ'  str  '
        "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s ' \"");
    cx.assert_state(
        indoc! {"
            ˇ\"  str  \"
            some example text here
            ˇ\"  str  \"
        "},
        Mode::Normal,
    );

    // test quote to bracket spacing.
    cx.set_state(indoc! {"'ˇfoobar'"}, Mode::Normal);
    cx.simulate_keystrokes("c s ' {");
    cx.assert_state(indoc! {"ˇ{ foobar }"}, Mode::Normal);

    cx.set_state(indoc! {"'ˇfoobar'"}, Mode::Normal);
    cx.simulate_keystrokes("c s ' }");
    cx.assert_state(indoc! {"ˇ{foobar}"}, Mode::Normal);

    cx.set_state(indoc! {"I'm 'goˇod'"}, Mode::Normal);
    cx.simulate_keystrokes("c s ' \"");
    cx.assert_state(indoc! {"I'm ˇ\"good\""}, Mode::Normal);

    cx.set_state(indoc! {"I'm 'goˇod'"}, Mode::Normal);
    cx.simulate_keystrokes("c s ' {");
    cx.assert_state(indoc! {"I'm ˇ{ good }"}, Mode::Normal);
}

#[gpui::test]
async fn test_change_surrounds_any_brackets(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Update keybindings so that using `csb` triggers Vim's `AnyBrackets`
    // action.
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "b",
            AnyBrackets,
            Some("vim_operator == a || vim_operator == i || vim_operator == cs"),
        )]);
    });

    cx.set_state(indoc! {"{braˇcketed}"}, Mode::Normal);
    cx.simulate_keystrokes("c s b [");
    cx.assert_state(indoc! {"ˇ[ bracketed ]"}, Mode::Normal);

    cx.set_state(indoc! {"[braˇcketed]"}, Mode::Normal);
    cx.simulate_keystrokes("c s b {");
    cx.assert_state(indoc! {"ˇ{ bracketed }"}, Mode::Normal);

    cx.set_state(indoc! {"<braˇcketed>"}, Mode::Normal);
    cx.simulate_keystrokes("c s b [");
    cx.assert_state(indoc! {"ˇ[ bracketed ]"}, Mode::Normal);

    cx.set_state(indoc! {"(braˇcketed)"}, Mode::Normal);
    cx.simulate_keystrokes("c s b [");
    cx.assert_state(indoc! {"ˇ[ bracketed ]"}, Mode::Normal);

    cx.set_state(indoc! {"(< name: ˇ'Mav' >)"}, Mode::Normal);
    cx.simulate_keystrokes("c s b }");
    cx.assert_state(indoc! {"(ˇ{ name: 'Mav' })"}, Mode::Normal);

    cx.set_state(
        indoc! {"
            (< name: ˇ'Mav' >)
            (< nˇame: 'DeltaDB' >)
        "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s b {");
    cx.set_state(
        indoc! {"
            (ˇ{ name: 'Mav' })
            (ˇ{ name: 'DeltaDB' })
        "},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_change_surrounds_mini_brackets(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Update keybindings so that using `csb` triggers Vim's `MiniBrackets` action.
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "b",
            MiniBrackets,
            Some("vim_operator == a || vim_operator == i || vim_operator == cs"),
        )]);
    });

    cx.set_state(indoc! {"{braˇcketed}"}, Mode::Normal);
    cx.simulate_keystrokes("c s b [");
    cx.assert_state(indoc! {"ˇ[ bracketed ]"}, Mode::Normal);

    cx.set_state(indoc! {"[braˇcketed]"}, Mode::Normal);
    cx.simulate_keystrokes("c s b {");
    cx.assert_state(indoc! {"ˇ{ bracketed }"}, Mode::Normal);

    cx.set_state(indoc! {"<braˇcketed>"}, Mode::Normal);
    cx.simulate_keystrokes("c s b [");
    cx.assert_state(indoc! {"ˇ[ bracketed ]"}, Mode::Normal);

    cx.set_state(indoc! {"(braˇcketed)"}, Mode::Normal);
    cx.simulate_keystrokes("c s b [");
    cx.assert_state(indoc! {"ˇ[ bracketed ]"}, Mode::Normal);

    cx.set_state(indoc! {"(<ˇMav>)"}, Mode::Normal);
    cx.simulate_keystrokes("c s b )");
    cx.assert_state(indoc! {"(ˇ(Mav))"}, Mode::Normal);

    cx.set_state(
        indoc! {"
                (<ˇMav>)
                (<ˇDeltaDB>)
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s b (");
    cx.assert_state(
        indoc! {"
                (ˇ( Mav ))
                (ˇ( DeltaDB ))
            "},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_change_surrounds_any_quotes(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Update keybindings so that using `csq` triggers Vim's `AnyQuotes` action.
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "q",
            AnyQuotes,
            Some("vim_operator == a || vim_operator == i || vim_operator == cs"),
        )]);
    });

    cx.set_state(indoc! {"'  ˇstr  '"}, Mode::Normal);
    cx.simulate_keystrokes("c s q \"");
    cx.assert_state(indoc! {"ˇ\"  str  \""}, Mode::Normal);

    cx.set_state(indoc! {"`  ˇstr  `"}, Mode::Normal);
    cx.simulate_keystrokes("c s q '");
    cx.assert_state(indoc! {"ˇ'  str  '"}, Mode::Normal);

    cx.set_state(indoc! {"\"  ˇstr  \""}, Mode::Normal);
    cx.simulate_keystrokes("c s q `");
    cx.assert_state(indoc! {"ˇ`  str  `"}, Mode::Normal);
}

#[gpui::test]
async fn test_change_surrounds_mini_quotes(cx: &mut gpui::TestAppContext) {
    // NOTE: needs TypeScript test cx to recognize single/backquotes
    let mut cx = VimTestContext::new_typescript(cx).await;

    // Update keybindings so that using `csq` triggers Vim's `MiniQuotes` action.
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "q",
            MiniQuotes,
            Some("vim_operator == a || vim_operator == i || vim_operator == cs"),
        )]);
    });
    cx.set_state(indoc! {"'  ˇstr  '"}, Mode::Normal);
    cx.simulate_keystrokes("c s q \"");
    cx.assert_state(indoc! {"ˇ\"  str  \""}, Mode::Normal);

    cx.set_state(indoc! {"`  ˇstr  `"}, Mode::Normal);
    cx.simulate_keystrokes("c s q '");
    cx.assert_state(indoc! {"ˇ'  str  '"}, Mode::Normal);

    cx.set_state(indoc! {"\"  ˇstr  \""}, Mode::Normal);
    cx.simulate_keystrokes("c s q `");
    cx.assert_state(indoc! {"ˇ`  str  `"}, Mode::Normal);
}

// The following test cases all follow tpope/vim-surround's behaviour
// and are more focused on how whitespace is handled.
#[gpui::test]
async fn test_change_surrounds_vim(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Changing quote to quote should never change the surrounding
    // whitespace.
    cx.set_state(indoc! {"'  ˇa  '"}, Mode::Normal);
    cx.simulate_keystrokes("c s ' \"");
    cx.assert_state(indoc! {"ˇ\"  a  \""}, Mode::Normal);

    cx.set_state(indoc! {"\"  ˇa  \""}, Mode::Normal);
    cx.simulate_keystrokes("c s \" '");
    cx.assert_state(indoc! {"ˇ'  a  '"}, Mode::Normal);

    // Changing quote to bracket adds one more space when the opening
    // bracket is used, does not affect whitespace when the closing bracket
    // is used.
    cx.set_state(indoc! {"'  ˇa  '"}, Mode::Normal);
    cx.simulate_keystrokes("c s ' {");
    cx.assert_state(indoc! {"ˇ{   a   }"}, Mode::Normal);

    cx.set_state(indoc! {"'  ˇa  '"}, Mode::Normal);
    cx.simulate_keystrokes("c s ' }");
    cx.assert_state(indoc! {"ˇ{  a  }"}, Mode::Normal);

    // Changing bracket to quote should remove all space when the
    // opening bracket is used and preserve all space when the
    // closing one is used.
    cx.set_state(indoc! {"{  ˇa  }"}, Mode::Normal);
    cx.simulate_keystrokes("c s { '");
    cx.assert_state(indoc! {"ˇ'a'"}, Mode::Normal);

    cx.set_state(indoc! {"{  ˇa  }"}, Mode::Normal);
    cx.simulate_keystrokes("c s } '");
    cx.assert_state(indoc! {"ˇ'  a  '"}, Mode::Normal);

    // Changing bracket to bracket follows these rules:
    // * opening → opening – keeps only one space.
    // * opening → closing – removes all space.
    // * closing → opening – adds one space.
    // * closing → closing – does not change space.
    cx.set_state(indoc! {"{   ˇa   }"}, Mode::Normal);
    cx.simulate_keystrokes("c s { [");
    cx.assert_state(indoc! {"ˇ[ a ]"}, Mode::Normal);

    cx.set_state(indoc! {"{   ˇa   }"}, Mode::Normal);
    cx.simulate_keystrokes("c s { ]");
    cx.assert_state(indoc! {"ˇ[a]"}, Mode::Normal);

    cx.set_state(indoc! {"{  ˇa  }"}, Mode::Normal);
    cx.simulate_keystrokes("c s } [");
    cx.assert_state(indoc! {"ˇ[   a   ]"}, Mode::Normal);

    cx.set_state(indoc! {"{  ˇa  }"}, Mode::Normal);
    cx.simulate_keystrokes("c s } ]");
    cx.assert_state(indoc! {"ˇ[  a  ]"}, Mode::Normal);
}

#[gpui::test]
async fn test_surrounds(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s i w [");
    cx.assert_state(
        indoc! {"
            The ˇ[ quick ] brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.simulate_keystrokes("c s [ }");
    cx.assert_state(
        indoc! {"
            The ˇ{quick} brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.simulate_keystrokes("d s {");
    cx.assert_state(
        indoc! {"
            The ˇquick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.simulate_keystrokes("u");
    cx.assert_state(
        indoc! {"
            The ˇ{quick} brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
}

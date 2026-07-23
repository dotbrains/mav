use gpui::KeyBinding;
use indoc::indoc;

use crate::{
    PushAddSurrounds,
    object::{AnyBrackets, AnyQuotes, MiniBrackets, MiniQuotes},
    state::Mode,
    test::VimTestContext,
};

#[gpui::test]
async fn test_delete_surrounds(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // test delete surround
    cx.set_state(
        indoc! {"
            The {quˇick} brown
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

    // test delete not exist surrounds
    cx.set_state(
        indoc! {"
            The {quˇick} brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("d s [");
    cx.assert_state(
        indoc! {"
            The {quˇick} brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    // test delete surround forward exist, in the surrounds plugin of other editors,
    // the bracket pair in front of the current line will be deleted here, which is not implemented at the moment
    cx.set_state(
        indoc! {"
            The {quick} brˇown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("d s {");
    cx.assert_state(
        indoc! {"
            The {quick} brˇown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    // test cursor delete inner surrounds
    cx.set_state(
        indoc! {"
            The { quick brown
            fox jumˇps over }
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

    // test multi cursor delete surrounds
    cx.set_state(
        indoc! {"
            The [quˇick] brown
            fox jumps over
            the [laˇzy] dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("d s ]");
    cx.assert_state(
        indoc! {"
            The ˇquick brown
            fox jumps over
            the ˇlazy dog."},
        Mode::Normal,
    );

    // test multi cursor delete surrounds with around
    cx.set_state(
        indoc! {"
            Tˇhe [ quick ] brown
            fox jumps over
            the [laˇzy] dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("d s [");
    cx.assert_state(
        indoc! {"
            The ˇquick brown
            fox jumps over
            the ˇlazy dog."},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
            Tˇhe [ quick ] brown
            fox jumps over
            the [laˇzy ] dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("d s [");
    cx.assert_state(
        indoc! {"
            The ˇquick brown
            fox jumps over
            the ˇlazy dog."},
        Mode::Normal,
    );

    // test multi cursor delete different surrounds
    // the pair corresponding to the two cursors is the same,
    // so they are combined into one cursor
    cx.set_state(
        indoc! {"
            The [quˇick] brown
            fox jumps over
            the {laˇzy} dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("d s {");
    cx.assert_state(
        indoc! {"
            The [quick] brown
            fox jumps over
            the ˇlazy dog."},
        Mode::Normal,
    );

    // test delete surround with multi cursor and nest surrounds
    cx.set_state(
        indoc! {"
            fn test_surround() {
                ifˇ 2 > 1 {
                    ˇprintln!(\"it is fine\");
                };
            }"},
        Mode::Normal,
    );
    cx.simulate_keystrokes("d s }");
    cx.assert_state(
        indoc! {"
            fn test_surround() ˇ
                if 2 > 1 ˇ
                    println!(\"it is fine\");
                ;
            "},
        Mode::Normal,
    );
}

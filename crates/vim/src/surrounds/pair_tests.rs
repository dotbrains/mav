use gpui::KeyBinding;
use indoc::indoc;

use crate::{
    PushAddSurrounds,
    object::{AnyBrackets, AnyQuotes, MiniBrackets, MiniQuotes},
    state::Mode,
    test::VimTestContext,
};

#[gpui::test]
async fn test_surround_aliases(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // add aliases
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s i w b");
    cx.assert_state(
        indoc! {"
            The ˇ(quick) brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s i w B");
    cx.assert_state(
        indoc! {"
            The ˇ{quick} brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s i w a");
    cx.assert_state(
        indoc! {"
            The ˇ<quick> brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s i w r");
    cx.assert_state(
        indoc! {"
            The ˇ[quick] brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    // change aliases
    cx.set_state(
        indoc! {"
            The {quˇick} brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s { b");
    cx.assert_state(
        indoc! {"
            The ˇ(quick) brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
            The (quˇick) brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s ( B");
    cx.assert_state(
        indoc! {"
            The ˇ{quick} brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
            The (quˇick) brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s ( a");
    cx.assert_state(
        indoc! {"
            The ˇ<quick> brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
            The <quˇick> brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s < b");
    cx.assert_state(
        indoc! {"
            The ˇ(quick) brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
            The (quˇick) brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s ( r");
    cx.assert_state(
        indoc! {"
            The ˇ[quick] brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
            The [quˇick] brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("c s [ b");
    cx.assert_state(
        indoc! {"
            The ˇ(quick) brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    // delete alias
    cx.set_state(
        indoc! {"
            The {quˇick} brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("d s B");
    cx.assert_state(
        indoc! {"
            The ˇquick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
            The (quˇick) brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("d s b");
    cx.assert_state(
        indoc! {"
            The ˇquick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
            The [quˇick] brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("d s r");
    cx.assert_state(
        indoc! {"
            The ˇquick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
            The <quˇick> brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("d s a");
    cx.assert_state(
        indoc! {"
            The ˇquick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
}

#[test]
fn test_surround_pair_for_char() {
    use super::{SURROUND_PAIRS, surround_pair_for_char_helix, surround_pair_for_char_vim};

    fn as_tuple(pair: Option<super::SurroundPair>) -> Option<(char, char)> {
        pair.map(|p| (p.open, p.close))
    }

    assert_eq!(as_tuple(surround_pair_for_char_vim('b')), Some(('(', ')')));
    assert_eq!(as_tuple(surround_pair_for_char_vim('B')), Some(('{', '}')));
    assert_eq!(as_tuple(surround_pair_for_char_vim('r')), Some(('[', ']')));
    assert_eq!(as_tuple(surround_pair_for_char_vim('a')), Some(('<', '>')));

    assert_eq!(surround_pair_for_char_vim('m'), None);

    for pair in SURROUND_PAIRS {
        assert_eq!(
            as_tuple(surround_pair_for_char_vim(pair.open)),
            Some((pair.open, pair.close))
        );
        assert_eq!(
            as_tuple(surround_pair_for_char_vim(pair.close)),
            Some((pair.open, pair.close))
        );
    }

    // Test unknown char returns None
    assert_eq!(surround_pair_for_char_vim('x'), None);

    // Helix resolves literal chars and falls back to symmetric pairs.
    assert_eq!(
        as_tuple(surround_pair_for_char_helix('*')),
        Some(('*', '*'))
    );
    assert_eq!(surround_pair_for_char_helix('m'), None);
}

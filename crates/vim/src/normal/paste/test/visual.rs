
use crate::{
    state::{Mode, Register},
    test::{NeovimBackedTestContext, VimTestContext},
};
use gpui::ClipboardItem;
use indoc::indoc;
use language::{LanguageName, language_settings::LanguageSettingsContent};
use settings::{SettingsStore, UseSystemClipboard};
#[gpui::test]
async fn test_paste_visual(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // copy in visual mode
    cx.set_shared_state(indoc! {"
                The quick brown
                fox jˇumps over
                the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("v i w y").await;
    cx.shared_state().await.assert_eq(indoc! {"
                The quick brown
                fox ˇjumps over
                the lazy dog"});
    // paste in visual mode
    cx.simulate_shared_keystrokes("w v i w p").await;
    cx.shared_state().await.assert_eq(indoc! {"
                The quick brown
                fox jumps jumpˇs
                the lazy dog"});
    cx.shared_clipboard().await.assert_eq("over");
    // paste in visual line mode
    cx.simulate_shared_keystrokes("up shift-v shift-p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            ˇover
            fox jumps jumps
            the lazy dog"});
    cx.shared_clipboard().await.assert_eq("over");
    // paste in visual block mode
    cx.simulate_shared_keystrokes("ctrl-v down down p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            oveˇrver
            overox jumps jumps
            overhe lazy dog"});

    // copy in visual line mode
    cx.set_shared_state(indoc! {"
                The quick brown
                fox juˇmps over
                the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("shift-v d").await;
    cx.shared_state().await.assert_eq(indoc! {"
                The quick brown
                the laˇzy dog"});
    // paste in visual mode
    cx.simulate_shared_keystrokes("v i w p").await;
    cx.shared_state().await.assert_eq(indoc! {"
                The quick brown
                the•
                ˇfox jumps over
                 dog"});
    cx.shared_clipboard().await.assert_eq("lazy");
    cx.set_shared_state(indoc! {"
            The quick brown
            fox juˇmps over
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("shift-v d").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown
            the laˇzy dog"});
    cx.shared_clipboard().await.assert_eq("fox jumps over\n");
    // paste in visual line mode
    cx.simulate_shared_keystrokes("k shift-v p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            ˇfox jumps over
            the lazy dog"});
    cx.shared_clipboard().await.assert_eq("The quick brown\n");

    // Copy line and paste in visual mode, with cursor on newline character.
    cx.set_shared_state(indoc! {"
            ˇThe quick brown
            fox jumps over
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("y y shift-v j $ p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            ˇThe quick brown
            the lazy dog"});
}

#[gpui::test]
async fn test_paste_visual_block(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    // copy in visual block mode
    cx.set_shared_state(indoc! {"
            The ˇquick brown
            fox jumps over
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("ctrl-v 2 j y").await;
    cx.shared_clipboard().await.assert_eq("q\nj\nl");
    cx.simulate_shared_keystrokes("p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The qˇquick brown
            fox jjumps over
            the llazy dog"});
    cx.simulate_shared_keystrokes("v i w shift-p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The ˇq brown
            fox jjjumps over
            the lllazy dog"});
    cx.simulate_shared_keystrokes("v i w shift-p").await;

    cx.set_shared_state(indoc! {"
            The ˇquick brown
            fox jumps over
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("ctrl-v j y").await;
    cx.shared_clipboard().await.assert_eq("q\nj");
    cx.simulate_shared_keystrokes("l ctrl-v 2 j shift-p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The qˇqick brown
            fox jjmps over
            the lzy dog"});

    cx.simulate_shared_keystrokes("shift-v p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            ˇq
            j
            fox jjmps over
            the lzy dog"});
}

#[gpui::test]
async fn test_paste_indent(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_typescript(cx).await;

    cx.set_state(
        indoc! {"
            class A {ˇ
            }
        "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("o a ( ) { escape");
    cx.assert_state(
        indoc! {"
            class A {
                a()ˇ{}
            }
            "},
        Mode::Normal,
    );
    // cursor goes to the first non-blank character in the line;
    cx.simulate_keystrokes("y y p");
    cx.assert_state(
        indoc! {"
            class A {
                a(){}
                ˇa(){}
            }
            "},
        Mode::Normal,
    );
    // indentation is preserved when pasting
    cx.simulate_keystrokes("u shift-v up y shift-p");
    cx.assert_state(
        indoc! {"
                ˇclass A {
                    a(){}
                class A {
                    a(){}
                }
            "},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_paste_auto_indent(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
            mod some_module {
                ˇfn main() {
                }
            }
            "},
        Mode::Normal,
    );
    // default auto indentation
    cx.simulate_keystrokes("y y p");
    cx.assert_state(
        indoc! {"
                mod some_module {
                    fn main() {
                        ˇfn main() {
                    }
                }
                "},
        Mode::Normal,
    );
    // back to previous state
    cx.simulate_keystrokes("u u");
    cx.assert_state(
        indoc! {"
                mod some_module {
                    ˇfn main() {
                    }
                }
                "},
        Mode::Normal,
    );
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings.project.all_languages.languages.0.insert(
                LanguageName::new_static("Rust").0.to_string(),
                LanguageSettingsContent {
                    auto_indent_on_paste: Some(false),
                    ..Default::default()
                },
            );
        });
    });
    // auto indentation turned off
    cx.simulate_keystrokes("y y p");
    cx.assert_state(
        indoc! {"
                mod some_module {
                    fn main() {
                    ˇfn main() {
                    }
                }
                "},
        Mode::Normal,
    );
}

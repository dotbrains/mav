use gpui::{KeyBinding, TestAppContext, UpdateGlobal};
use indoc::indoc;
use settings::SettingsStore;

use crate::{
    motion,
    state::Mode::{self},
    test::{NeovimBackedTestContext, VimTestContext},
};
use language;

#[gpui::test]
async fn test_repeated_word(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    for count in 1..=5 {
        cx.simulate_at_each_offset(
            &format!("{count} w"),
            indoc! {"
                ˇThe quˇickˇ browˇn
                ˇ
                ˇfox ˇjumpsˇ-ˇoˇver
                ˇthe lazy dog
            "},
        )
        .await
        .assert_matches();
    }
}

#[gpui::test]
async fn test_h_through_unicode(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset("h", "Testˇ├ˇ──ˇ┐ˇTest")
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_f_and_t(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    for count in 1..=3 {
        let test_case = indoc! {"
            ˇaaaˇbˇ ˇbˇ   ˇbˇbˇ aˇaaˇbaaa
            ˇ    ˇbˇaaˇa ˇbˇbˇb
            ˇ
            ˇb
        "};

        cx.simulate_at_each_offset(&format!("{count} f b"), test_case)
            .await
            .assert_matches();

        cx.simulate_at_each_offset(&format!("{count} t b"), test_case)
            .await
            .assert_matches();
    }
}

#[gpui::test]
async fn test_capital_f_and_capital_t(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    let test_case = indoc! {"
        ˇaaaˇbˇ ˇbˇ   ˇbˇbˇ aˇaaˇbaaa
        ˇ    ˇbˇaaˇa ˇbˇbˇb
        ˇ•••
        ˇb
        "
    };

    for count in 1..=3 {
        cx.simulate_at_each_offset(&format!("{count} shift-f b"), test_case)
            .await
            .assert_matches();

        cx.simulate_at_each_offset(&format!("{count} shift-t b"), test_case)
            .await
            .assert_matches();
    }
}

#[gpui::test]
async fn test_f_and_t_smartcase(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.vim.get_or_insert_default().use_smartcase_find = Some(true);
        });
    });

    cx.assert_binding(
        "f p",
        indoc! {"ˇfmt.Println(\"Hello, World!\")"},
        Mode::Normal,
        indoc! {"fmt.ˇPrintln(\"Hello, World!\")"},
        Mode::Normal,
    );

    cx.assert_binding(
        "shift-f p",
        indoc! {"fmt.Printlnˇ(\"Hello, World!\")"},
        Mode::Normal,
        indoc! {"fmt.ˇPrintln(\"Hello, World!\")"},
        Mode::Normal,
    );

    cx.assert_binding(
        "t p",
        indoc! {"ˇfmt.Println(\"Hello, World!\")"},
        Mode::Normal,
        indoc! {"fmtˇ.Println(\"Hello, World!\")"},
        Mode::Normal,
    );

    cx.assert_binding(
        "shift-t p",
        indoc! {"fmt.Printlnˇ(\"Hello, World!\")"},
        Mode::Normal,
        indoc! {"fmt.Pˇrintln(\"Hello, World!\")"},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_percent(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset("%", "ˇconsole.logˇ(ˇvaˇrˇ)ˇ;")
        .await
        .assert_matches();
    cx.simulate_at_each_offset("%", "ˇconsole.logˇ(ˇ'var', ˇ[ˇ1, ˇ2, 3ˇ]ˇ)ˇ;")
        .await
        .assert_matches();
    cx.simulate_at_each_offset("%", "let result = curried_funˇ(ˇ)ˇ(ˇ)ˇ;")
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_percent_in_comment(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset("%", "// ˇconsole.logˇ(ˇvaˇrˇ)ˇ;")
        .await
        .assert_matches();
    cx.simulate_at_each_offset("%", "// ˇ{ ˇ{ˇ}ˇ }ˇ")
        .await
        .assert_matches();
    // Template-style brackets (like Liquid {% %} and {{ }})
    cx.simulate_at_each_offset("%", "ˇ{ˇ% block %ˇ}ˇ")
        .await
        .assert_matches();
    cx.simulate_at_each_offset("%", "ˇ{ˇ{ˇ var ˇ}ˇ}ˇ")
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_end_of_line_with_neovim(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // goes to current line end
    cx.set_shared_state(indoc! {"ˇaa\nbb\ncc"}).await;
    cx.simulate_shared_keystrokes("$").await;
    cx.shared_state().await.assert_eq("aˇa\nbb\ncc");

    // goes to next line end
    cx.simulate_shared_keystrokes("2 $").await;
    cx.shared_state().await.assert_eq("aa\nbˇb\ncc");

    // try to exceed the final line.
    cx.simulate_shared_keystrokes("4 $").await;
    cx.shared_state().await.assert_eq("aa\nbb\ncˇc");
}

#[gpui::test]
async fn test_subword_motions(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.update(|_, cx| {
        cx.bind_keys(vec![
            KeyBinding::new(
                "w",
                motion::NextSubwordStart {
                    ignore_punctuation: false,
                },
                Some("Editor && VimControl && !VimWaiting && !menu"),
            ),
            KeyBinding::new(
                "b",
                motion::PreviousSubwordStart {
                    ignore_punctuation: false,
                },
                Some("Editor && VimControl && !VimWaiting && !menu"),
            ),
            KeyBinding::new(
                "e",
                motion::NextSubwordEnd {
                    ignore_punctuation: false,
                },
                Some("Editor && VimControl && !VimWaiting && !menu"),
            ),
            KeyBinding::new(
                "g e",
                motion::PreviousSubwordEnd {
                    ignore_punctuation: false,
                },
                Some("Editor && VimControl && !VimWaiting && !menu"),
            ),
        ]);
    });

    cx.assert_binding_normal("w", indoc! {"ˇassert_binding"}, indoc! {"assert_ˇbinding"});
    // Special case: In 'cw', 'w' acts like 'e'
    cx.assert_binding(
        "c w",
        indoc! {"ˇassert_binding"},
        Mode::Normal,
        indoc! {"ˇ_binding"},
        Mode::Insert,
    );

    cx.assert_binding_normal("e", indoc! {"ˇassert_binding"}, indoc! {"asserˇt_binding"});

    // Subword end should stop at EOL
    cx.assert_binding_normal("e", indoc! {"foo_bˇar\nbaz"}, indoc! {"foo_baˇr\nbaz"});

    // Already at subword end, should move to next subword on next line
    cx.assert_binding_normal(
        "e",
        indoc! {"foo_barˇ\nbaz_qux"},
        indoc! {"foo_bar\nbaˇz_qux"},
    );

    // CamelCase at EOL
    cx.assert_binding_normal("e", indoc! {"fooˇBar\nbaz"}, indoc! {"fooBaˇr\nbaz"});

    cx.assert_binding_normal("b", indoc! {"assert_ˇbinding"}, indoc! {"ˇassert_binding"});

    cx.assert_binding_normal(
        "g e",
        indoc! {"assert_bindinˇg"},
        indoc! {"asserˇt_binding"},
    );
}

#[gpui::test]
async fn test_r(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇhello\n").await;
    cx.simulate_shared_keystrokes("r -").await;
    cx.shared_state().await.assert_eq("ˇ-ello\n");

    cx.set_shared_state("ˇhello\n").await;
    cx.simulate_shared_keystrokes("3 r -").await;
    cx.shared_state().await.assert_eq("--ˇ-lo\n");

    cx.set_shared_state("ˇhello\n").await;
    cx.simulate_shared_keystrokes("r - 2 l .").await;
    cx.shared_state().await.assert_eq("-eˇ-lo\n");

    cx.set_shared_state("ˇhello world\n").await;
    cx.simulate_shared_keystrokes("2 r - f w .").await;
    cx.shared_state().await.assert_eq("--llo -ˇ-rld\n");

    cx.set_shared_state("ˇhello world\n").await;
    cx.simulate_shared_keystrokes("2 0 r - ").await;
    cx.shared_state().await.assert_eq("ˇhello world\n");

    cx.set_shared_state("  helloˇ world\n").await;
    cx.simulate_shared_keystrokes("r enter").await;
    cx.shared_state().await.assert_eq("  hello\n ˇ world\n");

    cx.set_shared_state("  helloˇ world\n").await;
    cx.simulate_shared_keystrokes("2 r enter").await;
    cx.shared_state().await.assert_eq("  hello\n ˇ orld\n");
}

#[gpui::test]
async fn test_gq(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_neovim_option("textwidth=5").await;

    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings
                    .project
                    .all_languages
                    .defaults
                    .preferred_line_length = Some(5);
            });
        })
    });

    cx.set_shared_state("ˇth th th th th th\n").await;
    cx.simulate_shared_keystrokes("g q q").await;
    cx.shared_state().await.assert_eq("th th\nth th\nˇth th\n");

    cx.set_shared_state("ˇth th th th th th\nth th th th th th\n")
        .await;
    cx.simulate_shared_keystrokes("v j g q").await;
    cx.shared_state()
        .await
        .assert_eq("th th\nth th\nth th\nth th\nth th\nˇth th\n");
}

#[gpui::test]
async fn test_o_comment(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_neovim_option("filetype=rust").await;

    cx.set_shared_state("// helloˇ\n").await;
    cx.simulate_shared_keystrokes("o").await;
    cx.shared_state().await.assert_eq("// hello\n// ˇ\n");
    cx.simulate_shared_keystrokes("x escape shift-o").await;
    cx.shared_state().await.assert_eq("// hello\n// ˇ\n// x\n");
}

#[gpui::test]
async fn test_o_auto_indent_none(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.project.all_languages.defaults.auto_indent = Some(language::AutoIndentMode::None);
        });
    });

    // o: new line below starts at column 0 regardless of current indentation
    cx.set_state("    let xˇ = 1;", Mode::Normal);
    cx.simulate_keystrokes("o");
    cx.assert_state("    let x = 1;\nˇ", Mode::Insert);

    // O: new line above starts at column 0 regardless of current indentation
    cx.set_state("    let xˇ = 1;", Mode::Normal);
    cx.simulate_keystrokes("shift-o");
    cx.assert_state("ˇ\n    let x = 1;", Mode::Insert);

    // o on the first line: no crash and column 0
    cx.set_state("ˇfoo", Mode::Normal);
    cx.simulate_keystrokes("o");
    cx.assert_state("foo\nˇ", Mode::Insert);

    // O on the first line: no crash and column 0
    cx.set_state("ˇfoo", Mode::Normal);
    cx.simulate_keystrokes("shift-o");
    cx.assert_state("ˇ\nfoo", Mode::Insert);

    // o on an already-empty line: stays at column 0
    cx.set_state("fooˇ\n\nbar", Mode::Normal);
    cx.simulate_keystrokes("j o");
    cx.assert_state("foo\n\nˇ\nbar", Mode::Insert);
}

#[gpui::test]
async fn test_o_preserve_indent(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.project.all_languages.defaults.auto_indent =
                Some(language::AutoIndentMode::PreserveIndent);
        });
    });

    // o: new line below copies current line's indentation
    cx.set_state("    let xˇ = 1;", Mode::Normal);
    cx.simulate_keystrokes("o");
    cx.assert_state("    let x = 1;\n    ˇ", Mode::Insert);

    // O: new line above copies current line's indentation
    cx.set_state("    let xˇ = 1;", Mode::Normal);
    cx.simulate_keystrokes("shift-o");
    cx.assert_state("    ˇ\n    let x = 1;", Mode::Insert);
}

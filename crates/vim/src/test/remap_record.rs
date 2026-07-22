use super::*;

#[perf]
#[gpui::test]
async fn test_command_alias(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            let mut aliases = HashMap::default();
            aliases.insert("Q".to_string(), CommandAliasTarget::new("upper"));
            s.workspace.command_aliases = aliases
        });
    });

    cx.set_state("ˇhello world", Mode::Normal);
    cx.simulate_keystrokes(": Q");
    cx.set_state("ˇHello world", Mode::Normal);
}

#[perf]
#[gpui::test]
async fn test_remap_adjacent_dog_cat(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.update(|_, cx| {
        cx.bind_keys([
            KeyBinding::new(
                "d o g",
                workspace::SendKeystrokes("🐶".to_string()),
                Some("vim_mode == insert"),
            ),
            KeyBinding::new(
                "c a t",
                workspace::SendKeystrokes("🐱".to_string()),
                Some("vim_mode == insert"),
            ),
        ])
    });
    cx.neovim.exec("imap dog 🐶").await;
    cx.neovim.exec("imap cat 🐱").await;

    cx.set_shared_state("ˇ").await;
    cx.simulate_shared_keystrokes("i d o g").await;
    cx.shared_state().await.assert_eq("🐶ˇ");

    cx.set_shared_state("ˇ").await;
    cx.simulate_shared_keystrokes("i d o d o g").await;
    cx.shared_state().await.assert_eq("do🐶ˇ");

    cx.set_shared_state("ˇ").await;
    cx.simulate_shared_keystrokes("i d o c a t").await;
    cx.shared_state().await.assert_eq("do🐱ˇ");
}

#[perf]
#[gpui::test]
async fn test_remap_nested_pineapple(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.update(|_, cx| {
        cx.bind_keys([
            KeyBinding::new(
                "p i n",
                workspace::SendKeystrokes("📌".to_string()),
                Some("vim_mode == insert"),
            ),
            KeyBinding::new(
                "p i n e",
                workspace::SendKeystrokes("🌲".to_string()),
                Some("vim_mode == insert"),
            ),
            KeyBinding::new(
                "p i n e a p p l e",
                workspace::SendKeystrokes("🍍".to_string()),
                Some("vim_mode == insert"),
            ),
        ])
    });
    cx.neovim.exec("imap pin 📌").await;
    cx.neovim.exec("imap pine 🌲").await;
    cx.neovim.exec("imap pineapple 🍍").await;

    cx.set_shared_state("ˇ").await;
    cx.simulate_shared_keystrokes("i p i n").await;
    cx.executor().advance_clock(Duration::from_millis(1000));
    cx.run_until_parked();
    cx.shared_state().await.assert_eq("📌ˇ");

    cx.set_shared_state("ˇ").await;
    cx.simulate_shared_keystrokes("i p i n e").await;
    cx.executor().advance_clock(Duration::from_millis(1000));
    cx.run_until_parked();
    cx.shared_state().await.assert_eq("🌲ˇ");

    cx.set_shared_state("ˇ").await;
    cx.simulate_shared_keystrokes("i p i n e a p p l e").await;
    cx.shared_state().await.assert_eq("🍍ˇ");
}

#[perf]
#[gpui::test]
async fn test_remap_recursion(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "x",
            workspace::SendKeystrokes("\" _ x".to_string()),
            Some("VimControl"),
        )]);
        cx.bind_keys([KeyBinding::new(
            "y",
            workspace::SendKeystrokes("2 x".to_string()),
            Some("VimControl"),
        )])
    });
    cx.neovim.exec("noremap x \"_x").await;
    cx.neovim.exec("map y 2x").await;

    cx.set_shared_state("ˇhello").await;
    cx.simulate_shared_keystrokes("d l").await;
    cx.shared_clipboard().await.assert_eq("h");
    cx.simulate_shared_keystrokes("y").await;
    cx.shared_clipboard().await.assert_eq("h");
    cx.shared_state().await.assert_eq("ˇlo");
}

#[perf]
#[gpui::test]
async fn test_escape_while_waiting(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state("ˇhi").await;
    cx.simulate_shared_keystrokes("\" + escape x").await;
    cx.shared_state().await.assert_eq("ˇi");
}

#[perf]
#[gpui::test]
async fn test_ctrl_w_override(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new("ctrl-w", DeleteLine, None)]);
    });
    cx.neovim.exec("map <c-w> D").await;
    cx.set_shared_state("ˇhi").await;
    cx.simulate_shared_keystrokes("ctrl-w").await;
    cx.shared_state().await.assert_eq("ˇ");
}

#[perf]
#[gpui::test]
async fn test_visual_indent_count(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state("ˇhi", Mode::Normal);
    cx.simulate_keystrokes("shift-v 3 >");
    cx.assert_state("            ˇhi", Mode::Normal);
    cx.simulate_keystrokes("shift-v 2 <");
    cx.assert_state("    ˇhi", Mode::Normal);
}

#[perf]
#[gpui::test]
async fn test_record_replay_recursion(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇhello world").await;
    cx.simulate_shared_keystrokes(">").await;
    cx.simulate_shared_keystrokes(".").await;
    cx.simulate_shared_keystrokes(".").await;
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq("ˇhello world");
}

#[perf]
#[gpui::test]
async fn test_blackhole_register(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇhello world").await;
    cx.simulate_shared_keystrokes("d i w \" _ d a w").await;
    cx.simulate_shared_keystrokes("p").await;
    cx.shared_state().await.assert_eq("hellˇo");
}

#[perf]
#[gpui::test]
async fn test_sentence_backwards(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("one\n\ntwo\nthree\nˇ\nfour").await;
    cx.simulate_shared_keystrokes("(").await;
    cx.shared_state()
        .await
        .assert_eq("one\n\nˇtwo\nthree\n\nfour");

    cx.set_shared_state("hello.\n\n\nworˇld.").await;
    cx.simulate_shared_keystrokes("(").await;
    cx.shared_state().await.assert_eq("hello.\n\n\nˇworld.");
    cx.simulate_shared_keystrokes("(").await;
    cx.shared_state().await.assert_eq("hello.\n\nˇ\nworld.");
    cx.simulate_shared_keystrokes("(").await;
    cx.shared_state().await.assert_eq("ˇhello.\n\n\nworld.");

    cx.set_shared_state("hello. worlˇd.").await;
    cx.simulate_shared_keystrokes("(").await;
    cx.shared_state().await.assert_eq("hello. ˇworld.");
    cx.simulate_shared_keystrokes("(").await;
    cx.shared_state().await.assert_eq("ˇhello. world.");

    cx.set_shared_state(". helˇlo.").await;
    cx.simulate_shared_keystrokes("(").await;
    cx.shared_state().await.assert_eq(". ˇhello.");
    cx.simulate_shared_keystrokes("(").await;
    cx.shared_state().await.assert_eq(". ˇhello.");

    cx.set_shared_state(indoc! {
        "{
            hello_world();
        ˇ}"
    })
    .await;
    cx.simulate_shared_keystrokes("(").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇ{
            hello_world();
        }"
    });

    cx.set_shared_state(indoc! {
        "Hello! World..?

        \tHello! World... ˇ"
    })
    .await;
    cx.simulate_shared_keystrokes("(").await;
    cx.shared_state().await.assert_eq(indoc! {
        "Hello! World..?

        \tHello! ˇWorld... "
    });
    cx.simulate_shared_keystrokes("(").await;
    cx.shared_state().await.assert_eq(indoc! {
        "Hello! World..?

        \tˇHello! World... "
    });
    cx.simulate_shared_keystrokes("(").await;
    cx.shared_state().await.assert_eq(indoc! {
        "Hello! World..?
        ˇ
        \tHello! World... "
    });
    cx.simulate_shared_keystrokes("(").await;
    cx.shared_state().await.assert_eq(indoc! {
        "Hello! ˇWorld..?

        \tHello! World... "
    });
}

#[perf]
#[gpui::test]
async fn test_sentence_forwards(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("helˇlo.\n\n\nworld.").await;
    cx.simulate_shared_keystrokes(")").await;
    cx.shared_state().await.assert_eq("hello.\nˇ\n\nworld.");
    cx.simulate_shared_keystrokes(")").await;
    cx.shared_state().await.assert_eq("hello.\n\n\nˇworld.");
    cx.simulate_shared_keystrokes(")").await;
    cx.shared_state().await.assert_eq("hello.\n\n\nworldˇ.");

    cx.set_shared_state("helˇlo.\n\n\nworld.").await;
}

#[perf]
#[gpui::test]
async fn test_ctrl_o_visual(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("helloˇ world.").await;
    cx.simulate_shared_keystrokes("i ctrl-o v b r l").await;
    cx.shared_state().await.assert_eq("ˇllllllworld.");
    cx.simulate_shared_keystrokes("ctrl-o v f w d").await;
    cx.shared_state().await.assert_eq("ˇorld.");
}

#[perf]
#[gpui::test]
async fn test_ctrl_o_position(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("helˇlo world.").await;
    cx.simulate_shared_keystrokes("i ctrl-o d i w").await;
    cx.shared_state().await.assert_eq("ˇ world.");
    cx.simulate_shared_keystrokes("ctrl-o p").await;
    cx.shared_state().await.assert_eq(" helloˇworld.");
}

#[perf]
#[gpui::test]
async fn test_ctrl_o_dot(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("heˇllo world.").await;
    cx.simulate_shared_keystrokes("x i ctrl-o .").await;
    cx.shared_state().await.assert_eq("heˇo world.");
    cx.simulate_shared_keystrokes("l l escape .").await;
    cx.shared_state().await.assert_eq("hellˇllo world.");
}

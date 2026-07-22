use super::*;

#[perf]
#[gpui::test]
async fn test_buffer_search(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
            The quick brown
            fox juˇmps over
            the lazy dog"},
        Mode::Normal,
    );
    cx.simulate_keystrokes("/");

    let search_bar = cx.workspace(|workspace, _, cx| {
        workspace
            .active_pane()
            .read(cx)
            .toolbar()
            .read(cx)
            .item_of_type::<BufferSearchBar>()
            .expect("Buffer search bar should be deployed")
    });

    cx.update_entity(search_bar, |bar, _, cx| {
        assert_eq!(bar.query(cx), "");
    })
}

#[perf]
#[gpui::test]
async fn test_count_down(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(indoc! {"aˇa\nbb\ncc\ndd\nee"}, Mode::Normal);
    cx.simulate_keystrokes("2 down");
    cx.assert_editor_state("aa\nbb\ncˇc\ndd\nee");
    cx.simulate_keystrokes("9 down");
    cx.assert_editor_state("aa\nbb\ncc\ndd\neˇe");
}

#[perf]
#[gpui::test]
async fn test_end_of_document_710(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // goes to end by default
    cx.set_state(indoc! {"aˇa\nbb\ncc"}, Mode::Normal);
    cx.simulate_keystrokes("shift-g");
    cx.assert_editor_state("aa\nbb\ncˇc");

    // can go to line 1 (https://github.com/mav-industries/mav/issues/5812)
    cx.simulate_keystrokes("1 shift-g");
    cx.assert_editor_state("aˇa\nbb\ncc");
}

#[perf]
#[gpui::test]
async fn test_end_of_line_with_times(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // goes to current line end
    cx.set_state(indoc! {"ˇaa\nbb\ncc"}, Mode::Normal);
    cx.simulate_keystrokes("$");
    cx.assert_editor_state("aˇa\nbb\ncc");

    // goes to next line end
    cx.simulate_keystrokes("2 $");
    cx.assert_editor_state("aa\nbˇb\ncc");

    // try to exceed the final line.
    cx.simulate_keystrokes("4 $");
    cx.assert_editor_state("aa\nbb\ncˇc");
}

#[perf]
#[gpui::test]
async fn test_indent_outdent(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // works in normal mode
    cx.set_state(indoc! {"aa\nbˇb\ncc"}, Mode::Normal);
    cx.simulate_keystrokes("> >");
    cx.assert_editor_state("aa\n    bˇb\ncc");
    cx.simulate_keystrokes("< <");
    cx.assert_editor_state("aa\nbˇb\ncc");

    // works in visual mode
    cx.simulate_keystrokes("shift-v down >");
    cx.assert_editor_state("aa\n    bˇb\n    cc");

    // works as operator
    cx.set_state("aa\nbˇb\ncc\n", Mode::Normal);
    cx.simulate_keystrokes("> j");
    cx.assert_editor_state("aa\n    bˇb\n    cc\n");
    cx.simulate_keystrokes("< k");
    cx.assert_editor_state("aa\nbˇb\n    cc\n");
    cx.simulate_keystrokes("> i p");
    cx.assert_editor_state("    aa\n    bˇb\n        cc\n");
    cx.simulate_keystrokes("< i p");
    cx.assert_editor_state("aa\nbˇb\n    cc\n");
    cx.simulate_keystrokes("< i p");
    cx.assert_editor_state("aa\nbˇb\ncc\n");

    cx.set_state("ˇaa\nbb\ncc\n", Mode::Normal);
    cx.simulate_keystrokes("> 2 j");
    cx.assert_editor_state("    ˇaa\n    bb\n    cc\n");

    cx.set_state("aa\nbb\nˇcc\n", Mode::Normal);
    cx.simulate_keystrokes("> 2 k");
    cx.assert_editor_state("    aa\n    bb\n    ˇcc\n");

    // works with repeat
    cx.set_state("a\nb\nccˇc\n", Mode::Normal);
    cx.simulate_keystrokes("> 2 k");
    cx.assert_editor_state("    a\n    b\n    ccˇc\n");
    cx.simulate_keystrokes(".");
    cx.assert_editor_state("        a\n        b\n        ccˇc\n");
    cx.simulate_keystrokes("v k <");
    cx.assert_editor_state("        a\n    bˇ\n    ccc\n");
    cx.simulate_keystrokes(".");
    cx.assert_editor_state("        a\nbˇ\nccc\n");
}

#[perf]
#[gpui::test]
async fn test_escape_command_palette(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("aˇbc\n", Mode::Normal);
    cx.simulate_keystrokes("i cmd-shift-p");

    assert!(
        cx.workspace(|workspace, _, cx| workspace.active_modal::<CommandPalette>(cx).is_some())
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        !cx.workspace(|workspace, _, cx| workspace.active_modal::<CommandPalette>(cx).is_some())
    );
    cx.assert_state("aˇbc\n", Mode::Insert);
}

#[perf]
#[gpui::test]
async fn test_escape_cancels(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("aˇbˇc", Mode::Normal);
    cx.simulate_keystrokes("escape");

    cx.assert_state("aˇbc", Mode::Normal);
}

#[perf]
#[gpui::test]
async fn test_selection_on_search(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(indoc! {"aa\nbˇb\ncc\ncc\ncc\n"}, Mode::Normal);
    cx.simulate_keystrokes("/ c c");

    let search_bar = cx.workspace(|workspace, _, cx| {
        workspace
            .active_pane()
            .read(cx)
            .toolbar()
            .read(cx)
            .item_of_type::<BufferSearchBar>()
            .expect("Buffer search bar should be deployed")
    });

    cx.update_entity(search_bar, |bar, _, cx| {
        assert_eq!(bar.query(cx), "cc");
    });

    cx.update_editor(|editor, window, cx| {
        let highlights = editor.all_text_background_highlights(window, cx);
        assert_eq!(3, highlights.len());
        assert_eq!(
            DisplayPoint::new(DisplayRow(2), 0)..DisplayPoint::new(DisplayRow(2), 2),
            highlights[0].0
        )
    });
    cx.simulate_keystrokes("enter");

    cx.assert_state(indoc! {"aa\nbb\nˇcc\ncc\ncc\n"}, Mode::Normal);
    cx.simulate_keystrokes("n");
    cx.assert_state(indoc! {"aa\nbb\ncc\nˇcc\ncc\n"}, Mode::Normal);
    cx.simulate_keystrokes("shift-n");
    cx.assert_state(indoc! {"aa\nbb\nˇcc\ncc\ncc\n"}, Mode::Normal);
}

#[perf]
#[gpui::test]
async fn test_word_characters(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_typescript(cx).await;
    cx.set_state(
        indoc! { "
        class A {
            #ˇgoop = 99;
            $ˇgoop () { return this.#gˇoop };
        };
        console.log(new A().$gooˇp())
    "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i w");
    cx.assert_state(
        indoc! {"
        class A {
            «#goopˇ» = 99;
            «$goopˇ» () { return this.«#goopˇ» };
        };
        console.log(new A().«$goopˇ»())
    "},
        Mode::Visual,
    )
}

#[perf]
#[gpui::test]
async fn test_kebab_case(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_html(cx).await;
    cx.set_state(
        indoc! { r#"
            <div><a class="bg-rˇed"></a></div>
            "#},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i w");
    cx.assert_state(
        indoc! { r#"
        <div><a class="bg-«redˇ»"></a></div>
        "#
        },
        Mode::Visual,
    )
}

#[perf]
#[gpui::test]
async fn test_join_lines(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
      ˇone
      two
      three
      four
      five
      six
      "})
        .await;
    cx.simulate_shared_keystrokes("shift-j").await;
    cx.shared_state().await.assert_eq(indoc! {"
          oneˇ two
          three
          four
          five
          six
          "});
    cx.simulate_shared_keystrokes("3 shift-j").await;
    cx.shared_state().await.assert_eq(indoc! {"
          one two threeˇ four
          five
          six
          "});

    cx.set_shared_state(indoc! {"
      ˇone
      two
      three
      four
      five
      six
      "})
        .await;
    cx.simulate_shared_keystrokes("j v 3 j shift-j").await;
    cx.shared_state().await.assert_eq(indoc! {"
      one
      two three fourˇ five
      six
      "});

    cx.set_shared_state(indoc! {"
      ˇone
      two
      three
      four
      five
      six
      "})
        .await;
    cx.simulate_shared_keystrokes("g shift-j").await;
    cx.shared_state().await.assert_eq(indoc! {"
          oneˇtwo
          three
          four
          five
          six
          "});
    cx.simulate_shared_keystrokes("3 g shift-j").await;
    cx.shared_state().await.assert_eq(indoc! {"
          onetwothreeˇfour
          five
          six
          "});

    cx.set_shared_state(indoc! {"
      ˇone
      two
      three
      four
      five
      six
      "})
        .await;
    cx.simulate_shared_keystrokes("j v 3 j g shift-j").await;
    cx.shared_state().await.assert_eq(indoc! {"
      one
      twothreefourˇfive
      six
      "});
}

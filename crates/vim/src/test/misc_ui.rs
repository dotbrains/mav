use super::*;

#[perf]
#[gpui::test]
async fn test_multi_cursor_replay(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state(
        indoc! {
            "
        oˇne one one

        two two two
        "
        },
        Mode::Normal,
    );

    cx.simulate_keystrokes("3 g l s wow escape escape");
    cx.assert_state(
        indoc! {
            "
        woˇw wow wow

        two two two
        "
        },
        Mode::Normal,
    );

    cx.simulate_keystrokes("2 j 3 g l .");
    cx.assert_state(
        indoc! {
            "
        wow wow wow

        woˇw woˇw woˇw
        "
        },
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_clipping_on_mode_change(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {
        "
        ˇverylongline
        andsomelinebelow
        "
        },
        Mode::Normal,
    );

    cx.simulate_keystrokes("v e");
    cx.assert_state(
        indoc! {
        "
        «verylonglineˇ»
        andsomelinebelow
        "
        },
        Mode::Visual,
    );

    let mut pixel_position = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let current_head = editor
            .selections
            .newest_display(&snapshot.display_snapshot)
            .end;
        editor.last_bounds().unwrap().origin
            + editor
                .display_to_pixel_point(current_head, &snapshot, window, cx)
                .unwrap()
    });
    pixel_position.x += px(100.);
    // click beyond end of the line
    cx.simulate_click(pixel_position, Modifiers::default());
    cx.run_until_parked();

    cx.assert_state(
        indoc! {
        "
        verylonglinˇe
        andsomelinebelow
        "
        },
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_wrap_selections_in_tag_line_mode(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    let js_language = Arc::new(Language::new(
        LanguageConfig {
            name: "JavaScript".into(),
            wrap_characters: Some(language::WrapCharactersConfig {
                start_prefix: "<".into(),
                start_suffix: ">".into(),
                end_prefix: "</".into(),
                end_suffix: ">".into(),
            }),
            ..LanguageConfig::default()
        },
        None,
    ));

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(js_language), cx));

    cx.set_state(
        indoc! {
        "
        ˇaaaaa
        bbbbb
        "
        },
        Mode::Normal,
    );

    cx.simulate_keystrokes("shift-v j");
    cx.dispatch_action(WrapSelectionsInTag);

    cx.assert_state(
        indoc! {
            "
            <ˇ>aaaaa
            bbbbb</ˇ>
            "
        },
        Mode::VisualLine,
    );
}

#[gpui::test]
async fn test_repeat_grouping_41735(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // typically transaction gropuing is disabled in tests, but here we need to test it.
    cx.update_buffer(|buffer, _cx| buffer.set_group_interval(Duration::from_millis(300)));

    cx.set_shared_state("ˇ").await;

    cx.simulate_shared_keystrokes("i a escape").await;
    cx.simulate_shared_keystrokes(". . .").await;
    cx.shared_state().await.assert_eq("ˇaaaa");
    cx.simulate_shared_keystrokes("u").await;
    cx.shared_state().await.assert_eq("ˇaaa");
}

#[gpui::test]
async fn test_deactivate(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings.editor.cursor_shape = Some(settings::CursorShape::Underline);
        });
    });

    // Assert that, while in `Normal` mode, the cursor shape is `Block` but,
    // after deactivating vim mode, it should revert to the one specified in the
    // user's settings, if set.
    cx.update_editor(|editor, _window, _cx| {
        assert_eq!(editor.cursor_shape(), CursorShape::Block);
    });

    cx.disable_vim();

    cx.update_editor(|editor, _window, _cx| {
        assert_eq!(editor.cursor_shape(), CursorShape::Underline);
    });
}

// workspace::SendKeystrokes should pass literal keystrokes without triggering vim motions.
// When sending `" _ x`, the `_` should select the blackhole register, not trigger
// vim::StartOfLineDownward.
#[gpui::test]
async fn test_send_keystrokes_underscore_is_literal_46509(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Bind a key to send `" _ x` which should:
    // `"` - start register selection
    // `_` - select blackhole register (NOT vim::StartOfLineDownward)
    // `x` - delete character into blackhole register
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "g x",
            workspace::SendKeystrokes("\" _ x".to_string()),
            Some("VimControl"),
        )])
    });

    cx.set_state("helˇlo", Mode::Normal);

    cx.simulate_keystrokes("g x");
    cx.run_until_parked();

    cx.assert_state("helˇo", Mode::Normal);
}

#[gpui::test]
async fn test_send_keystrokes_no_key_equivalent_mapping_46509(cx: &mut gpui::TestAppContext) {
    use collections::HashMap;
    use gpui::{KeybindingKeystroke, Keystroke, PlatformKeyboardMapper};

    // create a mock Danish keyboard mapper
    // on Danish keyboards, the macOS key equivalents mapping includes: '{' -> 'Æ' and '}' -> 'Ø'
    // this means the `{` character is produced by the key labeled `Æ` (with shift modifier)
    struct DanishKeyboardMapper;
    impl PlatformKeyboardMapper for DanishKeyboardMapper {
        fn map_key_equivalent(
            &self,
            mut keystroke: Keystroke,
            use_key_equivalents: bool,
        ) -> KeybindingKeystroke {
            if use_key_equivalents {
                if keystroke.key == "{" {
                    keystroke.key = "Æ".to_string();
                }
                if keystroke.key == "}" {
                    keystroke.key = "Ø".to_string();
                }
            }
            KeybindingKeystroke::from_keystroke(keystroke)
        }

        fn get_key_equivalents(&self) -> Option<&HashMap<char, char>> {
            None
        }
    }

    let mapper = DanishKeyboardMapper;

    let keystroke_brace = Keystroke::parse("{").unwrap();
    let mapped_with_bug = mapper.map_key_equivalent(keystroke_brace.clone(), true);
    assert_eq!(
        mapped_with_bug.key(),
        "Æ",
        "BUG: With use_key_equivalents=true, {{ is mapped to Æ on Danish keyboard"
    );

    // Fixed behavior, where the literal `{` character is preserved
    let mapped_fixed = mapper.map_key_equivalent(keystroke_brace.clone(), false);
    assert_eq!(
        mapped_fixed.key(),
        "{",
        "FIX: With use_key_equivalents=false, {{ stays as {{"
    );

    // Same applies to }
    let keystroke_close = Keystroke::parse("}").unwrap();
    let mapped_close_bug = mapper.map_key_equivalent(keystroke_close.clone(), true);
    assert_eq!(mapped_close_bug.key(), "Ø");
    let mapped_close_fixed = mapper.map_key_equivalent(keystroke_close.clone(), false);
    assert_eq!(mapped_close_fixed.key(), "}");

    let mut cx = VimTestContext::new(cx, true).await;

    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "g p",
            workspace::SendKeystrokes("{".to_string()),
            Some("vim_mode == normal"),
        )])
    });

    cx.set_state(
        indoc! {"
            first paragraph

            second paragraphˇ

            third paragraph
        "},
        Mode::Normal,
    );

    cx.simulate_keystrokes("g p");
    cx.run_until_parked();

    cx.assert_state(
        indoc! {"
            first paragraph
            ˇ
            second paragraph

            third paragraph
        "},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_project_search_opens_in_normal_mode(cx: &mut gpui::TestAppContext) {
    VimTestContext::init(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file_a.rs": "// File A.",
            "file_b.rs": "// File B.",
        }),
    )
    .await;

    let project = project::Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    cx.update(|cx| {
        VimTestContext::init_keybindings(true, cx);
    });

    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

    workspace.update_in(cx, |workspace, window, cx| {
        ProjectSearchView::deploy_search(workspace, &DeploySearch::default(), window, cx)
    });

    let search_view = workspace.update_in(cx, |workspace, _, cx| {
        workspace
            .active_pane()
            .read(cx)
            .items()
            .find_map(|item| item.downcast::<ProjectSearchView>())
            .expect("Project search view should be active")
    });

    project_search::perform_project_search(&search_view, "File A", cx);

    search_view.update(cx, |search_view, cx| {
        let vim_mode = search_view
            .results_editor()
            .read(cx)
            .addon::<VimAddon>()
            .map(|addon| addon.entity.read(cx).mode);

        assert_eq!(vim_mode, Some(Mode::Normal));
    });
}

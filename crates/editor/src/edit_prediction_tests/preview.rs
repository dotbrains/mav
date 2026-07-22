use super::support::*;
use super::*;

#[gpui::test]
async fn test_edit_prediction_refresh_suppressed_while_following(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);
    cx.set_state("let x = ˇ;");

    propose_edits(&provider, vec![(8..8, "42")], &mut cx);

    cx.update_editor(|editor, window, cx| {
        editor.refresh_edit_prediction(
            false,
            false,
            EditPredictionRequestTrigger::Other,
            window,
            cx,
        );
        editor.update_visible_edit_prediction(window, cx);
    });

    assert_eq!(
        provider.read_with(&cx.cx, |provider, _| {
            provider.refresh_count.load(atomic::Ordering::SeqCst)
        }),
        1
    );
    cx.editor(|editor, _, _| {
        assert!(editor.active_edit_prediction.is_some());
    });

    cx.update_editor(|editor, window, cx| {
        editor.leader_id = Some(CollaboratorId::PeerId(PeerId::default()));
        editor.refresh_edit_prediction(
            false,
            false,
            EditPredictionRequestTrigger::Other,
            window,
            cx,
        );
    });

    assert_eq!(
        provider.read_with(&cx.cx, |provider, _| {
            provider.refresh_count.load(atomic::Ordering::SeqCst)
        }),
        1
    );
    cx.editor(|editor, _, _| {
        assert!(editor.active_edit_prediction.is_none());
    });

    cx.update_editor(|editor, window, cx| {
        editor.leader_id = None;
        editor.refresh_edit_prediction(
            false,
            false,
            EditPredictionRequestTrigger::Other,
            window,
            cx,
        );
    });

    assert_eq!(
        provider.read_with(&cx.cx, |provider, _| {
            provider.refresh_count.load(atomic::Ordering::SeqCst)
        }),
        2
    );
}

#[gpui::test]
async fn test_edit_prediction_preview_cleanup_on_toggle_off(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    // Bind `ctrl-shift-a` to accept the provided edit prediction. The actual key
    // binding here doesn't matter, we simply need to confirm that holding the
    // binding's modifiers triggers the edit prediction preview.
    cx.update(|cx| cx.bind_keys([KeyBinding::new("ctrl-shift-a", AcceptEditPrediction, None)]));

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);
    cx.set_state("let x = ˇ;");

    propose_edits(&provider, vec![(8..8, "42")], &mut cx);
    cx.update_editor(|editor, window, cx| {
        editor.set_menu_edit_predictions_policy(MenuEditPredictionsPolicy::ByProvider);
        editor.update_visible_edit_prediction(window, cx)
    });

    cx.editor(|editor, _, _| {
        assert!(editor.has_active_edit_prediction());
    });

    // Simulate pressing the modifiers for `AcceptEditPrediction`, namely
    // `ctrl-shift`, so that we can confirm that the edit prediction preview is
    // activated.
    let modifiers = Modifiers::control_shift();
    cx.simulate_modifiers_change(modifiers);
    cx.run_until_parked();

    cx.editor(|editor, _, _| {
        assert!(editor.edit_prediction_preview_is_active());
    });

    // Disable showing edit predictions without issuing a new modifiers changed
    // event, to confirm that the edit prediction preview is still active.
    cx.update_editor(|editor, window, cx| {
        editor.set_show_edit_predictions(Some(false), window, cx);
    });

    cx.editor(|editor, _, _| {
        assert!(!editor.has_active_edit_prediction());
        assert!(editor.edit_prediction_preview_is_active());
    });

    // Now release the modifiers
    // Simulate releasing all modifiers, ensuring that even with edit prediction
    // disabled, the edit prediction preview is cleaned up.
    cx.simulate_modifiers_change(Modifiers::none());
    cx.run_until_parked();

    cx.editor(|editor, _, _| {
        assert!(!editor.edit_prediction_preview_is_active());
    });
}

#[gpui::test]
async fn test_hidden_edit_prediction_does_not_open_snippet_menu_on_word_input(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = hidden_edit_prediction_snippet_test_context(cx).await;
    cx.simulate_input("t");
    cx.run_until_parked();

    cx.update_editor(|editor, _, _| {
        assert!(editor.has_active_edit_prediction());
        assert!(editor.context_menu.borrow().is_none());
    });
}

#[gpui::test]
async fn test_hidden_edit_prediction_opens_snippet_menu_for_strong_prefix_match(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = hidden_edit_prediction_snippet_test_context(cx).await;
    cx.simulate_input("t");
    cx.run_until_parked();
    cx.simulate_input("h");
    cx.run_until_parked();

    cx.update_editor(|editor, _, _| {
        let Some(CodeContextMenu::Completions(menu)) = &*editor.context_menu.borrow() else {
            panic!("expected completions menu");
        };
        let entries = menu.entries.borrow();
        assert!(
            entries
                .iter()
                .any(|entry| { entry.as_match().is_some_and(|m| m.string == "Theta") })
        );
    });
}

#[gpui::test]
async fn test_edit_prediction_preview_activates_when_prediction_arrives_with_modifier_held(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});
    load_default_keymap(cx);
    update_test_language_settings(cx, &|settings| {
        settings.edit_predictions.get_or_insert_default().mode = Some(EditPredictionsMode::Subtle);
    });

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);
    cx.set_state("let x = ˇ;");

    cx.editor(|editor, _, _| {
        assert!(!editor.has_active_edit_prediction());
        assert!(!editor.edit_prediction_preview_is_active());
    });

    let preview_modifiers = cx.update_editor(|editor, window, cx| {
        *editor
            .preview_edit_prediction_keystroke(window, cx)
            .unwrap()
            .modifiers()
    });

    cx.simulate_modifiers_change(preview_modifiers);
    cx.run_until_parked();

    cx.editor(|editor, _, _| {
        assert!(!editor.has_active_edit_prediction());
        assert!(editor.edit_prediction_preview_is_active());
    });

    propose_edits(&provider, vec![(8..8, "42")], &mut cx);
    cx.update_editor(|editor, window, cx| {
        editor.set_menu_edit_predictions_policy(MenuEditPredictionsPolicy::ByProvider);
        editor.update_visible_edit_prediction(window, cx)
    });

    cx.editor(|editor, _, _| {
        assert!(editor.has_active_edit_prediction());
        assert!(
            editor.edit_prediction_preview_is_active(),
            "prediction preview should activate immediately when the prediction arrives while the preview modifier is still held",
        );
    });
}

#[gpui::test]
async fn test_edit_prediction_preview_does_not_hide_code_actions_on_modifier_press(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});
    update_test_language_settings(cx, &|settings| {
        settings.edit_predictions.get_or_insert_default().mode = Some(EditPredictionsMode::Subtle);
    });
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new(
            "ctrl-enter",
            AcceptEditPrediction,
            Some("Editor && edit_prediction && !showing_completions"),
        )]);
    });

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            code_action_provider: Some(lsp::CodeActionProviderCapability::Simple(true)),
            ..Default::default()
        },
        cx,
    )
    .await;
    cx.set_state(indoc! {"
        fn main() {
            let valueˇ = 1;
        }
    "});

    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    cx.update_editor(|editor, window, cx| {
        editor.set_edit_prediction_provider(Some(provider.clone()), window, cx);
    });

    let snapshot = cx.buffer_snapshot();
    let edit_position = snapshot.anchor_after(Point::new(1, 13));
    cx.update(|_, cx| {
        provider.update(cx, |provider, _| {
            provider.set_edit_prediction(Some(edit_prediction_types::EditPrediction::Local {
                id: None,
                edits: vec![(edit_position..edit_position, " + 1".into())],
                cursor_position: None,
                edit_preview: None,
            }))
        })
    });
    cx.update_editor(|editor, window, cx| {
        editor.set_menu_edit_predictions_policy(MenuEditPredictionsPolicy::ByProvider);
        editor.update_visible_edit_prediction(window, cx);
    });
    cx.update_editor(|editor, _, _| {
        assert!(editor.has_active_edit_prediction());
        assert!(editor.stale_edit_prediction_in_menu.is_none());
    });

    let mut code_action_requests = cx.set_request_handler::<lsp::request::CodeActionRequest, _, _>(
        move |_, _, _| async move {
            Ok(Some(vec![lsp::CodeActionOrCommand::CodeAction(
                lsp::CodeAction {
                    title: "Inline value".to_string(),
                    kind: Some(lsp::CodeActionKind::QUICKFIX),
                    ..Default::default()
                },
            )]))
        },
    );

    cx.update_editor(|editor, window, cx| {
        editor.toggle_code_actions(
            &crate::actions::ToggleCodeActions {
                deployed_from: None,
                quick_launch: false,
            },
            window,
            cx,
        );
    });
    code_action_requests.next().await;
    cx.run_until_parked();
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;

    cx.update_editor(|editor, _, _| {
        assert!(!editor.has_active_edit_prediction());
        assert!(editor.stale_edit_prediction_in_menu.is_some());
        assert!(editor.context_menu_visible());
        assert!(matches!(
            editor.context_menu.borrow().as_ref(),
            Some(crate::code_context_menus::CodeContextMenu::CodeActions(_))
        ));
        assert!(!editor.edit_prediction_preview_is_active());
    });

    cx.simulate_modifiers_change(Modifiers::control());
    cx.run_until_parked();

    cx.update_editor(|editor, _, _| {
        assert!(
            !editor.edit_prediction_preview_is_active(),
            "modifier-only press should not activate edit prediction preview while code actions are open"
        );
        assert!(
            editor.context_menu_visible(),
            "modifier-only press should not hide the code actions menu"
        );
        assert!(matches!(
            editor.context_menu.borrow().as_ref(),
            Some(crate::code_context_menus::CodeContextMenu::CodeActions(_))
        ));
    });
}

#[gpui::test]
async fn test_edit_prediction_preview_supersedes_completions_menu(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    update_test_language_settings(cx, &|settings| {
        settings.edit_predictions.get_or_insert_default().mode = Some(EditPredictionsMode::Subtle);
    });
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new(
            "ctrl-enter",
            AcceptEditPrediction,
            Some("Editor && edit_prediction && showing_completions"),
        )]);
    });

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);
    assign_editor_completion_menu_provider(&mut cx);
    cx.set_state("let x = ˇ;");

    propose_edits(&provider, vec![(8..8, "42")], &mut cx);
    cx.update_editor(|editor, window, cx| {
        editor.set_menu_edit_predictions_policy(MenuEditPredictionsPolicy::ByProvider);
        editor.update_visible_edit_prediction(window, cx);
    });
    cx.update_editor(|editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    cx.run_until_parked();

    cx.editor(|editor, _, _| {
        assert!(editor.has_active_edit_prediction());
        assert!(editor.context_menu_visible());
        assert!(matches!(
            editor.context_menu.borrow().as_ref(),
            Some(crate::code_context_menus::CodeContextMenu::Completions(_))
        ));
        assert!(!editor.edit_prediction_preview_is_active());
    });

    cx.simulate_modifiers_change(Modifiers::control());
    cx.run_until_parked();

    cx.editor(|editor, _, _| {
        assert!(editor.edit_prediction_preview_is_active());
        assert!(!editor.context_menu_visible());
        assert!(matches!(
            editor.context_menu.borrow().as_ref(),
            Some(crate::code_context_menus::CodeContextMenu::Completions(_))
        ));
    });
}

use super::*;

#[gpui::test]
async fn test_hint_setting_changes(cx: &mut gpui::TestAppContext) {
    let allowed_hint_kinds = HashSet::from_iter([None, Some(InlayHintKind::Type)]);
    init_test(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            show_value_hints: Some(true),
            enabled: Some(true),
            edit_debounce_ms: Some(0),
            scroll_debounce_ms: Some(0),
            show_type_hints: Some(allowed_hint_kinds.contains(&Some(InlayHintKind::Type))),
            show_parameter_hints: Some(
                allowed_hint_kinds.contains(&Some(InlayHintKind::Parameter)),
            ),
            show_other_hints: Some(allowed_hint_kinds.contains(&None)),
            show_background: Some(false),
            toggle_on_modifiers_press: None,
        })
    });

    let lsp_request_count = Arc::new(AtomicUsize::new(0));
    let (_, editor, fake_server) = prepare_test_objects(cx, {
        let lsp_request_count = lsp_request_count.clone();
        move |fake_server, file_with_hints| {
            let lsp_request_count = lsp_request_count.clone();
            fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                move |params, _| {
                    lsp_request_count.fetch_add(1, Ordering::Release);
                    async move {
                        assert_eq!(
                            params.text_document.uri,
                            lsp::Uri::from_file_path(file_with_hints).unwrap(),
                        );
                        Ok(Some(vec![
                            lsp::InlayHint {
                                position: lsp::Position::new(0, 1),
                                label: lsp::InlayHintLabel::String("type hint".to_string()),
                                kind: Some(lsp::InlayHintKind::TYPE),
                                text_edits: None,
                                tooltip: None,
                                padding_left: None,
                                padding_right: None,
                                data: None,
                            },
                            lsp::InlayHint {
                                position: lsp::Position::new(0, 2),
                                label: lsp::InlayHintLabel::String("parameter hint".to_string()),
                                kind: Some(lsp::InlayHintKind::PARAMETER),
                                text_edits: None,
                                tooltip: None,
                                padding_left: None,
                                padding_right: None,
                                data: None,
                            },
                            lsp::InlayHint {
                                position: lsp::Position::new(0, 3),
                                label: lsp::InlayHintLabel::String("other hint".to_string()),
                                kind: None,
                                text_edits: None,
                                tooltip: None,
                                padding_left: None,
                                padding_right: None,
                                data: None,
                            },
                        ]))
                    }
                },
            );
        }
    })
    .await;
    cx.executor().run_until_parked();

    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                lsp_request_count.load(Ordering::Relaxed),
                1,
                "Should query new hints once"
            );
            assert_eq!(
                vec![
                    "type hint".to_string(),
                    "parameter hint".to_string(),
                    "other hint".to_string(),
                ],
                cached_hint_labels(editor, cx),
                "Should get its first hints when opening the editor"
            );
            assert_eq!(
                vec!["type hint".to_string(), "other hint".to_string()],
                visible_hint_labels(editor, cx)
            );
            assert_eq!(
                allowed_hint_kinds_for_editor(editor),
                allowed_hint_kinds,
                "Cache should use editor settings to get the allowed hint kinds"
            );
        })
        .unwrap();

    fake_server
        .request::<lsp::request::InlayHintRefreshRequest>((), DEFAULT_LSP_REQUEST_TIMEOUT)
        .await
        .into_response()
        .expect("inlay refresh request failed");
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                lsp_request_count.load(Ordering::Relaxed),
                2,
                "Should load new hints twice"
            );
            assert_eq!(
                vec![
                    "type hint".to_string(),
                    "parameter hint".to_string(),
                    "other hint".to_string(),
                ],
                cached_hint_labels(editor, cx),
                "Cached hints should not change due to allowed hint kinds settings update"
            );
            assert_eq!(
                vec!["type hint".to_string(), "other hint".to_string()],
                visible_hint_labels(editor, cx)
            );
        })
        .unwrap();

    for (new_allowed_hint_kinds, expected_visible_hints) in [
        (HashSet::from_iter([None]), vec!["other hint".to_string()]),
        (
            HashSet::from_iter([Some(InlayHintKind::Type)]),
            vec!["type hint".to_string()],
        ),
        (
            HashSet::from_iter([Some(InlayHintKind::Parameter)]),
            vec!["parameter hint".to_string()],
        ),
        (
            HashSet::from_iter([None, Some(InlayHintKind::Type)]),
            vec!["type hint".to_string(), "other hint".to_string()],
        ),
        (
            HashSet::from_iter([None, Some(InlayHintKind::Parameter)]),
            vec!["parameter hint".to_string(), "other hint".to_string()],
        ),
        (
            HashSet::from_iter([Some(InlayHintKind::Type), Some(InlayHintKind::Parameter)]),
            vec!["type hint".to_string(), "parameter hint".to_string()],
        ),
        (
            HashSet::from_iter([
                None,
                Some(InlayHintKind::Type),
                Some(InlayHintKind::Parameter),
            ]),
            vec![
                "type hint".to_string(),
                "parameter hint".to_string(),
                "other hint".to_string(),
            ],
        ),
    ] {
        update_test_language_settings(cx, &|settings| {
            settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
                show_value_hints: Some(true),
                enabled: Some(true),
                edit_debounce_ms: Some(0),
                scroll_debounce_ms: Some(0),
                show_type_hints: Some(new_allowed_hint_kinds.contains(&Some(InlayHintKind::Type))),
                show_parameter_hints: Some(
                    new_allowed_hint_kinds.contains(&Some(InlayHintKind::Parameter)),
                ),
                show_other_hints: Some(new_allowed_hint_kinds.contains(&None)),
                show_background: Some(false),
                toggle_on_modifiers_press: None,
            })
        });
        cx.executor().run_until_parked();
        editor.update(cx, |editor, _, cx| {
            assert_eq!(
                lsp_request_count.load(Ordering::Relaxed),
                2,
                "Should not load new hints on allowed hint kinds change for hint kinds {new_allowed_hint_kinds:?}"
            );
            assert_eq!(
                vec![
                    "type hint".to_string(),
                    "parameter hint".to_string(),
                    "other hint".to_string(),
                ],
                cached_hint_labels(editor, cx),
                "Should get its cached hints unchanged after the settings change for hint kinds {new_allowed_hint_kinds:?}"
            );
            assert_eq!(
                expected_visible_hints,
                visible_hint_labels(editor, cx),
                "Should get its visible hints filtered after the settings change for hint kinds {new_allowed_hint_kinds:?}"
            );
            assert_eq!(
                allowed_hint_kinds_for_editor(editor),
                new_allowed_hint_kinds,
                "Cache should use editor settings to get the allowed hint kinds for hint kinds {new_allowed_hint_kinds:?}"
            );
        }).unwrap();
    }

    let another_allowed_hint_kinds = HashSet::from_iter([Some(InlayHintKind::Type)]);
    update_test_language_settings(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            show_value_hints: Some(true),
            enabled: Some(false),
            edit_debounce_ms: Some(0),
            scroll_debounce_ms: Some(0),
            show_type_hints: Some(another_allowed_hint_kinds.contains(&Some(InlayHintKind::Type))),
            show_parameter_hints: Some(
                another_allowed_hint_kinds.contains(&Some(InlayHintKind::Parameter)),
            ),
            show_other_hints: Some(another_allowed_hint_kinds.contains(&None)),
            show_background: Some(false),
            toggle_on_modifiers_press: None,
        })
    });
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                lsp_request_count.load(Ordering::Relaxed),
                2,
                "Should not load new hints when hints got disabled"
            );
            assert_eq!(
                vec![
                    "type hint".to_string(),
                    "parameter hint".to_string(),
                    "other hint".to_string(),
                ],
                cached_hint_labels(editor, cx),
                "Should not clear the cache when hints got disabled"
            );
            assert_eq!(
                Vec::<String>::new(),
                visible_hint_labels(editor, cx),
                "Should clear visible hints when hints got disabled"
            );
            assert_eq!(
                allowed_hint_kinds_for_editor(editor),
                another_allowed_hint_kinds,
                "Should update its allowed hint kinds even when hints got disabled"
            );
        })
        .unwrap();

    fake_server
        .request::<lsp::request::InlayHintRefreshRequest>((), DEFAULT_LSP_REQUEST_TIMEOUT)
        .await
        .into_response()
        .expect("inlay refresh request failed");
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            assert_eq!(
                lsp_request_count.load(Ordering::Relaxed),
                2,
                "Should not load new hints when they got disabled"
            );
            assert_eq!(
                vec![
                    "type hint".to_string(),
                    "parameter hint".to_string(),
                    "other hint".to_string(),
                ],
                cached_hint_labels(editor, cx)
            );
            assert_eq!(Vec::<String>::new(), visible_hint_labels(editor, cx));
        })
        .unwrap();

    let final_allowed_hint_kinds = HashSet::from_iter([Some(InlayHintKind::Parameter)]);
    update_test_language_settings(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            show_value_hints: Some(true),
            enabled: Some(true),
            edit_debounce_ms: Some(0),
            scroll_debounce_ms: Some(0),
            show_type_hints: Some(final_allowed_hint_kinds.contains(&Some(InlayHintKind::Type))),
            show_parameter_hints: Some(
                final_allowed_hint_kinds.contains(&Some(InlayHintKind::Parameter)),
            ),
            show_other_hints: Some(final_allowed_hint_kinds.contains(&None)),
            show_background: Some(false),
            toggle_on_modifiers_press: None,
        })
    });
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                lsp_request_count.load(Ordering::Relaxed),
                2,
                "Should not query for new hints when they got re-enabled, as the file version did not change"
            );
            assert_eq!(
                vec![
                    "type hint".to_string(),
                    "parameter hint".to_string(),
                    "other hint".to_string(),
                ],
                cached_hint_labels(editor, cx),
                "Should get its cached hints fully repopulated after the hints got re-enabled"
            );
            assert_eq!(
                vec!["parameter hint".to_string()],
                visible_hint_labels(editor, cx),
                "Should get its visible hints repopulated and filtered after the h"
            );
            assert_eq!(
                allowed_hint_kinds_for_editor(editor),
                final_allowed_hint_kinds,
                "Cache should update editor settings when hints got re-enabled"
            );
        })
        .unwrap();

    fake_server
        .request::<lsp::request::InlayHintRefreshRequest>((), DEFAULT_LSP_REQUEST_TIMEOUT)
        .await
        .into_response()
        .expect("inlay refresh request failed");
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                lsp_request_count.load(Ordering::Relaxed),
                3,
                "Should query for new hints again"
            );
            assert_eq!(
                vec![
                    "type hint".to_string(),
                    "parameter hint".to_string(),
                    "other hint".to_string(),
                ],
                cached_hint_labels(editor, cx),
            );
            assert_eq!(
                vec!["parameter hint".to_string()],
                visible_hint_labels(editor, cx),
            );
        })
        .unwrap();
}

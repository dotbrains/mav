use super::support::*;
use super::*;

#[gpui::test]
async fn test_edit_prediction_insert(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);
    cx.set_state("let absolute_zero_celsius = ˇ;");

    propose_edits(&provider, vec![(28..28, "-273.15")], &mut cx);
    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));

    assert_editor_active_edit_completion(&mut cx, |_, edits| {
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].1.as_ref(), "-273.15");
    });

    accept_completion(&mut cx);

    cx.assert_editor_state("let absolute_zero_celsius = -273.15ˇ;")
}

#[gpui::test]
async fn test_edit_prediction_cursor_position_inside_insertion(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {
        eprintln!("");
    });

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());

    assign_editor_completion_provider(provider.clone(), &mut cx);
    // Buffer: "fn foo() {}" - we'll insert text and position cursor inside the insertion
    cx.set_state("fn foo() ˇ{}");

    // Insert "bar()" at offset 9, with cursor at offset 2 within the insertion (after "ba")
    // This tests the case where cursor is inside newly inserted text
    propose_edits_with_cursor_position_in_insertion(
        &provider,
        vec![(9..9, "bar()")],
        9, // anchor at the insertion point
        2, // offset 2 within "bar()" puts cursor after "ba"
        &mut cx,
    );
    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));

    assert_editor_active_edit_completion(&mut cx, |_, edits| {
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].1.as_ref(), "bar()");
    });

    accept_completion(&mut cx);

    // Cursor should be inside the inserted text at "baˇr()"
    cx.assert_editor_state("fn foo() baˇr(){}");
}

#[gpui::test]
async fn test_edit_prediction_cursor_position_outside_edit(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);
    // Buffer: "let x = ;" with cursor before semicolon - we'll insert "42" and position cursor elsewhere
    cx.set_state("let x = ˇ;");

    // Insert "42" at offset 8, but set cursor_position to offset 4 (the 'x')
    // This tests that cursor moves to the predicted position, not the end of the edit
    propose_edits_with_cursor_position(
        &provider,
        vec![(8..8, "42")],
        Some(4), // cursor at offset 4 (the 'x'), NOT at the edit location
        &mut cx,
    );
    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));

    assert_editor_active_edit_completion(&mut cx, |_, edits| {
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].1.as_ref(), "42");
    });

    accept_completion(&mut cx);

    // Cursor should be at offset 4 (the 'x'), not at the end of the inserted "42"
    cx.assert_editor_state("let ˇx = 42;");
}

#[gpui::test]
async fn test_edit_prediction_cursor_position_fallback(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);
    cx.set_state("let x = ˇ;");

    // Propose an edit without a cursor position - should fall back to end of edit
    propose_edits(&provider, vec![(8..8, "42")], &mut cx);
    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));

    accept_completion(&mut cx);

    // Cursor should be at the end of the inserted text (default behavior)
    cx.assert_editor_state("let x = 42ˇ;")
}

#[gpui::test]
async fn test_edit_prediction_modification(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);
    cx.set_state("let pi = ˇ\"foo\";");

    propose_edits(&provider, vec![(9..14, "3.14159")], &mut cx);
    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));

    assert_editor_active_edit_completion(&mut cx, |_, edits| {
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].1.as_ref(), "3.14159");
    });

    accept_completion(&mut cx);

    cx.assert_editor_state("let pi = 3.14159ˇ;")
}

#[gpui::test]
async fn test_edit_prediction_jump_button(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);

    // Cursor is 2+ lines above the proposed edit
    cx.set_state(indoc! {"
        line 0
        line ˇ1
        line 2
        line 3
        line
    "});

    propose_edits(
        &provider,
        vec![(Point::new(4, 3)..Point::new(4, 3), " 4")],
        &mut cx,
    );

    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));
    assert_editor_active_move_completion(&mut cx, |snapshot, move_target| {
        assert_eq!(move_target.to_point(&snapshot), Point::new(4, 3));
    });

    // When accepting, cursor is moved to the proposed location
    accept_completion(&mut cx);
    cx.assert_editor_state(indoc! {"
        line 0
        line 1
        line 2
        line 3
        linˇe
    "});

    // Cursor is 2+ lines below the proposed edit
    cx.set_state(indoc! {"
        line 0
        line
        line 2
        line 3
        line ˇ4
    "});

    propose_edits(
        &provider,
        vec![(Point::new(1, 3)..Point::new(1, 3), " 1")],
        &mut cx,
    );

    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));
    assert_editor_active_move_completion(&mut cx, |snapshot, move_target| {
        assert_eq!(move_target.to_point(&snapshot), Point::new(1, 3));
    });

    // When accepting, cursor is moved to the proposed location
    accept_completion(&mut cx);
    cx.assert_editor_state(indoc! {"
        line 0
        linˇe
        line 2
        line 3
        line 4
    "});
}

#[gpui::test]
async fn test_edit_prediction_invalidation_range(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);

    // Cursor is 3+ lines above the proposed edit
    cx.set_state(indoc! {"
        line 0
        line ˇ1
        line 2
        line 3
        line 4
        line
    "});
    let edit_location = Point::new(5, 3);

    propose_edits(
        &provider,
        vec![(edit_location..edit_location, " 5")],
        &mut cx,
    );

    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));
    assert_editor_active_move_completion(&mut cx, |snapshot, move_target| {
        assert_eq!(move_target.to_point(&snapshot), edit_location);
    });

    // If we move *towards* the completion, it stays active
    cx.set_selections_state(indoc! {"
        line 0
        line 1
        line ˇ2
        line 3
        line 4
        line
    "});
    assert_editor_active_move_completion(&mut cx, |snapshot, move_target| {
        assert_eq!(move_target.to_point(&snapshot), edit_location);
    });

    // If we move *away* from the completion, it is discarded
    cx.set_selections_state(indoc! {"
        line ˇ0
        line 1
        line 2
        line 3
        line 4
        line
    "});
    cx.editor(|editor, _, _| {
        assert!(editor.active_edit_prediction.is_none());
    });

    // Cursor is 3+ lines below the proposed edit
    cx.set_state(indoc! {"
        line
        line 1
        line 2
        line 3
        line ˇ4
        line 5
    "});
    let edit_location = Point::new(0, 3);

    propose_edits(
        &provider,
        vec![(edit_location..edit_location, " 0")],
        &mut cx,
    );

    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));
    assert_editor_active_move_completion(&mut cx, |snapshot, move_target| {
        assert_eq!(move_target.to_point(&snapshot), edit_location);
    });

    // If we move *towards* the completion, it stays active
    cx.set_selections_state(indoc! {"
        line
        line 1
        line 2
        line ˇ3
        line 4
        line 5
    "});
    assert_editor_active_move_completion(&mut cx, |snapshot, move_target| {
        assert_eq!(move_target.to_point(&snapshot), edit_location);
    });

    // If we move *away* from the completion, it is discarded
    cx.set_selections_state(indoc! {"
        line
        line 1
        line 2
        line 3
        line 4
        line ˇ5
    "});
    cx.editor(|editor, _, _| {
        assert!(editor.active_edit_prediction.is_none());
    });
}

#[gpui::test]
async fn test_edit_prediction_jump_disabled_for_non_mav_providers(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeNonMavEditPredictionDelegate::default());
    assign_editor_completion_provider_non_mav(provider.clone(), &mut cx);

    // Cursor is 2+ lines above the proposed edit
    cx.set_state(indoc! {"
        line 0
        line ˇ1
        line 2
        line 3
        line
    "});

    propose_edits_non_mav(
        &provider,
        vec![(Point::new(4, 3)..Point::new(4, 3), " 4")],
        &mut cx,
    );

    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));

    // For non-Mav providers, there should be no move completion (jump functionality disabled)
    cx.editor(|editor, _, _| {
        if let Some(completion_state) = &editor.active_edit_prediction {
            // Should be an Edit prediction, not a Move prediction
            match &completion_state.completion {
                EditPrediction::Edit { .. } => {
                    // This is expected for non-Mav providers
                }
                EditPrediction::MoveWithin { .. } | EditPrediction::MoveOutside { .. } => {
                    panic!(
                        "Non-Mav providers should not show Move predictions (jump functionality)"
                    );
                }
            }
        }
    });
}

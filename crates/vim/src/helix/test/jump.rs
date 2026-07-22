use super::*;

fn active_helix_jump_labels(cx: &mut VimTestContext) -> Vec<(String, String)> {
    cx.update_editor(|editor, window, cx| {
        let labels = match editor
            .addon::<VimAddon>()
            .unwrap()
            .entity
            .read(cx)
            .operator_stack
            .last()
            .cloned()
        {
            Some(Operator::HelixJump { labels, .. }) => labels,
            other => panic!("expected active HelixJump operator, got {other:?}"),
        };

        let snapshot = editor.snapshot(window, cx);
        let buffer_snapshot = snapshot.display_snapshot.buffer_snapshot();

        labels
            .into_iter()
            .map(|label| {
                let jump_label = label.label.iter().collect::<String>();
                let word = buffer_snapshot
                    .text_for_range(label.range)
                    .collect::<String>();
                (jump_label, word)
            })
            .collect()
    })
}

fn helix_jump_label_for_word(cx: &mut VimTestContext, target_word: &str) -> String {
    active_helix_jump_labels(cx)
        .into_iter()
        .find_map(|(label, word)| (word == target_word).then_some(label))
        .unwrap_or_else(|| {
            let mut message = String::new();
            let labels = active_helix_jump_labels(cx);
            let _ = write!(
                &mut message,
                "expected jump label for word {target_word:?}, available labels: {labels:?}"
            );
            panic!("{message}");
        })
}

fn jump_to_word(cx: &mut VimTestContext, target_word: &str) {
    jump_to_word_with_keystrokes(cx, "g w", target_word);
}

fn jump_to_word_with_keystrokes(cx: &mut VimTestContext, keystrokes: &str, target_word: &str) {
    cx.simulate_keystrokes(keystrokes);

    let label = helix_jump_label_for_word(cx, target_word);

    let mut chars = label.chars();
    let first = chars.next().expect("jump labels are two characters long");
    let second = chars.next().expect("jump labels are two characters long");
    cx.simulate_keystrokes(&format!("{first} {second}"));
}

fn bind_vim_jump_to_word(cx: &mut VimTestContext, keystrokes: &'static str) {
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            keystrokes,
            HelixJumpToWord,
            Some("vim_mode == normal || vim_mode == visual"),
        )])
    });
}

fn active_helix_jump_overlay_counts(cx: &mut VimTestContext) -> (usize, usize) {
    let covered_text_range_count = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        snapshot
            .text_highlight_ranges(HighlightKey::NavigationOverlay(HELIX_JUMP_OVERLAY_KEY))
            .map(|ranges| ranges.as_ref().clone().1.len())
            .unwrap_or_default()
    });
    let label_count = match cx.active_operator() {
        Some(Operator::HelixJump { labels, .. }) => labels.len(),
        _ => 0,
    };

    (covered_text_range_count, label_count)
}

fn assert_helix_jump_cleared(cx: &mut VimTestContext, expected_overlay_counts: (usize, usize)) {
    assert_eq!(cx.active_operator(), None);
    assert_eq!(
        active_helix_jump_overlay_counts(cx),
        expected_overlay_counts,
        "expected Helix jump UI to be fully cleared"
    );
}

fn helix_jump_labels_for_full_buffer(cx: &mut VimTestContext) -> Vec<(String, String)> {
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let display_snapshot = &snapshot.display_snapshot;
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let selections = editor.selections.all::<Point>(display_snapshot);
        let skip_data = Vim::selection_skip_offsets(buffer_snapshot, &selections, false);
        let cursor_offset = selections
            .first()
            .map(|selection| buffer_snapshot.point_to_offset(selection.head()))
            .unwrap_or(MultiBufferOffset(0));
        let style = editor.style(cx);
        let font = style.text.font();
        let font_size = style.text.font_size.to_pixels(window.rem_size());
        let label_color = cx.theme().colors().vim_helix_jump_label_foreground;
        let data = Vim::build_helix_jump_ui_data(
            buffer_snapshot,
            MultiBufferOffset(0),
            buffer_snapshot.len(),
            cursor_offset,
            label_color,
            &skip_data,
            window.text_system(),
            font,
            font_size,
        );

        data.labels
            .into_iter()
            .map(|label| {
                let jump_label = label.label.iter().collect::<String>();
                let word = buffer_snapshot
                    .text_for_range(label.range)
                    .collect::<String>();
                (jump_label, word)
            })
            .collect()
    })
}

#[gpui::test]
async fn test_helix_jump_starts_operator(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("ˇhello world\njump labels", Mode::HelixNormal);

    cx.simulate_keystrokes("g w");

    assert!(
        matches!(cx.active_operator(), Some(Operator::HelixJump { .. })),
        "expected HelixJump operator to be active"
    )
}

#[gpui::test]
async fn test_helix_jump_cancels_on_escape(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("ˇhello world\njump labels", Mode::HelixNormal);
    let overlay_counts = active_helix_jump_overlay_counts(&mut cx);

    cx.simulate_keystrokes("g w");
    cx.simulate_keystrokes("escape");

    cx.assert_state("ˇhello world\njump labels", Mode::HelixNormal);
    assert_helix_jump_cleared(&mut cx, overlay_counts);
}

#[gpui::test]
async fn test_helix_jump_cancels_on_invalid_first_char(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("ˇalpha beta gamma", Mode::HelixNormal);
    let overlay_counts = active_helix_jump_overlay_counts(&mut cx);

    cx.simulate_keystrokes("g w");
    cx.simulate_keystrokes("z");

    cx.assert_state("ˇalpha beta gamma", Mode::HelixNormal);
    assert_helix_jump_cleared(&mut cx, overlay_counts);
}

#[gpui::test]
async fn test_helix_jump_cancels_on_invalid_second_char(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("ˇalpha beta gamma", Mode::HelixNormal);
    let overlay_counts = active_helix_jump_overlay_counts(&mut cx);

    cx.simulate_keystrokes("g w");
    cx.simulate_keystrokes("a z");

    cx.assert_state("ˇalpha beta gamma", Mode::HelixNormal);
    assert_helix_jump_cleared(&mut cx, overlay_counts);
}

#[gpui::test]
async fn test_helix_jump_keeps_full_overlay_after_first_key(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    let text = format!(
        "ˇ{}",
        (0..28)
            .map(|index| format!("w{index:02}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    cx.set_state(&text, Mode::HelixNormal);

    cx.simulate_keystrokes("g w");
    let labels = active_helix_jump_labels(&mut cx);
    let initial_overlay_counts = active_helix_jump_overlay_counts(&mut cx);
    let first_group = labels
        .first()
        .and_then(|(label, _)| label.chars().next())
        .expect("expected at least one helix jump label");
    let next_group = labels
        .iter()
        .filter_map(|(label, _)| label.chars().next())
        .find(|ch| *ch != first_group)
        .expect("expected labels spanning more than one first-character group");

    cx.simulate_keystrokes(&next_group.to_string());

    assert_eq!(
        active_helix_jump_overlay_counts(&mut cx),
        initial_overlay_counts
    );
    assert!(
        matches!(
            cx.active_operator(),
            Some(Operator::HelixJump {
                first_char: Some(ch),
                ..
            }) if ch == next_group
        ),
        "expected HelixJump operator to keep the first typed label character"
    );
}

#[gpui::test]
async fn test_helix_jump_includes_word_before_cursor_boundary(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("oneˇ two three", Mode::HelixNormal);

    jump_to_word(&mut cx, "one");

    cx.assert_state("«oneˇ» two three", Mode::HelixNormal);
    assert_eq!(cx.active_operator(), None);
}

#[gpui::test]
async fn test_helix_jump_skips_single_char_words(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("ˇa bb c dd e", Mode::HelixNormal);

    let words = helix_jump_labels_for_full_buffer(&mut cx)
        .into_iter()
        .map(|(_, word)| word)
        .collect::<Vec<_>>();

    assert_eq!(words, vec!["bb".to_string(), "dd".to_string()]);
}

#[gpui::test]
async fn test_helix_jump_handles_underscored_words(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("baz quxˇ foo_bar _private", Mode::HelixNormal);

    let words = helix_jump_labels_for_full_buffer(&mut cx)
        .into_iter()
        .map(|(_, word)| word)
        .collect::<Vec<_>>();

    assert!(words.iter().any(|word| word == "foo_bar"));
    assert!(words.iter().any(|word| word == "_private"));
    assert!(!words.iter().any(|word| word == "foo"));
    assert!(!words.iter().any(|word| word == "bar"));
}

#[gpui::test]
async fn test_helix_jump_at_end_of_buffer(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("alpha beta gammaˇ", Mode::HelixNormal);

    jump_to_word(&mut cx, "gamma");

    cx.assert_state("alpha beta «gammaˇ»", Mode::HelixNormal);
    assert_eq!(cx.active_operator(), None);
}

#[gpui::test]
async fn test_helix_jump_moves_to_target_word(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("ˇone two three", Mode::HelixNormal);

    jump_to_word(&mut cx, "three");

    cx.assert_state("one two «threeˇ»", Mode::HelixNormal);
    assert_eq!(cx.active_operator(), None);
}

#[gpui::test]
async fn test_helix_jump_includes_line_selection_targets(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("alpha beta\nˇfoo bar baz\nqux quux", Mode::HelixNormal);

    cx.simulate_keystrokes("x");
    jump_to_word(&mut cx, "bar");

    cx.assert_state("alpha beta\nfoo «barˇ» baz\nqux quux", Mode::HelixNormal);
    assert_eq!(cx.active_operator(), None);
}

#[gpui::test]
async fn test_vim_jump_moves_to_target_word_start(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    bind_vim_jump_to_word(&mut cx, "g z");
    cx.set_state("ˇone two three", Mode::Normal);

    jump_to_word_with_keystrokes(&mut cx, "g z", "two");

    cx.assert_state("one ˇtwo three", Mode::Normal);
    assert_eq!(cx.active_operator(), None);
}

#[gpui::test]
async fn test_vim_jump_keeps_normal_cursor_shape(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    bind_vim_jump_to_word(&mut cx, "g z");
    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.vim.get_or_insert_default().cursor_shape =
                    Some(settings::CursorShapeSettings {
                        normal: Some(settings::CursorShape::Bar),
                        ..Default::default()
                    });
            });
        });
    });
    cx.set_state("ˇone two three", Mode::Normal);

    cx.simulate_keystrokes("g z");

    assert!(
        matches!(cx.active_operator(), Some(Operator::HelixJump { .. })),
        "expected HelixJump operator to be active"
    );
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.cursor_shape(), CursorShape::Bar);
    });
}

#[gpui::test]
async fn test_vim_visual_jump_extends_selection(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    bind_vim_jump_to_word(&mut cx, "g z");
    cx.set_state("one «twoˇ» three four", Mode::Visual);

    jump_to_word_with_keystrokes(&mut cx, "g z", "three");

    cx.assert_state("one «two tˇ»hree four", Mode::Visual);
    assert_eq!(cx.active_operator(), None);
}

#[gpui::test]
async fn test_vim_visual_jump_extends_selection_backward(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    bind_vim_jump_to_word(&mut cx, "g z");
    cx.set_state("one two «threeˇ» four", Mode::Visual);

    jump_to_word_with_keystrokes(&mut cx, "g z", "one");

    cx.assert_state("«ˇone two three» four", Mode::Visual);
    assert_eq!(cx.active_operator(), None);
}

#[gpui::test]
async fn test_helix_jump_extends_selection_forward(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("one «twoˇ» three four", Mode::HelixSelect);

    jump_to_word(&mut cx, "four");

    cx.assert_state("one «two three fourˇ»", Mode::HelixSelect);
    assert_eq!(cx.active_operator(), None);
}

#[gpui::test]
async fn test_helix_jump_extends_selection_backward_from_forward_selection(
    cx: &mut gpui::TestAppContext,
) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("one «twoˇ» three four", Mode::HelixSelect);

    jump_to_word(&mut cx, "one");

    cx.assert_state("«ˇone two» three four", Mode::HelixSelect);
    assert_eq!(cx.active_operator(), None);
}

#[gpui::test]
async fn test_helix_jump_extends_reversed_selection_backward(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("one two «ˇthree» four", Mode::HelixSelect);

    jump_to_word(&mut cx, "one");

    cx.assert_state("«ˇone two three» four", Mode::HelixSelect);
    assert_eq!(cx.active_operator(), None);
}

#[gpui::test]
async fn test_helix_jump_prioritizes_nearby_targets_before_truncating(
    cx: &mut gpui::TestAppContext,
) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    let cursor_index = 850usize;
    let target_word = format!("w{:03}", cursor_index + 1);
    let early_word = "w010".to_string();
    let text = (0..900usize)
        .map(|index| {
            let word = format!("w{index:03}");
            if index == cursor_index {
                format!("ˇ{word}")
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    cx.set_state(&text, Mode::HelixNormal);

    let labels = helix_jump_labels_for_full_buffer(&mut cx);

    assert_eq!(labels.len(), HELIX_JUMP_LABEL_LIMIT);
    assert!(
        labels.iter().any(|(_, word)| word == &target_word),
        "expected nearby target {target_word:?} to survive truncation"
    );
    assert!(
        !labels.iter().any(|(_, word)| word == &early_word),
        "expected distant early target {early_word:?} to be truncated first"
    );
}

#[gpui::test]
async fn test_helix_jump_label_ordering_alternates_directions(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("aaa bbb ccc ˇddd eee fff ggg", Mode::HelixNormal);

    let first_labels = helix_jump_labels_for_full_buffer(&mut cx)
        .into_iter()
        .take(6)
        .collect::<Vec<_>>();

    assert_eq!(
        first_labels,
        vec![
            ("aa".to_string(), "eee".to_string()),
            ("ab".to_string(), "ccc".to_string()),
            ("ac".to_string(), "fff".to_string()),
            ("ad".to_string(), "bbb".to_string()),
            ("ae".to_string(), "ggg".to_string()),
            ("af".to_string(), "aaa".to_string()),
        ]
    );
}

mod appearance;

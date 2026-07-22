use super::*;

#[gpui::test]
async fn test_hover_preconditions(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            ..Default::default()
        },
        cx,
    )
    .await;

    macro_rules! assert_no_highlight {
        ($cx:expr) => {
            // No highlight
            $cx.update_editor(|editor, window, cx| {
                assert!(
                    editor
                        .snapshot(window, cx)
                        .text_highlight_ranges(HighlightKey::HoveredLinkState)
                        .unwrap_or_default()
                        .1
                        .is_empty()
                );
            });
        };
    }

    // No link
    cx.set_state(indoc! {"
        Let's test a [complex](https://mav.dev/channel/) caseˇ.
    "});
    assert_no_highlight!(cx);

    // No modifier
    let screen_coord = cx.pixel_position(indoc! {"
        Let's test a [complex](https://mav.dev/channel/ˇ) case.
        "});
    cx.simulate_mouse_move(screen_coord, None, Modifiers::none());
    assert_no_highlight!(cx);

    // Modifier active
    let screen_coord = cx.pixel_position(indoc! {"
        Let's test a [complex](https://mav.dev/channeˇl/) case.
        "});
    cx.simulate_mouse_move(screen_coord, None, Modifiers::secondary_key());
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
        Let's test a [complex](«https://mav.dev/channel/ˇ») case.
    "},
    );
}

use super::*;

#[gpui::test]
async fn test_urls(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"
        Let's test a [complex](https://mav.dev/channel/had-(oops)) caseˇ.
    "});

    let screen_coord = cx.pixel_position(indoc! {"
        Let's test a [complex](https://mav.dev/channel/had-(ˇoops)) case.
        "});

    cx.simulate_mouse_move(screen_coord, None, Modifiers::secondary_key());
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
        Let's test a [complex](«https://mav.dev/channel/had-(oops)ˇ») case.
    "},
    );

    cx.simulate_click(screen_coord, Modifiers::secondary_key());
    assert_eq!(
        cx.opened_url(),
        Some("https://mav.dev/channel/had-(oops)".into())
    );
}

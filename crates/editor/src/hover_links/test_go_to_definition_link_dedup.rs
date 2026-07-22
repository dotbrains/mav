use super::*;

#[gpui::test]
async fn test_go_to_definition_link_dedup(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            definition_provider: Some(lsp::OneOf::Left(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"
        fn ˇtest() { do_work(); }
        fn do_work() { test(); }
    "});

    let request_count = Arc::new(AtomicUsize::new(0));
    let _requests = cx.set_request_handler::<GotoDefinition, _, _>({
        let request_count = request_count.clone();
        move |url, _, _| {
            request_count.fetch_add(1, Ordering::SeqCst);
            async move {
                // Return a bare `Location`, not an `originSelectionRange`
                // so we can confirm that jiggling the mouse within the same
                // symbol range does not trigger a second request, even
                // though `originSelectionRange` was not returned.
                Ok(Some(lsp::GotoDefinitionResponse::Scalar(lsp::Location {
                    uri: url,
                    range: lsp::Range::default(),
                })))
            }
        }
    });

    let symbol_start = cx.pixel_position(indoc! {"
        fn test() { ˇdo_work(); }
        fn do_work() { test(); }
    "});
    let symbol_end = cx.pixel_position(indoc! {"
        fn test() { do_worˇk(); }
        fn do_work() { test(); }
    "});
    let other_symbol = cx.pixel_position(indoc! {"
        fn test() { do_work(); }
        fn do_work() { teˇst(); }
    "});

    cx.simulate_mouse_move(symbol_start, None, Modifiers::secondary_key());
    cx.run_until_parked();

    cx.simulate_mouse_move(symbol_end, None, Modifiers::secondary_key());
    cx.run_until_parked();

    cx.simulate_mouse_move(other_symbol, None, Modifiers::secondary_key());
    cx.run_until_parked();

    assert_eq!(
        request_count.load(Ordering::SeqCst),
        2,
        "expected one request per symbol, reused within a symbol"
    );
}

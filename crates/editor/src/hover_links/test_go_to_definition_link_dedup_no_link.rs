use super::*;

#[gpui::test]
async fn test_go_to_definition_link_dedup_no_link(cx: &mut gpui::TestAppContext) {
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

        move |_, _, _| {
            request_count.fetch_add(1, Ordering::SeqCst);

            // Simulate response from the language server, reporting
            // that no link was found.
            async move { Ok(None) }
        }
    });

    let first_point = cx.pixel_position(indoc! {"
        fn test() { do_wˇork(); }
        fn do_work() { test(); }
    "});
    let second_point = cx.pixel_position(indoc! {"
        fn test() { do_woˇrk(); }
        fn do_work() { test(); }
    "});

    cx.simulate_mouse_move(first_point, None, Modifiers::secondary_key());
    cx.run_until_parked();

    cx.simulate_mouse_move(second_point, None, Modifiers::secondary_key());
    cx.run_until_parked();

    // Jiggle within the same character should not produce a new request,
    // even though the previous response was empty and produced no link to
    // highlight.
    cx.simulate_mouse_move(second_point, None, Modifiers::secondary_key());
    cx.run_until_parked();

    assert_eq!(
        request_count.load(Ordering::SeqCst),
        2,
        "expected one definition request per distinct position"
    );
}

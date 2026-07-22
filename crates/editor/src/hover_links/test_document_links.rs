use super::*;

#[gpui::test]
async fn test_document_links(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            document_link_provider: Some(lsp::DocumentLinkOptions {
                resolve_provider: Some(false),
                work_done_progress_options: lsp::WorkDoneProgressOptions::default(),
            }),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"
        // See LICENSE for details
        fn main() {
            println!(\"hello\");
        }ˇ
    "});

    let link_range = cx.lsp_range(indoc! {"
        // See «LICENSE» for details
        fn main() {
            println!(\"hello\");
        }
    "});

    let mut requests = cx
        .lsp
        .set_request_handler::<lsp::request::DocumentLinkRequest, _, _>(move |_, _| async move {
            Ok(Some(vec![lsp::DocumentLink {
                range: link_range,
                target: Some(lsp::Uri::from_str("https://opensource.org/licenses/MIT").unwrap()),
                tooltip: Some("Open license".to_string()),
                data: None,
            }]))
        });

    // Trigger document link fetch via LSP data refresh
    cx.run_until_parked();
    requests.next().await;
    cx.run_until_parked();

    // Cmd-hover over "LICENSE" should highlight it as a link
    let screen_coord = cx.pixel_position(indoc! {"
        // See LICˇENSE for details
        fn main() {
            println!(\"hello\");
        }
    "});

    cx.simulate_mouse_move(screen_coord, None, Modifiers::secondary_key());
    cx.run_until_parked();

    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
        // See «LICENSEˇ» for details
        fn main() {
            println!(\"hello\");
        }
    "},
    );

    // Clicking opens the URL
    cx.simulate_click(screen_coord, Modifiers::secondary_key());
    assert_eq!(
        cx.opened_url(),
        Some("https://opensource.org/licenses/MIT".into())
    );
}

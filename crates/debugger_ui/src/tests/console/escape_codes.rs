use super::*;

#[gpui::test]
async fn test_escape_code_processing(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    fs.insert_tree(
        path!("/project"),
        json!({
            "main.rs": "First line\nSecond line\nThird line\nFourth line",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let workspace = init_test_workspace(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(*workspace, cx);
    workspace
        .update(cx, |workspace, window, cx| {
            workspace.focus_panel::<DebugPanel>(window, cx);
        })
        .unwrap();

    let session = start_debug_session(&workspace, cx, |_| {}).unwrap();
    let client = session.read_with(cx, |session, _| session.adapter_client().unwrap());

    client.on_request::<StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: Vec::default(),
            total_frames: None,
        })
    });

    client
        .fake_event(dap::messages::Events::Output(dap::OutputEvent {
            category: None,
            output: "Checking latest version of JavaScript...".to_string(),
            data: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            group: None,
            location_reference: None,
        }))
        .await;
    client
        .fake_event(dap::messages::Events::Output(dap::OutputEvent {
            category: None,
            output: "   \u{1b}[1m\u{1b}[38;2;173;127;168m▲ Next.js 15.1.5\u{1b}[39m\u{1b}[22m"
                .to_string(),
            data: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            group: None,
            location_reference: None,
        }))
        .await;
    client
        .fake_event(dap::messages::Events::Output(dap::OutputEvent {
            category: None,
            output: "   - Local:        http://localhost:3000\n   - Network:      http://192.168.1.144:3000\n\n \u{1b}[32m\u{1b}[1m✓\u{1b}[22m\u{1b}[39m Starting..."
                .to_string(),
            data: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            group: None,
            location_reference: None,
        }))
        .await;
    client
        .fake_event(dap::messages::Events::Output(dap::OutputEvent {
            category: None,
            output: "Something else...".to_string(),
            data: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            group: None,
            location_reference: None,
        }))
        .await;
    client
        .fake_event(dap::messages::Events::Output(dap::OutputEvent {
            category: None,
            output: " \u{1b}[32m\u{1b}[1m✓\u{1b}[22m\u{1b}[39m Ready in 1009ms\n".to_string(),
            data: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            group: None,
            location_reference: None,
        }))
        .await;

    client
        .fake_event(dap::messages::Events::Output(dap::OutputEvent {
            category: None,
            output: "\u{1b}[41m\u{1b}[37mBoth background and foreground!".to_string(),
            data: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            group: None,
            location_reference: None,
        }))
        .await;
    client
        .fake_event(dap::messages::Events::Output(dap::OutputEvent {
            category: None,
            output: "Even more...".to_string(),
            data: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            group: None,
            location_reference: None,
        }))
        .await;

    cx.run_until_parked();

    let _running_state =
        active_debug_session_panel(workspace, cx).update_in(cx, |item, window, cx| {
            cx.focus_self(window);
            item.running_state().update(cx, |this, cx| {
                this.console()
                    .update(cx, |this, cx| this.update_output(window, cx));
            });

            item.running_state().clone()
        });

    cx.run_until_parked();

    workspace
        .update(cx, |workspace, window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let active_debug_session_panel = debug_panel
                .update(cx, |this, _| this.active_session())
                .unwrap();

            let editor =
                active_debug_session_panel
                    .read(cx)
                    .running_state()
                    .read(cx)
                    .console()
                    .read(cx)
                    .editor().clone();

            assert_eq!(
                "Checking latest version of JavaScript...\n   ▲ Next.js 15.1.5\n   - Local:        http://localhost:3000\n   - Network:      http://192.168.1.144:3000\n\n ✓ Starting...\nSomething else...\n ✓ Ready in 1009ms\nBoth background and foreground!\nEven more...\n",
                editor
                    .read(cx)
                    .text(cx)
                    .as_str()
            );

            let text_highlights = editor.update(cx, |editor, cx| {
                let mut text_highlights = editor.all_text_highlights(window, cx).into_iter().flat_map(|(_, ranges)| ranges).collect::<Vec<_>>();
                text_highlights.sort_by_key(|hl| hl.start);
                text_highlights
            });
            pretty_assertions::assert_eq!(
                text_highlights,
                [
                    DisplayPoint::new(DisplayRow(1), 3)..DisplayPoint::new(DisplayRow(1), 21),
                    DisplayPoint::new(DisplayRow(1), 21)..DisplayPoint::new(DisplayRow(2), 0),
                    DisplayPoint::new(DisplayRow(5), 1)..DisplayPoint::new(DisplayRow(5), 4),
                    DisplayPoint::new(DisplayRow(5), 4)..DisplayPoint::new(DisplayRow(6), 0),
                    DisplayPoint::new(DisplayRow(7), 1)..DisplayPoint::new(DisplayRow(7), 4),
                    DisplayPoint::new(DisplayRow(7), 4)..DisplayPoint::new(DisplayRow(8), 0),
                    DisplayPoint::new(DisplayRow(8), 0)..DisplayPoint::new(DisplayRow(9), 0),
                ]
            );

            let background_highlights = editor.update(cx, |editor, cx| {
                editor.all_text_background_highlights(window, cx).into_iter().map(|(range, _)| range).collect::<Vec<_>>()
            });
            pretty_assertions::assert_eq!(
                background_highlights,
                [
                    DisplayPoint::new(DisplayRow(8), 0)..DisplayPoint::new(DisplayRow(9), 0),
                ]
            )
        })
        .unwrap();
}

// #[gpui::test]
// async fn test_grouped_output(executor: BackgroundExecutor, cx: &mut TestAppContext) {
//     init_test(cx);

//     let fs = FakeFs::new(executor.clone());

//     fs.insert_tree(
//         "/project",
//         json!({
//             "main.rs": "First line\nSecond line\nThird line\nFourth line",
//         }),
//     )
//     .await;

//     let project = Project::test(fs, ["/project".as_ref()], cx).await;
//     let workspace = init_test_workspace(&project, cx).await;
//     let cx = &mut VisualTestContext::from_window(*workspace, cx);

//     let task = project.update(cx, |project, cx| {
//         project.start_debug_session(

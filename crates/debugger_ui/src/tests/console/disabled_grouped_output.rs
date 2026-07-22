use super::*;

//             cx,
//         )
//     });

//     let session = task.await.unwrap();
//     let client = session.update(cx, |session, _| session.adapter_client().unwrap());

//     client
//         .on_request::<StackTrace, _>(move |_, _| {
//             Ok(dap::StackTraceResponse {
//                 stack_frames: Vec::default(),
//                 total_frames: None,
//             })
//         })
//         .await;

//     client
//         .fake_event(dap::messages::Events::Stopped(dap::StoppedEvent {
//             reason: dap::StoppedEventReason::Pause,
//             description: None,
//             thread_id: Some(1),
//             preserve_focus_hint: None,
//             text: None,
//             all_threads_stopped: None,
//             hit_breakpoint_ids: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: None,
//             output: "First line".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: None,
//             location_reference: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: Some(dap::OutputEventCategory::Stdout),
//             output: "First group".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: Some(dap::OutputEventGroup::Start),
//             location_reference: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: Some(dap::OutputEventCategory::Stdout),
//             output: "First item in group 1".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: None,
//             location_reference: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: Some(dap::OutputEventCategory::Stdout),
//             output: "Second item in group 1".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: None,
//             location_reference: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: Some(dap::OutputEventCategory::Stdout),
//             output: "Second group".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: Some(dap::OutputEventGroup::Start),
//             location_reference: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: Some(dap::OutputEventCategory::Stdout),
//             output: "First item in group 2".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: None,
//             location_reference: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: Some(dap::OutputEventCategory::Stdout),
//             output: "Second item in group 2".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: None,
//             location_reference: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: Some(dap::OutputEventCategory::Stdout),
//             output: "End group 2".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: Some(dap::OutputEventGroup::End),
//             location_reference: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: Some(dap::OutputEventCategory::Stdout),
//             output: "Third group".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: Some(dap::OutputEventGroup::StartCollapsed),
//             location_reference: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: Some(dap::OutputEventCategory::Stdout),
//             output: "First item in group 3".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: None,
//             location_reference: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: Some(dap::OutputEventCategory::Stdout),
//             output: "Second item in group 3".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: None,
//             location_reference: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: Some(dap::OutputEventCategory::Stdout),
//             output: "End group 3".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: Some(dap::OutputEventGroup::End),
//             location_reference: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: Some(dap::OutputEventCategory::Stdout),
//             output: "Third item in group 1".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: None,
//             location_reference: None,
//         }))
//         .await;

//     client
//         .fake_event(dap::messages::Events::Output(dap::OutputEvent {
//             category: Some(dap::OutputEventCategory::Stdout),
//             output: "Second item".to_string(),
//             data: None,
//             variables_reference: None,
//             source: None,
//             line: None,
//             column: None,
//             group: Some(dap::OutputEventGroup::End),
//             location_reference: None,
//         }))
//         .await;

//     cx.run_until_parked();

//     active_debug_session_panel(workspace, cx).update(cx, |debug_panel_item, cx| {
//         debug_panel_item
//             .mode()
//             .as_running()
//             .unwrap()
//             .update(cx, |running_state, cx| {
//                 running_state.console().update(cx, |console, cx| {
//                     console.editor().update(cx, |editor, cx| {
//                         pretty_assertions::assert_eq!(
//                             "
//                         First line
//                         First group
//                             First item in group 1
//                             Second item in group 1
//                             Second group
//                                 First item in group 2
//                                 Second item in group 2
//                             End group 2
//                         ⋯    End group 3
//                             Third item in group 1
//                         Second item
//                     "
//                             .unindent(),
//                             editor.display_text(cx)
//                         );
//                     })
//                 });
//             });
//     });

//     let shutdown_session = project.update(cx, |project, cx| {
//         project.dap_store().update(cx, |dap_store, cx| {
//             dap_store.shutdown_session(session.read(cx).session_id(), cx)
//         })
//     });

//     shutdown_session.await.unwrap();
// }

// todo(debugger): enable this again
// #[gpui::test]
// async fn test_evaluate_expression(executor: BackgroundExecutor, cx: &mut TestAppContext) {
//     init_test(cx);

//     const NEW_VALUE: &str = "{nested1: \"Nested 1 updated\", nested2: \"Nested 2 updated\"}";

//     let called_evaluate = Arc::new(AtomicBool::new(false));

//     let fs = FakeFs::new(executor.clone());

//     let test_file_content = r#"
//         const variable1 = {
//             nested1: "Nested 1",
//             nested2: "Nested 2",
//         };
//         const variable2 = "Value 2";
//         const variable3 = "Value 3";
//     "#
//     .unindent();

//     fs.insert_tree(
//         "/project",
//         json!({
//            "src": {
//                "test.js": test_file_content,
//            }
//         }),
//     )
//     .await;

//     let project = Project::test(fs, ["/project".as_ref()], cx).await;
//     let workspace = init_test_workspace(&project, cx).await;
//     let cx = &mut VisualTestContext::from_window(*workspace, cx);

//     let task = project.update(cx, |project, cx| {
//         project.start_debug_session(dap::test_config(None), cx)

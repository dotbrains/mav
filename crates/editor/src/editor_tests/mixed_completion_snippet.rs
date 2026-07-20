use super::*;

#[gpui::test]
async fn test_mixed_completions_with_multi_word_snippet(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;
    cx.lsp
        .set_request_handler::<lsp::request::Completion, _, _>(move |_, _| async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "unsafe".into(),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range {
                            start: lsp::Position {
                                line: 0,
                                character: 9,
                            },
                            end: lsp::Position {
                                line: 0,
                                character: 11,
                            },
                        },
                        new_text: "unsafe".to_string(),
                    })),
                    insert_text_mode: Some(lsp::InsertTextMode::AS_IS),
                    ..Default::default()
                },
            ])))
        });

    cx.update_editor(|editor, _, cx| {
        editor.project().unwrap().update(cx, |project, cx| {
            project.snippets().update(cx, |snippets, _cx| {
                snippets.add_snippet_for_test(
                    None,
                    PathBuf::from("test_snippets.json"),
                    vec![
                        Arc::new(project::snippet_provider::Snippet {
                            prefix: vec![
                                "unlimited word count".to_string(),
                                "unlimit word count".to_string(),
                                "unlimited unknown".to_string(),
                            ],
                            body: "this is many words".to_string(),
                            description: Some("description".to_string()),
                            name: "multi-word snippet test".to_string(),
                        }),
                        Arc::new(project::snippet_provider::Snippet {
                            prefix: vec!["unsnip".to_string(), "@few".to_string()],
                            body: "fewer words".to_string(),
                            description: Some("alt description".to_string()),
                            name: "other name".to_string(),
                        }),
                        Arc::new(project::snippet_provider::Snippet {
                            prefix: vec!["ab aa".to_string()],
                            body: "abcd".to_string(),
                            description: None,
                            name: "alphabet".to_string(),
                        }),
                    ],
                );
            });
        })
    });

    let get_completions = |cx: &mut EditorLspTestContext| {
        cx.update_editor(|editor, _, _| match &*editor.context_menu.borrow() {
            Some(CodeContextMenu::Completions(context_menu)) => {
                let entries = context_menu.entries.borrow();
                entries
                    .iter()
                    .filter_map(|entry| entry.as_match().map(|m| m.string.clone()))
                    .collect_vec()
            }
            _ => vec![],
        })
    };

    // snippets:
    //  @foo
    //  foo bar
    //
    // when typing:
    //
    // when typing:
    //  - if I type a symbol "open the completions with snippets only"
    //  - if I type a word character "open the completions menu" (if it had been open snippets only, clear it out)
    //
    // stuff we need:
    //  - filtering logic change?
    //  - remember how far back the completion started.

    let test_cases: &[(&str, &[&str])] = &[
        (
            "un",
            &[
                "unsafe",
                "unlimit word count",
                "unlimited unknown",
                "unlimited word count",
                "unsnip",
            ],
        ),
        (
            "u ",
            &[
                "unlimit word count",
                "unlimited unknown",
                "unlimited word count",
            ],
        ),
        ("u a", &["ab aa", "unsafe"]), // unsAfe
        (
            "u u",
            &[
                "unsafe",
                "unlimit word count",
                "unlimited unknown", // ranked highest among snippets
                "unlimited word count",
                "unsnip",
            ],
        ),
        ("uw c", &["unlimit word count", "unlimited word count"]),
        (
            "u w",
            &[
                "unlimit word count",
                "unlimited word count",
                "unlimited unknown",
            ],
        ),
        ("u w ", &["unlimit word count", "unlimited word count"]),
        (
            "u ",
            &[
                "unlimit word count",
                "unlimited unknown",
                "unlimited word count",
            ],
        ),
        ("wor", &[]),
        ("uf", &["unsafe"]),
        ("af", &["unsafe"]),
        ("afu", &[]),
        (
            "ue",
            &["unsafe", "unlimited unknown", "unlimited word count"],
        ),
        ("@", &["@few"]),
        ("@few", &["@few"]),
        ("@ ", &[]),
        ("a@", &["@few"]),
        ("a@f", &["@few", "unsafe"]),
        ("a@fw", &["@few"]),
        ("a", &["ab aa", "unsafe"]),
        ("aa", &["ab aa"]),
        ("aaa", &["ab aa"]),
        ("ab", &["ab aa"]),
        ("ab ", &["ab aa"]),
        ("ab a", &["ab aa", "unsafe"]),
        ("ab ab", &["ab aa"]),
        ("ab ab aa", &["ab aa"]),
    ];

    for &(input_to_simulate, expected_completions) in test_cases {
        cx.set_state("fn a() { ˇ }\n");
        for c in input_to_simulate.split("") {
            cx.simulate_input(c);
            cx.run_until_parked();
        }
        let expected_completions = expected_completions
            .iter()
            .map(|s| s.to_string())
            .collect_vec();
        assert_eq!(
            get_completions(&mut cx),
            expected_completions,
            "< actual / expected >, input = {input_to_simulate:?}",
        );
    }
}

/// Handle completion request passing a marked string specifying where the completion
/// should be triggered from using '|' character, what range should be replaced, and what completions
/// should be returned using '<' and '>' to delimit the range.
///
/// Also see `handle_completion_request_with_insert_and_replace`.
#[track_caller]
pub fn handle_completion_request(
    marked_string: &str,
    completions: Vec<&'static str>,
    is_incomplete: bool,
    counter: Arc<AtomicUsize>,
    cx: &mut EditorLspTestContext,
) -> impl Future<Output = ()> {
    let complete_from_marker: TextRangeMarker = '|'.into();
    let replace_range_marker: TextRangeMarker = ('<', '>').into();
    let (_, mut marked_ranges) = marked_text_ranges_by(
        marked_string,
        vec![complete_from_marker.clone(), replace_range_marker.clone()],
    );

    let complete_from_position = cx.to_lsp(MultiBufferOffset(
        marked_ranges.remove(&complete_from_marker).unwrap()[0].start,
    ));
    let range = marked_ranges.remove(&replace_range_marker).unwrap()[0].clone();
    let replace_range =
        cx.to_lsp_range(MultiBufferOffset(range.start)..MultiBufferOffset(range.end));

    let mut request =
        cx.set_request_handler::<lsp::request::Completion, _, _>(move |url, params, _| {
            let completions = completions.clone();
            counter.fetch_add(1, atomic::Ordering::Release);
            async move {
                assert_eq!(params.text_document_position.text_document.uri, url.clone());
                assert_eq!(
                    params.text_document_position.position,
                    complete_from_position
                );
                Ok(Some(lsp::CompletionResponse::List(lsp::CompletionList {
                    is_incomplete,
                    item_defaults: None,
                    items: completions
                        .iter()
                        .map(|completion_text| lsp::CompletionItem {
                            label: completion_text.to_string(),
                            text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                                range: replace_range,
                                new_text: completion_text.to_string(),
                            })),
                            ..Default::default()
                        })
                        .collect(),
                })))
            }
        });

    async move {
        request.next().await;
    }
}

/// Similar to `handle_completion_request`, but a [`CompletionTextEdit::InsertAndReplace`] will be
/// given instead, which also contains an `insert` range.
///
/// This function uses markers to define ranges:
/// - `|` marks the cursor position
/// - `<>` marks the replace range
/// - `[]` marks the insert range (optional, defaults to `replace_range.start..cursor_pos`which is what Rust-Analyzer provides)
pub fn handle_completion_request_with_insert_and_replace(
    cx: &mut EditorLspTestContext,
    marked_string: &str,
    completions: Vec<(&'static str, &'static str)>, // (label, new_text)
    counter: Arc<AtomicUsize>,
) -> impl Future<Output = ()> {
    let complete_from_marker: TextRangeMarker = '|'.into();
    let replace_range_marker: TextRangeMarker = ('<', '>').into();
    let insert_range_marker: TextRangeMarker = ('{', '}').into();

    let (_, mut marked_ranges) = marked_text_ranges_by(
        marked_string,
        vec![
            complete_from_marker.clone(),
            replace_range_marker.clone(),
            insert_range_marker.clone(),
        ],
    );

    let complete_from_position = cx.to_lsp(MultiBufferOffset(
        marked_ranges.remove(&complete_from_marker).unwrap()[0].start,
    ));
    let range = marked_ranges.remove(&replace_range_marker).unwrap()[0].clone();
    let replace_range =
        cx.to_lsp_range(MultiBufferOffset(range.start)..MultiBufferOffset(range.end));

    let insert_range = match marked_ranges.remove(&insert_range_marker) {
        Some(ranges) if !ranges.is_empty() => {
            let range1 = ranges[0].clone();
            cx.to_lsp_range(MultiBufferOffset(range1.start)..MultiBufferOffset(range1.end))
        }
        _ => lsp::Range {
            start: replace_range.start,
            end: complete_from_position,
        },
    };

    let mut request =
        cx.set_request_handler::<lsp::request::Completion, _, _>(move |url, params, _| {
            let completions = completions.clone();
            counter.fetch_add(1, atomic::Ordering::Release);
            async move {
                assert_eq!(params.text_document_position.text_document.uri, url.clone());
                assert_eq!(
                    params.text_document_position.position, complete_from_position,
                    "marker `|` position doesn't match",
                );
                Ok(Some(lsp::CompletionResponse::Array(
                    completions
                        .iter()
                        .map(|(label, new_text)| lsp::CompletionItem {
                            label: label.to_string(),
                            text_edit: Some(lsp::CompletionTextEdit::InsertAndReplace(
                                lsp::InsertReplaceEdit {
                                    insert: insert_range,
                                    replace: replace_range,
                                    new_text: new_text.to_string(),
                                },
                            )),
                            ..Default::default()
                        })
                        .collect(),
                )))
            }
        });

    async move {
        request.next().await;
    }
}

use super::*;

fn accept_completion(cx: &mut EditorTestContext) {
    cx.update_editor(|editor, window, cx| {
        editor.accept_edit_prediction(&crate::AcceptEditPrediction, window, cx)
    })
}

fn propose_edits<T: ToOffset>(
    provider: &Entity<FakeEditPredictionDelegate>,
    edits: Vec<(Range<T>, &str)>,
    cx: &mut EditorTestContext,
) {
    propose_edits_with_cursor_position(provider, edits, None, cx);
}

async fn propose_edits_with_preview<T: ToOffset + Clone>(
    provider: &Entity<FakeEditPredictionDelegate>,
    edits: Vec<(Range<T>, &str)>,
    cx: &mut EditorTestContext,
) {
    let snapshot = cx.buffer_snapshot();
    let edits = edits
        .into_iter()
        .map(|(range, text)| {
            let anchor_range =
                snapshot.anchor_after(range.start.clone())..snapshot.anchor_before(range.end);
            (anchor_range, Arc::<str>::from(text))
        })
        .collect::<Vec<_>>();

    let preview_edits = edits
        .iter()
        .map(|(range, text)| (range.clone(), text.clone()))
        .collect::<Arc<[_]>>();

    let edit_preview = cx
        .buffer(|buffer: &Buffer, app| buffer.preview_edits(preview_edits, app))
        .await;

    let provider_edits = edits.into_iter().collect();

    cx.update(|_, cx| {
        provider.update(cx, |provider, _| {
            provider.set_edit_prediction(Some(edit_prediction_types::EditPrediction::Local {
                id: None,
                edits: provider_edits,
                cursor_position: None,
                edit_preview: Some(edit_preview),
            }))
        })
    });
}

fn propose_edits_with_cursor_position<T: ToOffset>(
    provider: &Entity<FakeEditPredictionDelegate>,
    edits: Vec<(Range<T>, &str)>,
    cursor_offset: Option<usize>,
    cx: &mut EditorTestContext,
) {
    let snapshot = cx.buffer_snapshot();
    let cursor_position = cursor_offset
        .map(|offset| PredictedCursorPosition::at_anchor(snapshot.anchor_after(offset)));
    let edits = edits.into_iter().map(|(range, text)| {
        let range = snapshot.anchor_after(range.start)..snapshot.anchor_before(range.end);
        (range, text.into())
    });

    cx.update(|_, cx| {
        provider.update(cx, |provider, _| {
            provider.set_edit_prediction(Some(edit_prediction_types::EditPrediction::Local {
                id: None,
                edits: edits.collect(),
                cursor_position,
                edit_preview: None,
            }))
        })
    });
}

fn propose_edits_with_cursor_position_in_insertion<T: ToOffset>(
    provider: &Entity<FakeEditPredictionDelegate>,
    edits: Vec<(Range<T>, &str)>,
    anchor_offset: usize,
    offset_within_insertion: usize,
    cx: &mut EditorTestContext,
) {
    let snapshot = cx.buffer_snapshot();
    // Use anchor_before (left bias) so the anchor stays at the insertion point
    // rather than moving past the inserted text
    let cursor_position = Some(PredictedCursorPosition::new(
        snapshot.anchor_before(anchor_offset),
        offset_within_insertion,
    ));
    let edits = edits.into_iter().map(|(range, text)| {
        let range = snapshot.anchor_after(range.start)..snapshot.anchor_before(range.end);
        (range, text.into())
    });

    cx.update(|_, cx| {
        provider.update(cx, |provider, _| {
            provider.set_edit_prediction(Some(edit_prediction_types::EditPrediction::Local {
                id: None,
                edits: edits.collect(),
                cursor_position,
                edit_preview: None,
            }))
        })
    });
}

async fn hidden_edit_prediction_snippet_test_context(
    cx: &mut gpui::TestAppContext,
) -> EditorTestContext {
    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);
    cx.update_editor(|editor, _, cx| {
        editor.set_menu_edit_predictions_policy(MenuEditPredictionsPolicy::Never);
        editor.project().unwrap().update(cx, |project, cx| {
            project.snippets().update(cx, |snippets, _cx| {
                let snippet = project::snippet_provider::Snippet {
                    prefix: vec!["Theta".to_string(), "turnstile".to_string()],
                    body: "⊢".to_string(),
                    description: Some("unicode symbol".to_string()),
                    name: "unicode snippets".to_string(),
                };
                snippets.add_snippet_for_test(
                    None,
                    PathBuf::from("test_snippets.json"),
                    vec![Arc::new(snippet)],
                );
            });
        })
    });
    cx.set_state("ˇ");

    propose_edits(&provider, vec![(0..0, "x")], &mut cx);
    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));
    cx
}

fn assign_editor_completion_provider(
    provider: Entity<FakeEditPredictionDelegate>,
    cx: &mut EditorTestContext,
) {
    cx.update_editor(|editor, window, cx| {
        editor.set_edit_prediction_provider(Some(provider), window, cx);
    })
}

fn assign_editor_completion_menu_provider(cx: &mut EditorTestContext) {
    cx.update_editor(|editor, _, _| {
        editor.set_completion_provider(Some(Rc::new(FakeCompletionMenuProvider)));
    });
}

fn propose_edits_non_mav<T: ToOffset>(
    provider: &Entity<FakeNonMavEditPredictionDelegate>,
    edits: Vec<(Range<T>, &str)>,
    cx: &mut EditorTestContext,
) {
    let snapshot = cx.buffer_snapshot();
    let edits = edits.into_iter().map(|(range, text)| {
        let range = snapshot.anchor_after(range.start)..snapshot.anchor_before(range.end);
        (range, text.into())
    });

    cx.update(|_, cx| {
        provider.update(cx, |provider, _| {
            provider.set_edit_prediction(Some(edit_prediction_types::EditPrediction::Local {
                id: None,
                edits: edits.collect(),
                cursor_position: None,
                edit_preview: None,
            }))
        })
    });
}

fn assign_editor_completion_provider_non_mav(
    provider: Entity<FakeNonMavEditPredictionDelegate>,
    cx: &mut EditorTestContext,
) {
    cx.update_editor(|editor, window, cx| {
        editor.set_edit_prediction_provider(Some(provider), window, cx);
    })
}

struct FakeCompletionMenuProvider;

impl CompletionProvider for FakeCompletionMenuProvider {
    fn completions(
        &self,
        buffer: &Entity<Buffer>,
        _buffer_position: text::Anchor,
        _trigger: CompletionContext,
        _window: &mut Window,
        cx: &mut Context<crate::Editor>,
    ) -> Task<anyhow::Result<Vec<CompletionResponse>>> {
        let replace_range = text::Anchor::min_max_range_for_buffer(buffer.read(cx).remote_id());
        let completions = ["fake_completion", "fake_completion_2"]
            .into_iter()
            .map(|label| Completion {
                replace_range: replace_range.clone(),
                new_text: label.to_string(),
                label: CodeLabel::plain(label.to_string(), None),
                documentation: None,
                source: CompletionSource::Custom,
                icon_path: None,
                icon_color: None,
                match_start: None,
                snippet_deduplication_key: None,
                insert_text_mode: None,
                confirm: None,
                group: None,
            })
            .collect();

        Task::ready(Ok(vec![CompletionResponse {
            completions,
            display_options: Default::default(),
            is_incomplete: false,
        }]))
    }

    fn is_completion_trigger(
        &self,
        _buffer: &Entity<Buffer>,
        _position: language::Anchor,
        _text: &str,
        _trigger_in_words: bool,
        _cx: &mut Context<crate::Editor>,
    ) -> bool {
        false
    }

    fn filter_completions(&self) -> bool {
        false
    }
}

#[derive(Default, Clone)]
pub struct FakeEditPredictionDelegate {
    pub completion: Option<edit_prediction_types::EditPrediction>,
    pub refresh_count: Arc<AtomicUsize>,
}

impl FakeEditPredictionDelegate {
    pub fn set_edit_prediction(
        &mut self,
        completion: Option<edit_prediction_types::EditPrediction>,
    ) {
        self.completion = completion;
    }
}

impl EditPredictionDelegate for FakeEditPredictionDelegate {
    fn name() -> &'static str {
        "fake-completion-provider"
    }

    fn display_name() -> &'static str {
        "Fake Completion Provider"
    }

    fn show_predictions_in_menu() -> bool {
        true
    }

    fn supports_jump_to_edit() -> bool {
        true
    }

    fn icons(&self, _cx: &gpui::App) -> EditPredictionIconSet {
        EditPredictionIconSet::new(IconName::MavPredict)
    }

    fn is_enabled(
        &self,
        _buffer: &gpui::Entity<language::Buffer>,
        _cursor_position: language::Anchor,
        _cx: &gpui::App,
    ) -> bool {
        true
    }

    fn is_refreshing(&self, _cx: &gpui::App) -> bool {
        false
    }

    fn refresh(
        &mut self,
        _buffer: gpui::Entity<language::Buffer>,
        _cursor_position: language::Anchor,
        _debounce: bool,
        _trigger: edit_prediction_types::EditPredictionRequestTrigger,
        _cx: &mut gpui::Context<Self>,
    ) {
        self.refresh_count.fetch_add(1, atomic::Ordering::SeqCst);
    }

    fn accept(&mut self, _cx: &mut gpui::Context<Self>) {}

    fn discard(
        &mut self,
        _reason: edit_prediction_types::EditPredictionDiscardReason,
        _cx: &mut gpui::Context<Self>,
    ) {
        self.completion.take();
    }

    fn suggest<'a>(
        &mut self,
        _buffer: &gpui::Entity<language::Buffer>,
        _cursor_position: language::Anchor,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<edit_prediction_types::EditPrediction> {
        self.completion.clone()
    }
}

#[derive(Default, Clone)]
pub struct FakeNonMavEditPredictionDelegate {
    pub completion: Option<edit_prediction_types::EditPrediction>,
}

impl FakeNonMavEditPredictionDelegate {
    pub fn set_edit_prediction(
        &mut self,
        completion: Option<edit_prediction_types::EditPrediction>,
    ) {
        self.completion = completion;
    }
}

impl EditPredictionDelegate for FakeNonMavEditPredictionDelegate {
    fn name() -> &'static str {
        "fake-non-mav-provider"
    }

    fn display_name() -> &'static str {
        "Fake Non-Mav Provider"
    }

    fn show_predictions_in_menu() -> bool {
        false
    }

    fn supports_jump_to_edit() -> bool {
        false
    }

    fn icons(&self, _cx: &gpui::App) -> EditPredictionIconSet {
        EditPredictionIconSet::new(IconName::MavPredict)
    }

    fn is_enabled(
        &self,
        _buffer: &gpui::Entity<language::Buffer>,
        _cursor_position: language::Anchor,
        _cx: &gpui::App,
    ) -> bool {
        true
    }

    fn is_refreshing(&self, _cx: &gpui::App) -> bool {
        false
    }

    fn refresh(
        &mut self,
        _buffer: gpui::Entity<language::Buffer>,
        _cursor_position: language::Anchor,
        _debounce: bool,
        _trigger: edit_prediction_types::EditPredictionRequestTrigger,
        _cx: &mut gpui::Context<Self>,
    ) {
    }

    fn accept(&mut self, _cx: &mut gpui::Context<Self>) {}

    fn discard(
        &mut self,
        _reason: edit_prediction_types::EditPredictionDiscardReason,
        _cx: &mut gpui::Context<Self>,
    ) {
        self.completion.take();
    }

    fn suggest<'a>(
        &mut self,
        _buffer: &gpui::Entity<language::Buffer>,
        _cursor_position: language::Anchor,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<edit_prediction_types::EditPrediction> {
        self.completion.clone()
    }
}

pub(super) fn load_default_keymap(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(
            settings::KeymapFile::load_asset_allow_partial_failure(
                settings::DEFAULT_KEYMAP_PATH,
                cx,
            )
            .expect("failed to load default keymap"),
        );
    });
}

pub(super) fn assert_editor_active_edit_completion(
    cx: &mut EditorTestContext,
    assert: impl FnOnce(MultiBufferSnapshot, &Vec<(Range<Anchor>, Arc<str>)>),
) {
    cx.editor(|editor, _, cx| {
        let completion_state = editor
            .active_edit_prediction
            .as_ref()
            .expect("editor has no active completion");

        if let EditPrediction::Edit { edits, .. } = &completion_state.completion {
            assert(editor.buffer().read(cx).snapshot(cx), edits);
        } else {
            panic!("expected edit completion");
        }
    })
}

pub(super) fn assert_editor_active_move_completion(
    cx: &mut EditorTestContext,
    assert: impl FnOnce(MultiBufferSnapshot, Anchor),
) {
    cx.editor(|editor, _, cx| {
        let completion_state = editor
            .active_edit_prediction
            .as_ref()
            .expect("editor has no active completion");

        if let EditPrediction::MoveWithin { target, .. } = &completion_state.completion {
            assert(editor.buffer().read(cx).snapshot(cx), *target);
        } else {
            panic!("expected move completion");
        }
    })
}

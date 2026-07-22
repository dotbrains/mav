use super::*;

struct EditorInlineAssists {
    assist_ids: Vec<InlineAssistId>,
    scroll_lock: Option<InlineAssistScrollLock>,
    highlight_updates: watch::Sender<()>,
    _update_highlights: Task<Result<()>>,
    _subscriptions: Vec<gpui::Subscription>,
}

struct InlineAssistScrollLock {
    assist_id: InlineAssistId,
    distance_from_top: ScrollOffset,
}

impl EditorInlineAssists {
    fn new(editor: &Entity<Editor>, window: &mut Window, cx: &mut App) -> Self {
        let (highlight_updates_tx, mut highlight_updates_rx) = watch::channel(());
        Self {
            assist_ids: Vec::new(),
            scroll_lock: None,
            highlight_updates: highlight_updates_tx,
            _update_highlights: cx.spawn({
                let editor = editor.downgrade();
                async move |cx| {
                    while let Ok(()) = highlight_updates_rx.changed().await {
                        let editor = editor.upgrade().context("editor was dropped")?;
                        cx.update_global(|assistant: &mut InlineAssistant, cx| {
                            assistant.update_editor_highlights(&editor, cx);
                        });
                    }
                    Ok(())
                }
            }),
            _subscriptions: vec![
                cx.observe_release_in(editor, window, {
                    let editor = editor.downgrade();
                    |_, window, cx| {
                        InlineAssistant::update_global(cx, |this, cx| {
                            this.handle_editor_release(editor, window, cx);
                        })
                    }
                }),
                window.observe(editor, cx, move |editor, window, cx| {
                    InlineAssistant::update_global(cx, |this, cx| {
                        this.handle_editor_change(editor, window, cx)
                    })
                }),
                window.subscribe(editor, cx, move |editor, event, window, cx| {
                    InlineAssistant::update_global(cx, |this, cx| {
                        this.handle_editor_event(editor, event, window, cx)
                    })
                }),
                editor.update(cx, |editor, cx| {
                    let editor_handle = cx.entity().downgrade();
                    editor.register_action(move |_: &editor::actions::Newline, window, cx| {
                        InlineAssistant::update_global(cx, |this, cx| {
                            if let Some(editor) = editor_handle.upgrade() {
                                this.handle_editor_newline(editor, window, cx)
                            }
                        })
                    })
                }),
                editor.update(cx, |editor, cx| {
                    let editor_handle = cx.entity().downgrade();
                    editor.register_action(move |_: &editor::actions::Cancel, window, cx| {
                        InlineAssistant::update_global(cx, |this, cx| {
                            if let Some(editor) = editor_handle.upgrade() {
                                this.handle_editor_cancel(editor, window, cx)
                            }
                        })
                    })
                }),
            ],
        }
    }
}

struct InlineAssistGroup {
    assist_ids: Vec<InlineAssistId>,
    linked: bool,
    active_assist_id: Option<InlineAssistId>,
}

impl InlineAssistGroup {
    fn new() -> Self {
        Self {
            assist_ids: Vec::new(),
            linked: true,
            active_assist_id: None,
        }
    }
}

fn build_assist_editor_renderer(editor: &Entity<PromptEditor<BufferCodegen>>) -> RenderBlock {
    let editor = editor.clone();

    Arc::new(move |cx: &mut BlockContext| {
        let editor_margins = editor.read(cx).editor_margins();

        *editor_margins.lock() = *cx.margins;
        editor.clone().into_any_element()
    })
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, Hash)]
struct InlineAssistGroupId(usize);

impl InlineAssistGroupId {
    fn post_inc(&mut self) -> InlineAssistGroupId {
        let id = *self;
        self.0 += 1;
        id
    }
}

pub struct InlineAssist {
    group_id: InlineAssistGroupId,
    range: Range<Anchor>,
    editor: WeakEntity<Editor>,
    decorations: Option<InlineAssistDecorations>,
    codegen: Entity<BufferCodegen>,
    _subscriptions: Vec<Subscription>,
    workspace: WeakEntity<Workspace>,
}

impl InlineAssist {
    fn new(
        assist_id: InlineAssistId,
        group_id: InlineAssistGroupId,
        editor: &Entity<Editor>,
        prompt_editor: &Entity<PromptEditor<BufferCodegen>>,
        prompt_block_id: CustomBlockId,
        tool_description_block_id: CustomBlockId,
        end_block_id: CustomBlockId,
        range: Range<Anchor>,
        codegen: Entity<BufferCodegen>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let prompt_editor_focus_handle = prompt_editor.focus_handle(cx);
        InlineAssist {
            group_id,
            editor: editor.downgrade(),
            decorations: Some(InlineAssistDecorations {
                prompt_block_id,
                prompt_editor: prompt_editor.clone(),
                removed_line_block_ids: Default::default(),
                model_explanation: Some(tool_description_block_id),
                end_block_id,
            }),
            range,
            codegen: codegen.clone(),
            workspace,
            _subscriptions: vec![
                window.on_focus_in(&prompt_editor_focus_handle, cx, move |_, cx| {
                    InlineAssistant::update_global(cx, |this, cx| {
                        this.handle_prompt_editor_focus_in(assist_id, cx)
                    })
                }),
                window.on_focus_out(&prompt_editor_focus_handle, cx, move |_, _, cx| {
                    InlineAssistant::update_global(cx, |this, cx| {
                        this.handle_prompt_editor_focus_out(assist_id, cx)
                    })
                }),
                window.subscribe(prompt_editor, cx, |prompt_editor, event, window, cx| {
                    InlineAssistant::update_global(cx, |this, cx| {
                        this.handle_prompt_editor_event(prompt_editor, event, window, cx)
                    })
                }),
                window.observe(&codegen, cx, {
                    let editor = editor.downgrade();
                    move |_, window, cx| {
                        if let Some(editor) = editor.upgrade() {
                            InlineAssistant::update_global(cx, |this, cx| {
                                if let Some(editor_assists) =
                                    this.assists_by_editor.get_mut(&editor.downgrade())
                                {
                                    editor_assists.highlight_updates.send(()).ok();
                                }

                                this.update_editor_blocks(&editor, assist_id, window, cx);
                            })
                        }
                    }
                }),
                window.subscribe(&codegen, cx, move |codegen, event, window, cx| {
                    InlineAssistant::update_global(cx, |this, cx| match event {
                        CodegenEvent::Undone => this.finish_assist(assist_id, false, window, cx),
                        CodegenEvent::Finished => {
                            let assist = if let Some(assist) = this.assists.get(&assist_id) {
                                assist
                            } else {
                                return;
                            };

                            if let CodegenStatus::Error(error) = codegen.read(cx).status(cx)
                                && assist.decorations.is_none()
                                && let Some(workspace) = assist.workspace.upgrade()
                            {
                                #[cfg(any(test, feature = "test-support"))]
                                if let Some(sender) = &mut this._inline_assistant_completions {
                                    sender
                                        .unbounded_send(Err(anyhow::anyhow!(
                                            "Inline assistant error: {}",
                                            error
                                        )))
                                        .ok();
                                }

                                let error = format!("Inline assistant error: {}", error);
                                workspace.update(cx, |workspace, cx| {
                                    struct InlineAssistantError;

                                    let id = NotificationId::composite::<InlineAssistantError>(
                                        assist_id.0,
                                    );

                                    workspace.show_toast(Toast::new(id, error), cx);
                                })
                            } else {
                                #[cfg(any(test, feature = "test-support"))]
                                if let Some(sender) = &mut this._inline_assistant_completions {
                                    sender.unbounded_send(Ok(assist_id)).ok();
                                }
                            }

                            if assist.decorations.is_none() {
                                this.finish_assist(assist_id, false, window, cx);
                            }
                        }
                    })
                }),
            ],
        }
    }

    fn user_prompt(&self, cx: &App) -> Option<String> {
        let decorations = self.decorations.as_ref()?;
        Some(decorations.prompt_editor.read(cx).prompt(cx))
    }

    fn mention_set(&self, cx: &App) -> Option<Entity<MentionSet>> {
        let decorations = self.decorations.as_ref()?;
        Some(decorations.prompt_editor.read(cx).mention_set().clone())
    }
}

struct InlineAssistDecorations {
    prompt_block_id: CustomBlockId,
    prompt_editor: Entity<PromptEditor<BufferCodegen>>,
    removed_line_block_ids: HashSet<CustomBlockId>,
    model_explanation: Option<CustomBlockId>,
    end_block_id: CustomBlockId,
}

fn merge_ranges(ranges: &mut Vec<Range<Anchor>>, buffer: &MultiBufferSnapshot) {
    ranges.sort_unstable_by(|a, b| {
        a.start
            .cmp(&b.start, buffer)
            .then_with(|| b.end.cmp(&a.end, buffer))
    });

    let mut ix = 0;
    while ix + 1 < ranges.len() {
        let b = ranges[ix + 1].clone();
        let a = &mut ranges[ix];
        if a.end.cmp(&b.start, buffer).is_gt() {
            if a.end.cmp(&b.end, buffer).is_lt() {
                a.end = b.end;
            }
            ranges.remove(ix + 1);
        } else {
            ix += 1;
        }
    }
}

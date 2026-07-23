use super::*;

impl DivInspector {
    pub(super) fn create_editor(
        &self,
        buffer: Entity<Buffer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Editor> {
        cx.new(|cx| {
            let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
            let mut editor = Editor::new(
                EditorMode::full(),
                multi_buffer,
                Some(self.project.clone()),
                window,
                cx,
            );
            editor.set_soft_wrap_mode(SoftWrap::EditorWidth, cx);
            editor.set_show_line_numbers(false, cx);
            editor.set_show_code_actions(false, cx);
            editor.set_show_bookmarks(false, cx);
            editor.set_show_breakpoints(false, cx);
            editor.set_show_git_diff_gutter(false, cx);
            editor.set_show_runnables(false, cx);
            editor.disable_mouse_wheel_zoom();
            editor.set_show_edit_predictions(Some(false), window, cx);
            editor.set_minimap_visibility(MinimapVisibility::Disabled, window, cx);
            editor
        })
    }
}

impl Render for DivInspector {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .gap_2()
            .when_some(self.inspector_state.as_ref(), |this, inspector_state| {
                this.child(
                    v_flex()
                        .child(Label::new("Layout").size(LabelSize::Large))
                        .child(render_layout_state(inspector_state, cx)),
                )
            })
            .map(|this| match &self.state {
                State::Loading | State::BuffersLoaded { .. } => {
                    this.child(Label::new("Loading..."))
                }
                State::LoadError { message } => this.child(
                    div()
                        .w_full()
                        .border_1()
                        .border_color(Color::Error.color(cx))
                        .child(Label::new(message)),
                ),
                State::Ready {
                    rust_style_editor,
                    json_style_editor,
                    ..
                } => this
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .child(Label::new("Rust Style").size(LabelSize::Large))
                                    .child(
                                        IconButton::new("reset-style", IconName::Eraser)
                                            .tooltip(Tooltip::text("Reset style"))
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.reset_style(cx);
                                            })),
                                    ),
                            )
                            .child(div().h_64().child(rust_style_editor.clone())),
                    )
                    .child(
                        v_flex()
                            .gap_2()
                            .child(Label::new("JSON Style").size(LabelSize::Large))
                            .child(div().h_128().child(json_style_editor.clone()))
                            .when_some(self.json_style_error.as_ref(), |this, last_error| {
                                this.child(
                                    div()
                                        .w_full()
                                        .border_1()
                                        .border_color(Color::Error.color(cx))
                                        .child(Label::new(last_error)),
                                )
                            }),
                    ),
            })
            .into_any_element()
    }
}

fn render_layout_state(inspector_state: &DivInspectorState, cx: &App) -> Div {
    v_flex()
        .child(
            div()
                .text_ui(cx)
                .child(format!(
                    "Bounds: ⌜{} - {}⌟",
                    inspector_state.bounds.origin,
                    inspector_state.bounds.bottom_right()
                ))
                .child(format!("Size: {}", inspector_state.bounds.size)),
        )
        .child(
            div()
                .id("content-size")
                .text_ui(cx)
                .tooltip(Tooltip::text("Size of the element's children"))
                .child(
                    if inspector_state.content_size != inspector_state.bounds.size {
                        format!("Content size: {}", inspector_state.content_size)
                    } else {
                        "".to_string()
                    },
                ),
        )
}

pub(super) static STYLE_METHODS: LazyLock<
    Vec<(Box<StyleRefinement>, FunctionReflection<StyleRefinement>)>,
> = LazyLock::new(|| {
    // Include StyledExt methods first so that those methods take precedence.
    styled_ext_reflection::methods::<StyleRefinement>()
        .into_iter()
        .chain(styled_reflection::methods::<StyleRefinement>())
        .map(|method| (Box::new(method.invoke(StyleRefinement::default())), method))
        .collect()
});

pub(super) fn guess_rust_code_from_style(
    goal_style: &StyleRefinement,
) -> (String, StyleRefinement) {
    let mut subset_methods = Vec::new();
    for (style, method) in STYLE_METHODS.iter() {
        if goal_style.is_superset_of(style) {
            subset_methods.push(method);
        }
    }

    let mut code = "fn build() -> Div {\n    div()".to_string();
    let mut style = StyleRefinement::default();
    for method in subset_methods {
        let before_change = style.clone();
        style = method.invoke(style);
        if before_change != style {
            let _ = write!(code, "\n        .{}()", &method.name);
        }
    }
    code.push_str("\n}");

    (code, style)
}

pub(super) fn is_not_identifier_char(c: char) -> bool {
    !c.is_alphanumeric() && c != '_'
}

pub(super) struct RustStyleCompletionProvider {
    pub(super) div_inspector: Entity<DivInspector>,
}

impl CompletionProvider for RustStyleCompletionProvider {
    fn completions(
        &self,
        buffer: &Entity<Buffer>,
        position: Anchor,
        _: editor::CompletionContext,
        _window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Task<Result<Vec<CompletionResponse>>> {
        let Some(replace_range) = completion_replace_range(&buffer.read(cx).snapshot(), &position)
        else {
            return Task::ready(Ok(Vec::new()));
        };

        self.div_inspector.update(cx, |div_inspector, _cx| {
            div_inspector.rust_completion_replace_range = Some(replace_range.clone());
        });

        Task::ready(Ok(vec![CompletionResponse {
            completions: STYLE_METHODS
                .iter()
                .map(|(_, method)| Completion {
                    replace_range: replace_range.clone(),
                    new_text: format!(".{}()", method.name),
                    label: CodeLabel::plain(method.name.to_string(), None),
                    match_start: None,
                    snippet_deduplication_key: None,
                    icon_path: None,
                    icon_color: None,
                    documentation: method.documentation.map(|documentation| {
                        CompletionDocumentation::MultiLineMarkdown(documentation.into())
                    }),
                    source: CompletionSource::Custom,
                    insert_text_mode: None,
                    confirm: None,
                    group: None,
                })
                .collect(),
            display_options: CompletionDisplayOptions::default(),
            is_incomplete: false,
        }]))
    }

    fn is_completion_trigger(
        &self,
        buffer: &Entity<language::Buffer>,
        position: language::Anchor,
        _text: &str,
        _trigger_in_words: bool,
        cx: &mut Context<Editor>,
    ) -> bool {
        completion_replace_range(&buffer.read(cx).snapshot(), &position).is_some()
    }

    fn selection_changed(&self, mat: Option<&StringMatch>, _window: &mut Window, cx: &mut App) {
        let div_inspector = self.div_inspector.clone();
        let rust_completion = mat.as_ref().map(|mat| mat.string.clone());
        cx.defer(move |cx| {
            div_inspector.update(cx, |div_inspector, cx| {
                div_inspector.handle_rust_completion_selection_change(rust_completion, cx);
            });
        });
    }

    fn sort_completions(&self) -> bool {
        false
    }
}

fn completion_replace_range(snapshot: &BufferSnapshot, anchor: &Anchor) -> Option<Range<Anchor>> {
    let point = anchor.to_point(snapshot);
    let offset = point.to_offset(snapshot);
    let line_start = Point::new(point.row, 0).to_offset(snapshot);
    let line_end = Point::new(point.row, snapshot.line_len(point.row)).to_offset(snapshot);
    let mut lines = snapshot.text_for_range(line_start..line_end).lines();
    let line = lines.next()?;

    let start_in_line = &line[..offset - line_start]
        .rfind(|c| is_not_identifier_char(c) && c != '.')
        .map(|ix| ix + 1)
        .unwrap_or(0);
    let end_in_line = &line[offset - line_start..]
        .rfind(|c| is_not_identifier_char(c) && c != '(' && c != ')')
        .unwrap_or(line_end - line_start);

    if end_in_line > start_in_line {
        let replace_start = snapshot.anchor_before(line_start + start_in_line);
        let replace_end = snapshot.anchor_after(line_start + end_in_line);
        Some(replace_start..replace_end)
    } else {
        None
    }
}

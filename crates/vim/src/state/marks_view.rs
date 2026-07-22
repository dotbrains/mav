use super::*;

enum MarksMatchInfo {
    Path(Arc<Path>),
    Title(String),
    Content {
        line: String,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    },
}

impl MarksMatchInfo {
    fn from_chunks<'a>(chunks: impl Iterator<Item = Chunk<'a>>, cx: &App) -> Self {
        let mut line = String::new();
        let mut highlights = Vec::new();
        let mut offset = 0;
        for chunk in chunks {
            line.push_str(chunk.text);
            if let Some(highlight_id) = chunk.syntax_highlight_id
                && let Some(highlight) = cx.theme().syntax().get(highlight_id).cloned()
            {
                highlights.push((offset..offset + chunk.text.len(), highlight))
            }
            offset += chunk.text.len();
        }
        MarksMatchInfo::Content { line, highlights }
    }
}

struct MarksMatch {
    name: String,
    position: Point,
    info: MarksMatchInfo,
}

pub struct MarksViewDelegate {
    selected_index: usize,
    matches: Vec<MarksMatch>,
    point_column_width: usize,
    workspace: WeakEntity<Workspace>,
}

impl PickerDelegate for MarksViewDelegate {
    type ListItem = Div;

    fn name() -> &'static str {
        "marks view"
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(&mut self, ix: usize, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.selected_index = ix;
        cx.notify();
    }

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        Arc::default()
    }

    fn update_matches(
        &mut self,
        _: String,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> gpui::Task<()> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Task::ready(());
        };
        cx.spawn(async move |picker, cx| {
            let mut matches = Vec::new();
            let _ = workspace.update(cx, |workspace, cx| {
                let entity_id = cx.entity_id();
                let Some(editor) = workspace
                    .active_item(cx)
                    .and_then(|item| item.act_as::<Editor>(cx))
                else {
                    return;
                };
                let editor = editor.read(cx);
                let mut has_seen = HashSet::new();
                let Some(marks_state) = cx.global::<VimGlobals>().marks.get(&entity_id) else {
                    return;
                };
                let marks_state = marks_state.read(cx);

                if let Some(map) = marks_state
                    .multibuffer_marks
                    .get(&editor.buffer().entity_id())
                {
                    for (name, anchors) in map {
                        if has_seen.contains(name) {
                            continue;
                        }
                        has_seen.insert(name.clone());
                        let Some(anchor) = anchors.first() else {
                            continue;
                        };

                        let snapshot = editor.buffer().read(cx).snapshot(cx);
                        let position = anchor.to_point(&snapshot);

                        let chunks = snapshot.chunks(
                            Point::new(position.row, 0)
                                ..Point::new(
                                    position.row,
                                    snapshot.line_len(MultiBufferRow(position.row)),
                                ),
                            LanguageAwareStyling {
                                tree_sitter: true,
                                diagnostics: true,
                            },
                        );
                        matches.push(MarksMatch {
                            name: name.clone(),
                            position,
                            info: MarksMatchInfo::from_chunks(chunks, cx),
                        })
                    }
                }

                if let Some(buffer) = editor.buffer().read(cx).as_singleton() {
                    let buffer = buffer.read(cx);
                    if let Some(map) = marks_state.buffer_marks.get(&buffer.remote_id()) {
                        for (name, anchors) in map {
                            if has_seen.contains(name) {
                                continue;
                            }
                            has_seen.insert(name.clone());
                            let Some(anchor) = anchors.first() else {
                                continue;
                            };
                            let snapshot = buffer.snapshot();
                            let position = anchor.to_point(&snapshot);
                            let chunks = snapshot.chunks(
                                Point::new(position.row, 0)
                                    ..Point::new(position.row, snapshot.line_len(position.row)),
                                LanguageAwareStyling {
                                    tree_sitter: true,
                                    diagnostics: true,
                                },
                            );

                            matches.push(MarksMatch {
                                name: name.clone(),
                                position,
                                info: MarksMatchInfo::from_chunks(chunks, cx),
                            })
                        }
                    }
                }

                for (name, mark_location) in marks_state.global_marks.iter() {
                    if has_seen.contains(name) {
                        continue;
                    }
                    has_seen.insert(name.clone());

                    match mark_location {
                        MarkLocation::Buffer(entity_id) => {
                            if let Some(&anchor) = marks_state
                                .multibuffer_marks
                                .get(entity_id)
                                .and_then(|map| map.get(name))
                                .and_then(|anchors| anchors.first())
                            {
                                let Some((info, snapshot)) = workspace
                                    .items(cx)
                                    .filter_map(|item| item.act_as::<Editor>(cx))
                                    .map(|entity| entity.read(cx).buffer())
                                    .find(|buffer| buffer.entity_id().eq(entity_id))
                                    .map(|buffer| {
                                        (
                                            MarksMatchInfo::Title(
                                                buffer.read(cx).title(cx).to_string(),
                                            ),
                                            buffer.read(cx).snapshot(cx),
                                        )
                                    })
                                else {
                                    continue;
                                };
                                matches.push(MarksMatch {
                                    name: name.clone(),
                                    position: anchor.to_point(&snapshot),
                                    info,
                                });
                            }
                        }
                        MarkLocation::Path(path) => {
                            if let Some(&position) = marks_state
                                .serialized_marks
                                .get(path.as_ref())
                                .and_then(|map| map.get(name))
                                .and_then(|points| points.first())
                            {
                                let info = MarksMatchInfo::Path(path.clone());
                                matches.push(MarksMatch {
                                    name: name.clone(),
                                    position,
                                    info,
                                });
                            }
                        }
                    }
                }
            });
            let _ = picker.update(cx, |picker, cx| {
                matches.sort_by_key(|a| {
                    (
                        a.name.chars().next().map(|c| c.is_ascii_uppercase()),
                        a.name.clone(),
                    )
                });
                let digits = matches
                    .iter()
                    .map(|m| (m.position.row + 1).ilog10() + (m.position.column + 1).ilog10())
                    .max()
                    .unwrap_or_default();
                picker.delegate.matches = matches;
                picker.delegate.point_column_width = (digits + 4) as usize;
                cx.notify();
            });
        })
    }

    fn confirm(&mut self, _: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(vim) = self
            .workspace
            .upgrade()
            .map(|w| w.read(cx))
            .and_then(|w| w.focused_pane(window, cx).read(cx).active_item())
            .and_then(|item| item.act_as::<Editor>(cx))
            .and_then(|editor| editor.read(cx).addon::<VimAddon>().cloned())
            .map(|addon| addon.entity)
        else {
            return;
        };
        let Some(text): Option<Arc<str>> = self
            .matches
            .get(self.selected_index)
            .map(|m| Arc::from(m.name.to_string().into_boxed_str()))
        else {
            return;
        };
        vim.update(cx, |vim, cx| {
            vim.jump(text, false, false, window, cx);
        });

        cx.emit(DismissEvent);
    }

    fn dismissed(&mut self, _: &mut Window, _: &mut Context<Picker<Self>>) {}

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let mark_match = self.matches.get(ix)?;

        let mut left_output = String::new();
        let mut left_runs = Vec::new();
        left_output.push('`');
        left_output.push_str(&mark_match.name);
        left_runs.push((
            0..left_output.len(),
            HighlightStyle::color(cx.theme().colors().text_accent),
        ));
        left_output.push(' ');
        left_output.push(' ');
        let point_column = format!(
            "{},{}",
            mark_match.position.row + 1,
            mark_match.position.column + 1
        );
        left_output.push_str(&point_column);
        if let Some(padding) = self.point_column_width.checked_sub(point_column.len()) {
            left_output.push_str(&" ".repeat(padding));
        }

        let (right_output, right_runs): (String, Vec<_>) = match &mark_match.info {
            MarksMatchInfo::Path(path) => {
                let s = path.to_string_lossy().into_owned();
                (
                    s.clone(),
                    vec![(0..s.len(), HighlightStyle::color(cx.theme().colors().text))],
                )
            }
            MarksMatchInfo::Title(title) => (
                title.clone(),
                vec![(
                    0..title.len(),
                    HighlightStyle::color(cx.theme().colors().text),
                )],
            ),
            MarksMatchInfo::Content { line, highlights } => (line.clone(), highlights.clone()),
        };

        let theme = ThemeSettings::get_global(cx);
        let text_style = TextStyle {
            color: cx.theme().colors().editor_foreground,
            font_family: theme.buffer_font.family.clone(),
            font_features: theme.buffer_font.features.clone(),
            font_fallbacks: theme.buffer_font.fallbacks.clone(),
            font_size: theme.buffer_font_size(cx).into(),
            line_height: (theme.line_height() * theme.buffer_font_size(cx)).into(),
            font_weight: theme.buffer_font.weight,
            font_style: theme.buffer_font.style,
            ..Default::default()
        };

        Some(
            h_flex()
                .when(selected, |el| el.bg(cx.theme().colors().element_selected))
                .font_buffer(cx)
                .text_buffer(cx)
                .h(theme.buffer_font_size(cx) * theme.line_height())
                .px_2()
                .child(StyledText::new(left_output).with_default_highlights(&text_style, left_runs))
                .child(
                    StyledText::new(right_output).with_default_highlights(&text_style, right_runs),
                ),
        )
    }
}

pub struct MarksView {}

impl MarksView {
    fn register(workspace: &mut Workspace, _window: Option<&mut Window>) {
        workspace.register_action(|workspace, _: &ToggleMarksView, window, cx| {
            Self::toggle(workspace, window, cx);
        });
    }

    pub fn toggle(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
        let handle = cx.weak_entity();
        workspace.toggle_modal(window, cx, move |window, cx| {
            MarksView::new(handle, window, cx)
        });
    }

    fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Picker<MarksViewDelegate>>,
    ) -> Picker<MarksViewDelegate> {
        let matches = Vec::default();
        let delegate = MarksViewDelegate {
            selected_index: 0,
            point_column_width: 0,
            matches,
            workspace,
        };
        Picker::nonsearchable_uniform_list(delegate, window, cx).initial_width(rems(36.))
    }
}

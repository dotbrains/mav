use super::*;

#[derive(Clone)]
pub struct AvailableCodeAction {
    pub action: CodeAction,
    pub provider: Rc<dyn CodeActionProvider>,
}

#[derive(Clone)]
pub struct CodeActionContents {
    tasks: Option<Rc<ResolvedTasks>>,
    actions: Option<Rc<[AvailableCodeAction]>>,
    debug_scenarios: Vec<DebugScenario>,
    pub(crate) context: TaskContext,
}

impl CodeActionContents {
    pub(crate) fn new(
        tasks: Option<ResolvedTasks>,
        actions: Option<Rc<[AvailableCodeAction]>>,
        debug_scenarios: Vec<DebugScenario>,
        context: TaskContext,
    ) -> Self {
        Self {
            tasks: tasks.map(Rc::new),
            actions,
            debug_scenarios,
            context,
        }
    }

    pub fn tasks(&self) -> Option<&ResolvedTasks> {
        self.tasks.as_deref()
    }

    pub(crate) fn len(&self) -> usize {
        let tasks_len = self.tasks.as_ref().map_or(0, |tasks| tasks.templates.len());
        let code_actions_len = self.actions.as_ref().map_or(0, |actions| actions.len());
        tasks_len + code_actions_len + self.debug_scenarios.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = CodeActionsItem> + '_ {
        self.tasks
            .iter()
            .flat_map(|tasks| {
                tasks
                    .templates
                    .iter()
                    .map(|(kind, task)| CodeActionsItem::Task(kind.clone(), task.clone()))
            })
            .chain(self.actions.iter().flat_map(|actions| {
                actions.iter().map(|available| CodeActionsItem::CodeAction {
                    action: available.action.clone(),
                    provider: available.provider.clone(),
                })
            }))
            .chain(
                self.debug_scenarios
                    .iter()
                    .cloned()
                    .map(CodeActionsItem::DebugScenario),
            )
    }

    pub fn get(&self, mut index: usize) -> Option<CodeActionsItem> {
        if let Some(tasks) = &self.tasks {
            if let Some((kind, task)) = tasks.templates.get(index) {
                return Some(CodeActionsItem::Task(kind.clone(), task.clone()));
            } else {
                index -= tasks.templates.len();
            }
        }
        if let Some(actions) = &self.actions {
            if let Some(available) = actions.get(index) {
                return Some(CodeActionsItem::CodeAction {
                    action: available.action.clone(),
                    provider: available.provider.clone(),
                });
            } else {
                index -= actions.len();
            }
        }

        self.debug_scenarios
            .get(index)
            .cloned()
            .map(CodeActionsItem::DebugScenario)
    }
}

#[derive(Clone)]
pub enum CodeActionsItem {
    Task(TaskSourceKind, ResolvedTask),
    CodeAction {
        action: CodeAction,
        provider: Rc<dyn CodeActionProvider>,
    },
    DebugScenario(DebugScenario),
}

impl CodeActionsItem {
    pub fn label(&self) -> String {
        match self {
            Self::CodeAction { action, .. } => action.lsp_action.title().to_owned(),
            Self::Task(_, task) => task.resolved_label.clone(),
            Self::DebugScenario(scenario) => scenario.label.to_string(),
        }
    }

    pub fn menu_label(&self) -> String {
        match self {
            Self::CodeAction { action, .. } => action.lsp_action.title().replace("\n", ""),
            Self::Task(_, task) => task.resolved_label.replace("\n", ""),
            Self::DebugScenario(scenario) => format!("debug: {}", scenario.label),
        }
    }
}

pub struct CodeActionsMenu {
    pub actions: CodeActionContents,
    pub buffer: Entity<Buffer>,
    pub selected_item: usize,
    pub scroll_handle: UniformListScrollHandle,
    pub deployed_from: Option<CodeActionSource>,
}

impl CodeActionsMenu {
    pub(crate) fn select_first(&mut self, cx: &mut Context<Editor>) {
        self.selected_item = if self.scroll_handle.y_flipped() {
            self.actions.len() - 1
        } else {
            0
        };
        self.scroll_handle
            .scroll_to_item(self.selected_item, ScrollStrategy::Top);
        cx.notify()
    }

    pub(crate) fn select_last(&mut self, cx: &mut Context<Editor>) {
        self.selected_item = if self.scroll_handle.y_flipped() {
            0
        } else {
            self.actions.len() - 1
        };
        self.scroll_handle
            .scroll_to_item(self.selected_item, ScrollStrategy::Top);
        cx.notify()
    }

    pub(crate) fn select_prev(&mut self, cx: &mut Context<Editor>) {
        self.selected_item = if self.scroll_handle.y_flipped() {
            self.next_match_index()
        } else {
            self.prev_match_index()
        };
        self.scroll_handle
            .scroll_to_item(self.selected_item, ScrollStrategy::Top);
        cx.notify();
    }

    pub(crate) fn select_next(&mut self, cx: &mut Context<Editor>) {
        self.selected_item = if self.scroll_handle.y_flipped() {
            self.prev_match_index()
        } else {
            self.next_match_index()
        };
        self.scroll_handle
            .scroll_to_item(self.selected_item, ScrollStrategy::Top);
        cx.notify();
    }

    pub(crate) fn prev_match_index(&self) -> usize {
        if self.selected_item > 0 {
            self.selected_item - 1
        } else {
            self.actions.len() - 1
        }
    }

    pub(crate) fn next_match_index(&self) -> usize {
        if self.selected_item + 1 < self.actions.len() {
            self.selected_item + 1
        } else {
            0
        }
    }

    pub fn visible(&self) -> bool {
        !self.actions.is_empty()
    }

    pub(crate) fn origin(&self) -> ContextMenuOrigin {
        match &self.deployed_from {
            Some(CodeActionSource::Indicator(row)) | Some(CodeActionSource::RunMenu(row)) => {
                ContextMenuOrigin::GutterIndicator(*row)
            }
            Some(CodeActionSource::QuickActionBar) => ContextMenuOrigin::QuickActionBar,
            None => ContextMenuOrigin::Cursor,
        }
    }

    pub(crate) fn render(
        &self,
        _style: &EditorStyle,
        max_height_in_lines: u32,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> AnyElement {
        let actions = self.actions.clone();
        let selected_item = self.selected_item;
        let is_quick_action_bar = matches!(self.origin(), ContextMenuOrigin::QuickActionBar);

        let list = uniform_list(
            "code_actions_menu",
            self.actions.len(),
            cx.processor(move |_this, range: Range<usize>, _, cx| {
                actions
                    .iter()
                    .skip(range.start)
                    .take(range.end - range.start)
                    .enumerate()
                    .map(|(ix, action)| {
                        let item_ix = range.start + ix;
                        let selected = item_ix == selected_item;
                        let colors = cx.theme().colors();

                        ListItem::new(item_ix)
                            .inset(true)
                            .toggle_state(selected)
                            .overflow_x()
                            .child(
                                div()
                                    .min_w(CODE_ACTION_MENU_MIN_WIDTH)
                                    .max_w(CODE_ACTION_MENU_MAX_WIDTH)
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .when(is_quick_action_bar, |this| this.text_ui(cx))
                                    .when(selected, |this| this.text_color(colors.text_accent))
                                    .child(action.menu_label()),
                            )
                            .on_click(cx.listener(move |editor, _, window, cx| {
                                cx.stop_propagation();
                                if let Some(task) = editor.confirm_code_action(
                                    &ConfirmCodeAction {
                                        item_ix: Some(item_ix),
                                    },
                                    window,
                                    cx,
                                ) {
                                    task.detach_and_log_err(cx)
                                }
                            }))
                    })
                    .collect()
            }),
        )
        .occlude()
        .max_h(max_height_in_lines as f32 * window.line_height())
        .track_scroll(&self.scroll_handle)
        .with_width_from_item(
            self.actions
                .iter()
                .enumerate()
                .max_by_key(|(_, action)| match action {
                    CodeActionsItem::Task(_, task) => task.resolved_label.chars().count(),
                    CodeActionsItem::CodeAction { action, .. } => {
                        action.lsp_action.title().chars().count()
                    }
                    CodeActionsItem::DebugScenario(scenario) => {
                        format!("debug: {}", scenario.label).chars().count()
                    }
                })
                .map(|(ix, _)| ix),
        )
        .with_sizing_behavior(ListSizingBehavior::Infer);

        Popover::new().child(list).into_any_element()
    }

    pub(crate) fn render_aside(
        &mut self,
        max_size: Size<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Editor>,
    ) -> Option<AnyElement> {
        let Some(action) = self.actions.get(self.selected_item) else {
            return None;
        };

        let label = action.menu_label();
        let text_system = window.text_system();
        let mut line_wrapper = text_system.line_wrapper(
            window.text_style().font(),
            window.text_style().font_size.to_pixels(window.rem_size()),
        );
        let is_truncated = line_wrapper.should_truncate_line(
            &label,
            CODE_ACTION_MENU_MAX_WIDTH,
            "…",
            gpui::TruncateFrom::End,
        );

        if is_truncated.is_none() {
            return None;
        }

        Some(
            Popover::new()
                .child(
                    div()
                        .child(label)
                        .id("code_actions_menu_extended")
                        .px(MENU_ASIDE_X_PADDING / 2.)
                        .max_w(max_size.width)
                        .max_h(max_size.height)
                        .occlude(),
                )
                .into_any_element(),
        )
    }
}

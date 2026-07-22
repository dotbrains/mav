use super::*;

impl VariableList {
    pub(super) fn build_entries(&mut self, cx: &mut Context<Self>) {
        let Some(stack_frame_id) = self.selected_stack_frame_id else {
            return;
        };

        let mut entries = vec![];

        let scopes: Vec<_> = self.session.update(cx, |session, cx| {
            session.scopes(stack_frame_id, cx).to_vec()
        });

        let mut contains_local_scope = false;

        let mut stack = scopes
            .into_iter()
            .rev()
            .filter(|scope| {
                if scope
                    .presentation_hint
                    .as_ref()
                    .map(|hint| *hint == ScopePresentationHint::Locals)
                    .unwrap_or(scope.name.to_lowercase().starts_with("local"))
                {
                    contains_local_scope = true;
                }

                self.session.update(cx, |session, cx| {
                    !session.variables(scope.variables_reference, cx).is_empty()
                })
            })
            .map(|scope| {
                (
                    scope.variables_reference,
                    scope.variables_reference,
                    EntryPath::for_scope(&scope.name),
                    DapEntry::Scope(scope),
                )
            })
            .collect::<Vec<_>>();

        let watches = self.session.read(cx).watchers().clone();
        stack.extend(
            watches
                .into_values()
                .map(|watcher| {
                    (
                        watcher.variables_reference,
                        watcher.variables_reference,
                        EntryPath::for_watcher(watcher.expression.clone()),
                        DapEntry::Watcher(watcher),
                    )
                })
                .collect::<Vec<_>>(),
        );

        let scopes_count = stack.len();

        while let Some((container_reference, variables_reference, mut path, dap_kind)) = stack.pop()
        {
            match &dap_kind {
                DapEntry::Watcher(watcher) => path = path.with_child(watcher.expression.clone()),
                DapEntry::Variable(dap) => path = path.with_name(dap.name.clone().into()),
                DapEntry::Scope(dap) => path = path.with_child(dap.name.clone().into()),
            }

            let var_state = self
                .entry_states
                .entry(path.clone())
                .and_modify(|state| {
                    state.parent_reference = container_reference;
                    state.has_children = variables_reference != 0;
                })
                .or_insert(EntryState {
                    depth: path.indices.len(),
                    is_expanded: dap_kind.as_scope().is_some_and(|scope| {
                        (scopes_count == 1 && !contains_local_scope)
                            || scope
                                .presentation_hint
                                .as_ref()
                                .map(|hint| *hint == ScopePresentationHint::Locals)
                                .unwrap_or(scope.name.to_lowercase().starts_with("local"))
                    }),
                    parent_reference: container_reference,
                    has_children: variables_reference != 0,
                });

            entries.push(ListEntry {
                entry: dap_kind,
                path: path.clone(),
            });

            if var_state.is_expanded {
                let children = self
                    .session
                    .update(cx, |session, cx| session.variables(variables_reference, cx));
                stack.extend(children.into_iter().rev().map(|child| {
                    (
                        variables_reference,
                        child.variables_reference,
                        path.with_child(child.name.clone().into()),
                        DapEntry::Variable(child),
                    )
                }));
            }
        }

        self.entries = entries;

        let text_pixels = ui::TextSize::Default.pixels(cx).to_f64() as f32;
        let indent_size = INDENT_STEP_SIZE.to_f64() as f32;

        self.max_width_index = self
            .entries
            .iter()
            .map(|entry| match &entry.entry {
                DapEntry::Scope(scope) => scope.name.len() as f32 * text_pixels,
                DapEntry::Variable(variable) => {
                    (variable.value.len() + variable.name.len()) as f32 * text_pixels
                        + (entry.path.indices.len() as f32 * indent_size)
                }
                DapEntry::Watcher(watcher) => {
                    (watcher.value.len() + watcher.expression.len()) as f32 * text_pixels
                        + (entry.path.indices.len() as f32 * indent_size)
                }
            })
            .position_max_by(|left, right| left.total_cmp(right));

        cx.notify();
    }

    fn handle_stack_frame_list_events(
        &mut self,
        _: Entity<StackFrameList>,
        event: &StackFrameListEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            StackFrameListEvent::SelectedStackFrameChanged(stack_frame_id) => {
                self.selected_stack_frame_id = Some(*stack_frame_id);
                self.session.update(cx, |session, cx| {
                    session.refresh_watchers(*stack_frame_id, cx);
                });
                self.build_entries(cx);
            }
            StackFrameListEvent::BuiltEntries => {}
        }
    }

    pub fn completion_variables(&self, _cx: &mut Context<Self>) -> Vec<dap::Variable> {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.entry {
                DapEntry::Variable(dap) => Some(dap.clone()),
                DapEntry::Scope(_) | DapEntry::Watcher { .. } => None,
            })
            .collect()
    }
}

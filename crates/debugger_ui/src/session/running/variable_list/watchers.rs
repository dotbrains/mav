use super::*;

impl VariableList {
    pub(super) fn copy_variable_name(
        &mut self,
        _: &CopyVariableName,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.selection.as_ref() else {
            return;
        };

        let Some(entry) = self.entries.iter().find(|entry| &entry.path == selection) else {
            return;
        };

        let variable_name = match &entry.entry {
            DapEntry::Variable(dap) => dap.name.clone(),
            DapEntry::Watcher(watcher) => watcher.expression.to_string(),
            DapEntry::Scope(_) => return,
        };

        cx.write_to_clipboard(ClipboardItem::new_string(variable_name));
    }

    pub(super) fn copy_variable_value(
        &mut self,
        _: &CopyVariableValue,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.selection.as_ref() else {
            return;
        };

        let Some(entry) = self.entries.iter().find(|entry| &entry.path == selection) else {
            return;
        };

        let variable_value = match &entry.entry {
            DapEntry::Variable(dap) => dap.value.clone(),
            DapEntry::Watcher(watcher) => watcher.value.to_string(),
            DapEntry::Scope(_) => return,
        };

        cx.write_to_clipboard(ClipboardItem::new_string(variable_value));
    }

    pub(super) fn edit_variable(
        &mut self,
        _: &EditVariable,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.selection.as_ref() else {
            return;
        };

        let Some(entry) = self.entries.iter().find(|entry| &entry.path == selection) else {
            return;
        };

        let variable_value = match &entry.entry {
            DapEntry::Watcher(watcher) => watcher.value.to_string(),
            DapEntry::Variable(variable) => variable.value.clone(),
            DapEntry::Scope(_) => return,
        };

        let editor = Self::create_variable_editor(&variable_value, window, cx);
        self.edited_path = Some((entry.path.clone(), editor));

        cx.notify();
    }

    pub(super) fn add_watcher(&mut self, _: &AddWatch, _: &mut Window, cx: &mut Context<Self>) {
        let Some(selection) = self.selection.as_ref() else {
            return;
        };

        let Some(entry) = self.entries.iter().find(|entry| &entry.path == selection) else {
            return;
        };

        let Some(variable) = entry.as_variable() else {
            return;
        };

        let Some(stack_frame_id) = self.selected_stack_frame_id else {
            return;
        };

        let add_watcher_task = self.session.update(cx, |session, cx| {
            let expression = variable
                .evaluate_name
                .clone()
                .unwrap_or_else(|| variable.name.clone());

            session.add_watcher(expression.into(), stack_frame_id, cx)
        });

        cx.spawn(async move |this, cx| {
            add_watcher_task.await?;

            this.update(cx, |this, cx| {
                this.build_entries(cx);
            })
        })
        .detach_and_log_err(cx);
    }

    pub(super) fn remove_watcher(
        &mut self,
        _: &RemoveWatch,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.selection.as_ref() else {
            return;
        };

        let Some(entry) = self.entries.iter().find(|entry| &entry.path == selection) else {
            return;
        };

        let Some(watcher) = entry.as_watcher() else {
            return;
        };

        self.session.update(cx, |session, _| {
            session.remove_watcher(watcher.expression.clone());
        });
        self.build_entries(cx);
    }

    #[track_caller]
    #[cfg(test)]
    pub(crate) fn assert_visual_entries(&self, expected: Vec<&str>) {
        const INDENT: &str = "    ";

        let entries = &self.entries;
        let mut visual_entries = Vec::with_capacity(entries.len());
        for entry in entries {
            let state = self
                .entry_states
                .get(&entry.path)
                .expect("If there's a variable entry there has to be a state that goes with it");

            visual_entries.push(format!(
                "{}{} {}{}",
                INDENT.repeat(state.depth - 1),
                if state.is_expanded { "v" } else { ">" },
                entry.entry.name(),
                if self.selection.as_ref() == Some(&entry.path) {
                    " <=== selected"
                } else {
                    ""
                }
            ));
        }

        pretty_assertions::assert_eq!(expected, visual_entries);
    }

    #[track_caller]
    #[cfg(test)]
    pub(crate) fn scopes(&self) -> Vec<dap::Scope> {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.entry {
                DapEntry::Scope(scope) => Some(scope),
                _ => None,
            })
            .cloned()
            .collect()
    }

    #[track_caller]
    #[cfg(test)]
    pub(crate) fn variables_per_scope(&self) -> Vec<(dap::Scope, Vec<dap::Variable>)> {
        let mut scopes: Vec<(dap::Scope, Vec<_>)> = Vec::new();
        let mut idx = 0;

        for entry in self.entries.iter() {
            match &entry.entry {
                DapEntry::Watcher { .. } => continue,
                DapEntry::Variable(dap) => scopes[idx].1.push(dap.clone()),
                DapEntry::Scope(scope) => {
                    if !scopes.is_empty() {
                        idx += 1;
                    }

                    scopes.push((scope.clone(), Vec::new()));
                }
            }
        }

        scopes
    }

    #[track_caller]
    #[cfg(test)]
    pub(crate) fn variables(&self) -> Vec<dap::Variable> {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.entry {
                DapEntry::Variable(variable) => Some(variable),
                _ => None,
            })
            .cloned()
            .collect()
    }
}

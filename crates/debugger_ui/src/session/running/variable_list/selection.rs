use super::*;

impl VariableList {
    pub(crate) fn toggle_entry(&mut self, var_path: &EntryPath, cx: &mut Context<Self>) {
        let Some(entry) = self.entry_states.get_mut(var_path) else {
            log::error!("Could not find variable list entry state to toggle");
            return;
        };

        entry.is_expanded = !entry.is_expanded;
        self.build_entries(cx);
    }

    pub(super) fn select_first(
        &mut self,
        _: &SelectFirst,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel(&Default::default(), window, cx);
        if let Some(variable) = self.entries.first() {
            self.selection = Some(variable.path.clone());
            self.build_entries(cx);
        }
    }

    pub(super) fn select_last(
        &mut self,
        _: &SelectLast,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel(&Default::default(), window, cx);
        if let Some(variable) = self.entries.last() {
            self.selection = Some(variable.path.clone());
            self.build_entries(cx);
        }
    }

    pub(super) fn select_prev(
        &mut self,
        _: &SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel(&Default::default(), window, cx);
        if let Some(selection) = &self.selection {
            let index = self.entries.iter().enumerate().find_map(|(ix, var)| {
                if &var.path == selection && ix > 0 {
                    Some(ix.saturating_sub(1))
                } else {
                    None
                }
            });

            if let Some(new_selection) =
                index.and_then(|ix| self.entries.get(ix).map(|var| var.path.clone()))
            {
                self.selection = Some(new_selection);
                self.build_entries(cx);
            } else {
                self.select_last(&SelectLast, window, cx);
            }
        } else {
            self.select_last(&SelectLast, window, cx);
        }
    }

    pub(super) fn select_next(
        &mut self,
        _: &SelectNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel(&Default::default(), window, cx);
        if let Some(selection) = &self.selection {
            let index = self.entries.iter().enumerate().find_map(|(ix, var)| {
                if &var.path == selection {
                    Some(ix.saturating_add(1))
                } else {
                    None
                }
            });

            if let Some(new_selection) =
                index.and_then(|ix| self.entries.get(ix).map(|var| var.path.clone()))
            {
                self.selection = Some(new_selection);
                self.build_entries(cx);
            } else {
                self.select_first(&SelectFirst, window, cx);
            }
        } else {
            self.select_first(&SelectFirst, window, cx);
        }
    }

    pub(super) fn cancel(&mut self, _: &menu::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.edited_path.take();
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(super) fn confirm(
        &mut self,
        _: &menu::Confirm,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((var_path, editor)) = self.edited_path.take() {
            let Some(state) = self.entry_states.get(&var_path) else {
                return;
            };

            let variables_reference = state.parent_reference;
            let Some(name) = var_path.leaf_name else {
                return;
            };

            let Some(stack_frame_id) = self.selected_stack_frame_id else {
                return;
            };

            let value = editor.read(cx).text(cx);

            self.session.update(cx, |session, cx| {
                session.set_variable_value(
                    stack_frame_id,
                    variables_reference,
                    name.into(),
                    value,
                    cx,
                )
            });
        }
    }

    pub(super) fn collapse_selected_entry(
        &mut self,
        _: &CollapseSelectedEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ref selected_entry) = self.selection {
            let Some(entry_state) = self.entry_states.get_mut(selected_entry) else {
                debug_panic!("Trying to toggle variable in variable list that has an no state");
                return;
            };

            if !entry_state.is_expanded || !entry_state.has_children {
                self.select_prev(&SelectPrevious, window, cx);
            } else {
                entry_state.is_expanded = false;
                self.build_entries(cx);
            }
        }
    }

    pub(super) fn expand_selected_entry(
        &mut self,
        _: &ExpandSelectedEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(selected_entry) = &self.selection {
            let Some(entry_state) = self.entry_states.get_mut(selected_entry) else {
                debug_panic!("Trying to toggle variable in variable list that has an no state");
                return;
            };

            if entry_state.is_expanded || !entry_state.has_children {
                self.select_next(&SelectNext, window, cx);
            } else {
                entry_state.is_expanded = true;
                self.build_entries(cx);
            }
        }
    }
}

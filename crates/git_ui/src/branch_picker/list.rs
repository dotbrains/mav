use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum BranchListStyle {
    Modal,
    Popover,
}

pub struct BranchList {
    pub picker: Entity<Picker<BranchListDelegate>>,
    picker_focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
    embedded: bool,
}

impl BranchList {
    fn new(
        workspace: WeakEntity<Workspace>,
        repository: Option<Entity<Repository>>,
        style: BranchListStyle,
        width: Rems,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::new_inner(workspace, repository, style, width, false, window, cx);
        this._subscriptions
            .push(cx.subscribe(&this.picker, |_, _, _, cx| {
                cx.emit(DismissEvent);
            }));
        this
    }

    fn new_inner(
        workspace: WeakEntity<Workspace>,
        repository: Option<Entity<Repository>>,
        style: BranchListStyle,
        width: Rems,
        embedded: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_inner_with_behavior(
            workspace,
            repository,
            style,
            width,
            embedded,
            BranchSelectionBehavior::Checkout,
            window,
            cx,
        )
    }

    fn new_select(
        workspace: WeakEntity<Workspace>,
        repository: Option<Entity<Repository>>,
        style: BranchListStyle,
        width: Rems,
        selected_branch: Option<SharedString>,
        on_select: SelectBranchCallback,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::new_inner_with_behavior(
            workspace,
            repository,
            style,
            width,
            false,
            BranchSelectionBehavior::Select {
                selected_branch,
                on_select,
            },
            window,
            cx,
        );
        this._subscriptions
            .push(cx.subscribe(&this.picker, |_, _, _, cx| {
                cx.emit(DismissEvent);
            }));
        this
    }

    fn new_inner_with_behavior(
        workspace: WeakEntity<Workspace>,
        repository: Option<Entity<Repository>>,
        style: BranchListStyle,
        width: Rems,
        embedded: bool,
        branch_selection_behavior: BranchSelectionBehavior,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let all_branches = repository
            .as_ref()
            .map(|repo| {
                process_branches(
                    &repo.read(cx).branch_list,
                    branch_selection_behavior.selected_branch(),
                )
            })
            .unwrap_or_default();
        let branch_list_error = repository
            .as_ref()
            .and_then(|repo| repo.read(cx).branch_list_error.clone());

        let default_branch_request = repository.clone().map(|repository| {
            repository.update(cx, |repository, _| repository.default_branch(false))
        });

        let mut delegate = BranchListDelegate::new(
            workspace,
            repository.clone(),
            style,
            branch_selection_behavior,
            cx,
        );
        delegate.all_branches = all_branches;
        delegate.branch_list_error = branch_list_error;

        let picker = cx.new(|cx| {
            Picker::uniform_list(delegate, window, cx)
                .initial_width(width)
                .show_scrollbar(true)
                .when(embedded, |picker| picker.embedded())
        });
        let picker_focus_handle = picker.focus_handle(cx);

        picker.update(cx, |picker, _| {
            picker.delegate.focus_handle = picker_focus_handle.clone();
            picker.delegate.show_footer = !embedded && !picker.delegate.is_select_only();
        });

        let mut subscriptions = Vec::new();

        if let Some(repo) = &repository {
            subscriptions.push(cx.subscribe_in(
                repo,
                window,
                move |this, repo, event, window, cx| {
                    if matches!(event, RepositoryEvent::BranchListChanged) {
                        let snapshot = repo.read(cx);
                        let branch_list = snapshot.branch_list.clone();
                        let branch_list_error = snapshot.branch_list_error.clone();
                        this.picker.update(cx, |picker, cx| {
                            picker.delegate.restore_selected_branch = picker
                                .delegate
                                .matches
                                .get(picker.delegate.selected_index)
                                .and_then(|entry| entry.as_branch().map(|b| b.ref_name.clone()));
                            picker.delegate.all_branches = process_branches(
                                &branch_list,
                                picker.delegate.branch_selection_behavior.selected_branch(),
                            );
                            picker.delegate.branch_list_error = branch_list_error;
                            picker.refresh(window, cx);
                        });
                    }
                },
            ));
        }

        // Fetch default branch asynchronously since it requires a git operation
        cx.spawn_in(window, async move |this, cx| {
            let default_branch = default_branch_request
                .context("No active repository")?
                .await
                .map(Result::ok)
                .ok()
                .flatten()
                .flatten();

            let _ = this.update_in(cx, |this, _window, cx| {
                this.picker.update(cx, |picker, _cx| {
                    picker.delegate.default_branch = default_branch;
                });
            });

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);

        Self {
            picker,
            picker_focus_handle,
            _subscriptions: subscriptions,
            embedded,
        }
    }

    fn new_embedded(
        workspace: WeakEntity<Workspace>,
        repository: Option<Entity<Repository>>,
        width: Rems,
        show_footer: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::new_inner(
            workspace,
            repository,
            BranchListStyle::Modal,
            width,
            true,
            window,
            cx,
        );
        this.picker.update(cx, |picker, _| {
            picker.delegate.show_footer = show_footer;
        });
        this._subscriptions
            .push(cx.subscribe(&this.picker, |_, _, _, cx| {
                cx.emit(DismissEvent);
            }));
        this
    }

    pub fn handle_modifiers_changed(
        &mut self,
        ev: &ModifiersChangedEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            picker.delegate.modifiers = ev.modifiers;
            cx.notify();
        })
    }

    pub fn handle_delete(
        &mut self,
        _: &branch_picker::DeleteBranch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            if picker.delegate.is_select_only() {
                return;
            }
            picker
                .delegate
                .delete_at(picker.delegate.selected_index, false, window, cx)
        })
    }

    pub fn handle_force_delete(
        &mut self,
        _: &branch_picker::ForceDeleteBranch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            if picker.delegate.is_select_only() {
                return;
            }
            picker
                .delegate
                .delete_at(picker.delegate.selected_index, true, window, cx)
        })
    }

    pub fn handle_filter(
        &mut self,
        _: &branch_picker::FilterRemotes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            picker.delegate.branch_filter = picker.delegate.branch_filter.invert();
            picker.update_matches(picker.query(cx), window, cx);
            picker.refresh_placeholder(window, cx);
            cx.notify();
        });
    }
}
impl ModalView for BranchList {}
impl EventEmitter<DismissEvent> for BranchList {}

impl Focusable for BranchList {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.picker_focus_handle.clone()
    }
}

impl Render for BranchList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("GitBranchSelector")
            .on_modifiers_changed(cx.listener(Self::handle_modifiers_changed))
            .on_action(cx.listener(Self::handle_delete))
            .on_action(cx.listener(Self::handle_force_delete))
            .on_action(cx.listener(Self::handle_filter))
            .child(self.picker.clone())
            .when(!self.embedded, |this| {
                this.on_mouse_down_out({
                    cx.listener(move |this, _, window, cx| {
                        this.picker.update(cx, |this, cx| {
                            this.cancel(&Default::default(), window, cx);
                        })
                    })
                })
            })
    }
}

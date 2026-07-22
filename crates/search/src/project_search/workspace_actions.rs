use super::*;

pub fn init(cx: &mut App) {
    cx.set_global(ActiveSettings::default());
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        register_workspace_action(workspace, move |search_bar, _: &Deploy, window, cx| {
            search_bar.focus_search(window, cx);
        });
        register_workspace_action(workspace, move |search_bar, _: &FocusSearch, window, cx| {
            search_bar.focus_search(window, cx);
        });
        register_workspace_action(
            workspace,
            move |search_bar, _: &ToggleFilters, window, cx| {
                search_bar.toggle_filters(window, cx);
            },
        );
        register_workspace_action(
            workspace,
            move |search_bar, _: &ToggleCaseSensitive, window, cx| {
                search_bar.toggle_search_option(SearchOptions::CASE_SENSITIVE, window, cx);
            },
        );
        register_workspace_action(
            workspace,
            move |search_bar, _: &ToggleWholeWord, window, cx| {
                search_bar.toggle_search_option(SearchOptions::WHOLE_WORD, window, cx);
            },
        );
        register_workspace_action(workspace, move |search_bar, _: &ToggleRegex, window, cx| {
            search_bar.toggle_search_option(SearchOptions::REGEX, window, cx);
        });
        register_workspace_action(
            workspace,
            move |search_bar, action: &ToggleReplace, window, cx| {
                search_bar.toggle_replace(action, window, cx)
            },
        );
        register_workspace_action(
            workspace,
            move |search_bar, action: &SelectPreviousMatch, window, cx| {
                search_bar.select_prev_match(action, window, cx)
            },
        );
        register_workspace_action(
            workspace,
            move |search_bar, action: &SelectNextMatch, window, cx| {
                search_bar.select_next_match(action, window, cx)
            },
        );

        // Only handle search_in_new if there is a search present
        register_workspace_action_for_present_search(workspace, |workspace, action, window, cx| {
            ProjectSearchView::search_in_new(workspace, action, window, cx)
        });

        register_workspace_action_for_present_search(
            workspace,
            |workspace, action: &ToggleAllSearchResults, window, cx| {
                if let Some(search_view) = workspace
                    .active_item(cx)
                    .and_then(|item| item.downcast::<ProjectSearchView>())
                {
                    search_view.update(cx, |search_view, cx| {
                        search_view.toggle_all_search_results(action, window, cx);
                    });
                }
            },
        );

        register_workspace_action_for_present_search(
            workspace,
            |workspace, _: &menu::Cancel, window, cx| {
                if let Some(project_search_bar) = workspace
                    .active_pane()
                    .read(cx)
                    .toolbar()
                    .read(cx)
                    .item_of_type::<ProjectSearchBar>()
                {
                    project_search_bar.update(cx, |project_search_bar, cx| {
                        let search_is_focused = project_search_bar
                            .active_project_search
                            .as_ref()
                            .is_some_and(|search_view| {
                                search_view
                                    .read(cx)
                                    .query_editor
                                    .read(cx)
                                    .focus_handle(cx)
                                    .is_focused(window)
                            });
                        if search_is_focused {
                            project_search_bar.move_focus_to_results(window, cx);
                        } else {
                            project_search_bar.focus_search(window, cx)
                        }
                    });
                } else {
                    cx.propagate();
                }
            },
        );

        // Both on present and dismissed search, we need to unconditionally handle those actions to focus from the editor.
        workspace.register_action(move |workspace, action: &DeploySearch, window, cx| {
            if workspace.has_active_modal(window, cx) && !workspace.hide_modal(window, cx) {
                cx.propagate();
                return;
            }
            ProjectSearchView::deploy_search(workspace, action, window, cx);
            cx.notify();
        });
        workspace.register_action(move |workspace, action: &NewSearch, window, cx| {
            if workspace.has_active_modal(window, cx) && !workspace.hide_modal(window, cx) {
                cx.propagate();
                return;
            }
            ProjectSearchView::new_search(workspace, action, window, cx);
            cx.notify();
        });
    })
    .detach();
}

fn register_workspace_action<A: Action>(
    workspace: &mut Workspace,
    callback: fn(&mut ProjectSearchBar, &A, &mut Window, &mut Context<ProjectSearchBar>),
) {
    workspace.register_action(move |workspace, action: &A, window, cx| {
        if workspace.has_active_modal(window, cx) && !workspace.hide_modal(window, cx) {
            cx.propagate();
            return;
        }

        workspace.active_pane().update(cx, |pane, cx| {
            pane.toolbar().update(cx, move |workspace, cx| {
                if let Some(search_bar) = workspace.item_of_type::<ProjectSearchBar>() {
                    search_bar.update(cx, move |search_bar, cx| {
                        if search_bar.active_project_search.is_some() {
                            callback(search_bar, action, window, cx);
                            cx.notify();
                        } else {
                            cx.propagate();
                        }
                    });
                }
            });
        })
    });
}

fn register_workspace_action_for_present_search<A: Action>(
    workspace: &mut Workspace,
    callback: fn(&mut Workspace, &A, &mut Window, &mut Context<Workspace>),
) {
    workspace.register_action(move |workspace, action: &A, window, cx| {
        if workspace.has_active_modal(window, cx) && !workspace.hide_modal(window, cx) {
            cx.propagate();
            return;
        }

        let should_notify = workspace
            .active_pane()
            .read(cx)
            .toolbar()
            .read(cx)
            .item_of_type::<ProjectSearchBar>()
            .map(|search_bar| search_bar.read(cx).active_project_search.is_some())
            .unwrap_or(false);
        if should_notify {
            callback(workspace, action, window, cx);
            cx.notify();
        } else {
            cx.propagate();
        }
    });
}

#[cfg(any(test, feature = "test-support"))]
pub fn perform_project_search(
    search_view: &Entity<ProjectSearchView>,
    text: impl Into<std::sync::Arc<str>>,
    cx: &mut gpui::VisualTestContext,
) {
    cx.run_until_parked();
    search_view.update_in(cx, |search_view, window, cx| {
        search_view.query_editor.update(cx, |query_editor, cx| {
            query_editor.set_text(text, window, cx)
        });
        search_view.search(cx);
    });
    cx.run_until_parked();
}

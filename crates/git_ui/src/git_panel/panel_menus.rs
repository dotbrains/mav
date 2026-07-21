use super::*;

pub(super) fn prompt<T>(
    msg: &str,
    detail: Option<&str>,
    window: &mut Window,
    cx: &mut App,
) -> Task<anyhow::Result<T>>
where
    T: IntoEnumIterator + VariantNames + 'static,
{
    let rx = window.prompt(PromptLevel::Info, msg, detail, T::VARIANTS, cx);
    cx.spawn(async move |_| Ok(T::iter().nth(rx.await?).unwrap()))
}

#[derive(strum::EnumIter, strum::VariantNames)]
#[strum(serialize_all = "title_case")]
pub(super) enum TrashCancel {
    Trash,
    Cancel,
}

#[derive(Clone, Copy)]
struct GitPanelViewOptionsMenuState {
    sort_by: GitPanelSortBy,
    group_by: GitPanelGroupBy,
    tree_view: bool,
}

pub(super) fn git_panel_context_menu(
    has_tracked_changes: bool,
    has_staged_changes: bool,
    has_unstaged_changes: bool,
    has_new_changes: bool,
    has_stash_items: bool,
    focus_handle: FocusHandle,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ContextMenu> {
    ContextMenu::build(window, cx, |context_menu, _, _| {
        context_menu
            .context(focus_handle.clone())
            .action_disabled_when(!has_unstaged_changes, "Stage All", StageAll.boxed_clone())
            .action_disabled_when(!has_staged_changes, "Unstage All", UnstageAll.boxed_clone())
            .separator()
            .action_disabled_when(
                !(has_new_changes || has_tracked_changes),
                "Stash All",
                StashAll.boxed_clone(),
            )
            .action_disabled_when(!has_stash_items, "Stash Pop", StashPop.boxed_clone())
            .action("View Stash", mav_actions::git::ViewStash.boxed_clone())
            .separator()
            .action_disabled_when(
                !has_tracked_changes,
                "Discard Tracked Changes",
                RestoreTrackedFiles.boxed_clone(),
            )
            .action_disabled_when(
                !has_new_changes,
                "Trash Untracked Files",
                TrashUntrackedFiles.boxed_clone(),
            )
    })
}

pub(super) fn git_panel_view_options_menu(
    focus_handle: FocusHandle,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ContextMenu> {
    let view_options_menu_state = Rc::new(Cell::new(GitPanelViewOptionsMenuState {
        sort_by: GitPanelSettings::get_global(cx).sort_by,
        group_by: GitPanelSettings::get_global(cx).group_by,
        tree_view: GitPanelSettings::get_global(cx).tree_view,
    }));

    ContextMenu::build_persistent(window, cx, move |context_menu, _, _| {
        let state = view_options_menu_state.get();

        context_menu
            .context(focus_handle.clone())
            .header("View")
            .item({
                let view_options_menu_state = view_options_menu_state.clone();
                ContextMenuEntry::new("List")
                    .toggle(IconPosition::End, !state.tree_view)
                    .handler(move |window, cx| {
                        if state.tree_view {
                            view_options_menu_state.set(GitPanelViewOptionsMenuState {
                                tree_view: false,
                                ..state
                            });
                            window.dispatch_action(Box::new(ToggleTreeView), cx);
                        }
                    })
            })
            .item({
                let view_options_menu_state = view_options_menu_state.clone();
                ContextMenuEntry::new("Tree")
                    .toggle(IconPosition::End, state.tree_view)
                    .handler(move |window, cx| {
                        if !state.tree_view {
                            view_options_menu_state.set(GitPanelViewOptionsMenuState {
                                tree_view: true,
                                ..state
                            });
                            window.dispatch_action(Box::new(ToggleTreeView), cx);
                        }
                    })
            })
            .when(!state.tree_view, |this| {
                this.separator()
                    .header("Sort By")
                    .item({
                        let view_options_menu_state = view_options_menu_state.clone();
                        ContextMenuEntry::new("Path")
                            .toggle(IconPosition::End, state.sort_by == GitPanelSortBy::Path)
                            .handler(move |window, cx| {
                                if !state.tree_view {
                                    view_options_menu_state.set(GitPanelViewOptionsMenuState {
                                        sort_by: GitPanelSortBy::Path,
                                        ..state
                                    });
                                    window.dispatch_action(Box::new(SetSortByPath), cx);
                                }
                            })
                    })
                    .item({
                        let view_options_menu_state = view_options_menu_state.clone();
                        ContextMenuEntry::new("Name")
                            .toggle(IconPosition::End, state.sort_by == GitPanelSortBy::Name)
                            .handler(move |window, cx| {
                                if !state.tree_view {
                                    view_options_menu_state.set(GitPanelViewOptionsMenuState {
                                        sort_by: GitPanelSortBy::Name,
                                        ..state
                                    });
                                    window.dispatch_action(Box::new(SetSortByName), cx);
                                }
                            })
                    })
            })
            .separator()
            .header("Group By")
            .item({
                let view_options_menu_state = view_options_menu_state.clone();
                ContextMenuEntry::new("None")
                    .toggle(IconPosition::End, state.group_by == GitPanelGroupBy::None)
                    .handler(move |window, cx| {
                        if state.group_by != GitPanelGroupBy::None {
                            view_options_menu_state.set(GitPanelViewOptionsMenuState {
                                group_by: GitPanelGroupBy::None,
                                ..state
                            });
                            window.dispatch_action(Box::new(SetGroupByNone), cx);
                        }
                    })
            })
            .item({
                let view_options_menu_state = view_options_menu_state.clone();
                ContextMenuEntry::new("Status")
                    .toggle(IconPosition::End, state.group_by == GitPanelGroupBy::Status)
                    .handler(move |window, cx| {
                        if state.group_by != GitPanelGroupBy::Status {
                            view_options_menu_state.set(GitPanelViewOptionsMenuState {
                                group_by: GitPanelGroupBy::Status,
                                ..state
                            });
                            window.dispatch_action(Box::new(SetGroupByStatus), cx);
                        }
                    })
            })
    })
}

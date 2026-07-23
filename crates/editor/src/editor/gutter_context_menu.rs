use super::*;

impl Editor {
    pub(crate) fn gutter_context_menu(
        &self,
        anchor: Anchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ContextMenu> {
        let weak_editor = cx.weak_entity();
        let focus_handle = self.focus_handle(cx);
        let row = self
            .buffer
            .read(cx)
            .snapshot(cx)
            .summary_for_anchor::<Point>(&anchor)
            .row;

        let breakpoint = self
            .breakpoint_at_row(row, window, cx)
            .map(|(anchor, bp)| (anchor, Arc::from(bp)));

        let log_breakpoint_msg = if breakpoint.as_ref().is_some_and(|bp| bp.1.message.is_some()) {
            "Edit Log Breakpoint"
        } else {
            "Set Log Breakpoint"
        };
        let condition_breakpoint_msg = if breakpoint
            .as_ref()
            .is_some_and(|bp| bp.1.condition.is_some())
        {
            "Edit Condition Breakpoint"
        } else {
            "Set Condition Breakpoint"
        };
        let hit_condition_breakpoint_msg = if breakpoint
            .as_ref()
            .is_some_and(|bp| bp.1.hit_condition.is_some())
        {
            "Edit Hit Condition Breakpoint"
        } else {
            "Set Hit Condition Breakpoint"
        };
        let set_breakpoint_msg = if breakpoint.as_ref().is_some() {
            "Unset Breakpoint"
        } else {
            "Set Breakpoint"
        };
        let git_blame_msg = if self.show_git_blame_gutter {
            "Close Git Blame"
        } else {
            "Open Git Blame"
        };

        let bookmark = self.bookmark_at_row(row, window, cx);
        let set_bookmark_msg = if bookmark.as_ref().is_some() {
            "Remove Bookmark"
        } else {
            "Add Bookmark"
        };
        let has_bookmark = bookmark.as_ref().is_some();
        let run_to_cursor = window.is_action_available(&RunToCursor, cx);
        let toggle_state_entry: Option<(&str, Box<dyn Action>)> =
            breakpoint.as_ref().map(|bp| match bp.1.state {
                BreakpointState::Enabled => {
                    ("Disable", crate::actions::DisableBreakpoint.boxed_clone())
                }
                BreakpointState::Disabled => {
                    ("Enable", crate::actions::EnableBreakpoint.boxed_clone())
                }
            });
        let (anchor, breakpoint) =
            breakpoint.unwrap_or_else(|| (anchor, Arc::new(Breakpoint::new_standard())));

        ContextMenu::build(window, cx, |menu, _, _cx| {
            menu.on_blur_subscription(Subscription::new(|| {}))
                .context(focus_handle)
                .when(run_to_cursor, |this| {
                    let weak_editor = weak_editor.clone();
                    this.entry(
                        "Run to Cursor",
                        Some(RunToCursor.boxed_clone()),
                        move |window, cx| {
                            weak_editor
                                .update(cx, |editor, cx| {
                                    editor.change_selections(
                                        SelectionEffects::no_scroll(),
                                        window,
                                        cx,
                                        |s| {
                                            s.select_ranges(
                                                [Point::new(row, 0)..Point::new(row, 0)],
                                            )
                                        },
                                    );
                                })
                                .ok();
                            window.dispatch_action(Box::new(RunToCursor), cx);
                        },
                    )
                    .separator()
                })
                .when_some(toggle_state_entry, |this, (msg, action)| {
                    this.entry(msg, Some(action), {
                        let weak_editor = weak_editor.clone();
                        let breakpoint = breakpoint.clone();
                        move |_window, cx| {
                            weak_editor
                                .update(cx, |this, cx| {
                                    this.edit_breakpoint_at_anchor(
                                        anchor,
                                        breakpoint.as_ref().clone(),
                                        BreakpointEditAction::InvertState,
                                        cx,
                                    );
                                })
                                .log_err();
                        }
                    })
                })
                .entry(
                    set_breakpoint_msg,
                    Some(crate::actions::ToggleBreakpoint.boxed_clone()),
                    {
                        let weak_editor = weak_editor.clone();
                        let breakpoint = breakpoint.clone();
                        move |_window, cx| {
                            weak_editor
                                .update(cx, |this, cx| {
                                    this.edit_breakpoint_at_anchor(
                                        anchor,
                                        breakpoint.as_ref().clone(),
                                        BreakpointEditAction::Toggle,
                                        cx,
                                    );
                                })
                                .log_err();
                        }
                    },
                )
                .entry(
                    log_breakpoint_msg,
                    Some(crate::actions::EditLogBreakpoint.boxed_clone()),
                    {
                        let breakpoint = breakpoint.clone();
                        let weak_editor = weak_editor.clone();
                        move |window, cx| {
                            weak_editor
                                .update(cx, |this, cx| {
                                    this.add_edit_breakpoint_block(
                                        anchor,
                                        breakpoint.as_ref(),
                                        BreakpointPromptEditAction::Log,
                                        window,
                                        cx,
                                    );
                                })
                                .log_err();
                        }
                    },
                )
                .entry(condition_breakpoint_msg, None, {
                    let breakpoint = breakpoint.clone();
                    let weak_editor = weak_editor.clone();
                    move |window, cx| {
                        weak_editor
                            .update(cx, |this, cx| {
                                this.add_edit_breakpoint_block(
                                    anchor,
                                    breakpoint.as_ref(),
                                    BreakpointPromptEditAction::Condition,
                                    window,
                                    cx,
                                );
                            })
                            .log_err();
                    }
                })
                .entry(hit_condition_breakpoint_msg, None, {
                    let breakpoint = breakpoint.clone();
                    let weak_editor = weak_editor.clone();
                    move |window, cx| {
                        weak_editor
                            .update(cx, |this, cx| {
                                this.add_edit_breakpoint_block(
                                    anchor,
                                    breakpoint.as_ref(),
                                    BreakpointPromptEditAction::HitCondition,
                                    window,
                                    cx,
                                );
                            })
                            .log_err();
                    }
                })
                .separator()
                .entry(git_blame_msg, Some(Blame.boxed_clone()), {
                    let weak_editor = weak_editor.clone();
                    move |window, cx| {
                        weak_editor
                            .update(cx, |this, cx| {
                                this.toggle_git_blame(&Blame, window, cx);
                            })
                            .log_err();
                    }
                })
                .separator()
                .entry(set_bookmark_msg, Some(ToggleBookmark.boxed_clone()), {
                    let weak_editor = weak_editor.clone();
                    move |window, cx| {
                        weak_editor
                            .update(cx, |this, cx| {
                                this.toggle_bookmark_at_anchor(anchor, window, cx);
                            })
                            .log_err();
                    }
                })
                .when(has_bookmark, |this| {
                    this.entry(
                        "Edit Bookmark",
                        Some(EditBookmark.boxed_clone()),
                        move |window, cx| {
                            weak_editor
                                .update(cx, |this, cx| {
                                    this.edit_bookmark_at_anchor(anchor, window, cx);
                                })
                                .log_err();
                        },
                    )
                })
        })
    }
}

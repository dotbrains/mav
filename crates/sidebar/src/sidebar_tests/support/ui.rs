use super::*;

pub(super) fn focus_sidebar(sidebar: &Entity<Sidebar>, cx: &mut gpui::VisualTestContext) {
    sidebar.update_in(cx, |_, window, cx| {
        cx.focus_self(window);
    });
    cx.run_until_parked();
}

pub(super) fn request_test_tool_authorization(
    thread: &Entity<AcpThread>,
    tool_call_id: &str,
    option_id: &str,
    cx: &mut gpui::VisualTestContext,
) {
    let tool_call_id = acp::ToolCallId::new(tool_call_id);
    let label = format!("Tool {tool_call_id}");
    let option_id = acp::PermissionOptionId::new(option_id);
    let _authorization_task = cx.update(|_, cx| {
        thread.update(cx, |thread, cx| {
            thread
                .request_tool_call_authorization(
                    acp::ToolCall::new(tool_call_id, label)
                        .kind(acp::ToolKind::Edit)
                        .into(),
                    PermissionOptions::Flat(vec![acp::PermissionOption::new(
                        option_id,
                        "Allow",
                        acp::PermissionOptionKind::AllowOnce,
                    )]),
                    acp_thread::AuthorizationKind::PermissionGrant,
                    cx,
                )
                .unwrap()
        })
    });
    cx.run_until_parked();
}

pub(super) fn format_linked_worktree_chips(worktrees: &[ThreadItemWorktreeInfo]) -> String {
    let mut seen = Vec::new();
    let mut chips = Vec::new();
    for wt in worktrees {
        if wt.kind == ui::WorktreeKind::Main {
            continue;
        }
        let Some(name) = wt.worktree_name.as_ref() else {
            continue;
        };
        if !seen.contains(name) {
            seen.push(name.clone());
            chips.push(format!("{{{}}}", name));
        }
    }
    if chips.is_empty() {
        String::new()
    } else {
        format!(" {}", chips.join(", "))
    }
}

pub(super) fn visible_entries_as_strings(
    sidebar: &Entity<Sidebar>,
    cx: &mut gpui::VisualTestContext,
) -> Vec<String> {
    sidebar.read_with(cx, |sidebar, cx| {
        sidebar
            .contents
            .entries
            .iter()
            .enumerate()
            .map(|(ix, entry)| {
                let selected = if sidebar.selection == Some(ix) {
                    "  <== selected"
                } else {
                    ""
                };
                match entry {
                    ListEntry::ProjectHeader {
                        label,
                        key,
                        highlight_positions: _,
                        ..
                    } => {
                        let icon = if sidebar.is_group_collapsed(key, cx) {
                            ">"
                        } else {
                            "v"
                        };
                        format!("{} [{}]{}", icon, label, selected)
                    }
                    ListEntry::Thread(thread) => {
                        let title = thread.metadata.display_title();
                        let worktree = format_linked_worktree_chips(&thread.worktrees);

                        {
                            let live = if thread.is_live { " *" } else { "" };
                            let status_str = match thread.status {
                                AgentThreadStatus::Running => " (running)",
                                AgentThreadStatus::Error => " (error)",
                                AgentThreadStatus::WaitingForConfirmation => " (waiting)",
                                _ => "",
                            };
                            let notified = if sidebar
                                .contents
                                .is_thread_notified(&thread.metadata.thread_id)
                            {
                                " (!)"
                            } else {
                                ""
                            };
                            format!("  {title}{worktree}{live}{status_str}{notified}{selected}")
                        }
                    }
                    ListEntry::Terminal(terminal) => {
                        let title = terminal.metadata.display_title();
                        let worktree = format_linked_worktree_chips(&terminal.worktrees);
                        format!("  {title}{worktree}{selected}")
                    }
                }
            })
            .collect()
    })
}

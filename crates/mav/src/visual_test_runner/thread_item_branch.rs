use super::*;

struct ThreadItemBranchNameTestView;

#[cfg(target_os = "macos")]
impl gpui::Render for ThreadItemBranchNameTestView {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use ui::{
            IconName, Label, LabelSize, ThreadItem, ThreadItemWorktreeInfo, WorktreeKind,
            prelude::*,
        };

        let section_label = |text: &str| {
            Label::new(text.to_string())
                .size(LabelSize::Small)
                .color(Color::Muted)
        };

        let container = || {
            v_flex()
                .w_80()
                .border_1()
                .border_color(cx.theme().colors().border_variant)
                .bg(cx.theme().colors().panel_background)
        };

        v_flex()
            .size_full()
            .bg(cx.theme().colors().background)
            .p_4()
            .gap_3()
            .child(
                Label::new("ThreadItem Branch Names")
                    .size(LabelSize::Large)
                    .color(Color::Default),
            )
            .child(section_label(
                "Linked worktree with branch (worktree / branch)",
            ))
            .child(
                container().child(
                    ThreadItem::new("ti-linked-branch", "Fix scrolling behavior")
                        .icon(IconName::AiClaude)
                        .timestamp("5m")
                        .worktrees(vec![ThreadItemWorktreeInfo {
                            worktree_name: Some("jade-glen".into()),
                            full_path: "/worktrees/jade-glen/mav".into(),
                            highlight_positions: Vec::new(),
                            kind: WorktreeKind::Linked,
                            branch_name: Some("fix-scrolling".into()),
                        }]),
                ),
            )
            .child(section_label(
                "Linked worktree without branch (detached HEAD)",
            ))
            .child(
                container().child(
                    ThreadItem::new("ti-linked-no-branch", "Review worktree cleanup")
                        .icon(IconName::AiClaude)
                        .timestamp("1h")
                        .worktrees(vec![ThreadItemWorktreeInfo {
                            worktree_name: Some("focal-arrow".into()),
                            full_path: "/worktrees/focal-arrow/mav".into(),
                            highlight_positions: Vec::new(),
                            kind: WorktreeKind::Linked,
                            branch_name: None,
                        }]),
                ),
            )
            .child(section_label("Main worktree with branch (nothing shown)"))
            .child(
                container().child(
                    ThreadItem::new("ti-main-branch", "Request for Long Classic Poem")
                        .icon(IconName::MavAgent)
                        .timestamp("2d")
                        .worktrees(vec![ThreadItemWorktreeInfo {
                            worktree_name: Some("mav".into()),
                            full_path: "/projects/mav".into(),
                            highlight_positions: Vec::new(),
                            kind: WorktreeKind::Main,
                            branch_name: Some("main".into()),
                        }]),
                ),
            )
            .child(section_label(
                "Main worktree without branch (nothing shown)",
            ))
            .child(
                container().child(
                    ThreadItem::new("ti-main-no-branch", "Simple greeting thread")
                        .icon(IconName::MavAgent)
                        .timestamp("3d")
                        .worktrees(vec![ThreadItemWorktreeInfo {
                            worktree_name: Some("mav".into()),
                            full_path: "/projects/mav".into(),
                            highlight_positions: Vec::new(),
                            kind: WorktreeKind::Main,
                            branch_name: None,
                        }]),
                ),
            )
            .child(section_label("Linked worktree where name matches branch"))
            .child(
                container().child(
                    ThreadItem::new("ti-same-name", "Implement feature")
                        .icon(IconName::AiClaude)
                        .timestamp("6d")
                        .worktrees(vec![ThreadItemWorktreeInfo {
                            worktree_name: Some("stoic-reed".into()),
                            full_path: "/worktrees/stoic-reed/mav".into(),
                            highlight_positions: Vec::new(),
                            kind: WorktreeKind::Linked,
                            branch_name: Some("stoic-reed".into()),
                        }]),
                ),
            )
            .child(section_label(
                "Manually opened linked worktree (main_path resolves to original repo)",
            ))
            .child(
                container().child(
                    ThreadItem::new("ti-manual-linked", "Robust Git Worktree Rollback")
                        .icon(IconName::MavAgent)
                        .timestamp("40m")
                        .worktrees(vec![ThreadItemWorktreeInfo {
                            worktree_name: Some("focal-arrow".into()),
                            full_path: "/worktrees/focal-arrow/mav".into(),
                            highlight_positions: Vec::new(),
                            kind: WorktreeKind::Linked,
                            branch_name: Some("persist-worktree-3-wiring".into()),
                        }]),
                ),
            )
            .child(section_label(
                "Linked worktree + branch + diff stats + timestamp",
            ))
            .child(
                container().child(
                    ThreadItem::new("ti-linked-full", "Full metadata with diff stats")
                        .icon(IconName::AiClaude)
                        .timestamp("3w")
                        .added(42)
                        .removed(17)
                        .worktrees(vec![ThreadItemWorktreeInfo {
                            worktree_name: Some("jade-glen".into()),
                            full_path: "/worktrees/jade-glen/mav".into(),
                            highlight_positions: Vec::new(),
                            kind: WorktreeKind::Linked,
                            branch_name: Some("feature-branch".into()),
                        }]),
                ),
            )
            .child(section_label("Long branch name truncation with diff stats"))
            .child(
                container().child(
                    ThreadItem::new("ti-long-branch", "Overflow test with very long branch")
                        .icon(IconName::AiClaude)
                        .timestamp("2d")
                        .added(108)
                        .removed(53)
                        .worktrees(vec![ThreadItemWorktreeInfo {
                            worktree_name: Some("my-project".into()),
                            full_path: "/worktrees/my-project/mav".into(),
                            highlight_positions: Vec::new(),
                            kind: WorktreeKind::Linked,
                            branch_name: Some(
                                "fix-very-long-branch-name-that-should-truncate".into(),
                            ),
                        }]),
                ),
            )
            .child(section_label(
                "Main worktree with branch + diff stats + timestamp (branch hidden)",
            ))
            .child(
                container().child(
                    ThreadItem::new("ti-main-full", "Main worktree with everything")
                        .icon(IconName::MavAgent)
                        .timestamp("5m")
                        .added(23)
                        .removed(8)
                        .worktrees(vec![ThreadItemWorktreeInfo {
                            worktree_name: Some("mav".into()),
                            full_path: "/projects/mav".into(),
                            highlight_positions: Vec::new(),
                            kind: WorktreeKind::Main,
                            branch_name: Some("sidebar-show-branch-name".into()),
                        }]),
                ),
            )
    }
}

#[cfg(target_os = "macos")]
fn run_thread_item_branch_name_visual_tests(
    _app_state: Arc<AppState>,
    cx: &mut VisualTestAppContext,
    update_baseline: bool,
) -> Result<TestResult> {
    let window_size = size(px(400.0), px(1150.0));
    let bounds = Bounds {
        origin: point(px(0.0), px(0.0)),
        size: window_size,
    };

    let window = cx
        .update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    focus: false,
                    show: false,
                    ..Default::default()
                },
                |_window, cx| cx.new(|_| ThreadItemBranchNameTestView),
            )
        })
        .context("Failed to open thread item branch name test window")?;

    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, _cx| {
        window.refresh();
    })?;

    cx.run_until_parked();

    let test_result = run_visual_test(
        "thread_item_branch_names",
        window.into(),
        cx,
        update_baseline,
    )?;

    cx.update_window(window.into(), |_, window, _cx| {
        window.remove_window();
    })
    .log_err();

    cx.run_until_parked();

    for _ in 0..15 {
        cx.advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
    }

    Ok(test_result)
}

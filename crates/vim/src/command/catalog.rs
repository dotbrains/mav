use super::*;

fn generate_commands(_: &App) -> Vec<VimCommand> {
    vec![
        VimCommand::new(
            ("w", "rite"),
            VimSave {
                save_intent: Some(SaveIntent::Save),
                filename: "".into(),
                range: None,
            },
        )
        .bang(VimSave {
            save_intent: Some(SaveIntent::Overwrite),
            filename: "".into(),
            range: None,
        })
        .filename(|action, filename| {
            Some(
                VimSave {
                    save_intent: action
                        .as_any()
                        .downcast_ref::<VimSave>()
                        .and_then(|action| action.save_intent),
                    filename,
                    range: None,
                }
                .boxed_clone(),
            )
        })
        .range(|action, range| {
            let mut action: VimSave = action.as_any().downcast_ref::<VimSave>().unwrap().clone();
            action.range.replace(range.clone());
            Some(Box::new(action))
        }),
        VimCommand::new(("e", "dit"), editor::actions::ReloadFile)
            .bang(editor::actions::ReloadFile)
            .filename(|_, filename| Some(VimEdit { filename }.boxed_clone())),
        VimCommand::new(
            ("r", "ead"),
            VimRead {
                range: None,
                filename: "".into(),
            },
        )
        .filename(|_, filename| {
            Some(
                VimRead {
                    range: None,
                    filename,
                }
                .boxed_clone(),
            )
        })
        .range(|action, range| {
            let mut action: VimRead = action.as_any().downcast_ref::<VimRead>().unwrap().clone();
            action.range.replace(range.clone());
            Some(Box::new(action))
        }),
        VimCommand::new(("sp", "lit"), workspace::SplitHorizontal::default()).filename(
            |_, filename| {
                Some(
                    VimSplit {
                        vertical: false,
                        filename,
                    }
                    .boxed_clone(),
                )
            },
        ),
        VimCommand::new(("vs", "plit"), workspace::SplitVertical::default()).filename(
            |_, filename| {
                Some(
                    VimSplit {
                        vertical: true,
                        filename,
                    }
                    .boxed_clone(),
                )
            },
        ),
        VimCommand::new(("tabe", "dit"), workspace::NewFile)
            .filename(|_action, filename| Some(VimEdit { filename }.boxed_clone())),
        VimCommand::new(("tabnew", ""), workspace::NewFile)
            .filename(|_action, filename| Some(VimEdit { filename }.boxed_clone())),
        VimCommand::new(
            ("q", "uit"),
            workspace::CloseActiveItem {
                save_intent: Some(SaveIntent::Close),
                close_pinned: false,
            },
        )
        .bang(workspace::CloseActiveItem {
            save_intent: Some(SaveIntent::Skip),
            close_pinned: true,
        }),
        VimCommand::new(
            ("wq", ""),
            workspace::CloseActiveItem {
                save_intent: Some(SaveIntent::Save),
                close_pinned: false,
            },
        )
        .bang(workspace::CloseActiveItem {
            save_intent: Some(SaveIntent::Overwrite),
            close_pinned: true,
        }),
        VimCommand::new(
            ("x", "it"),
            workspace::CloseActiveItem {
                save_intent: Some(SaveIntent::SaveAll),
                close_pinned: false,
            },
        )
        .bang(workspace::CloseActiveItem {
            save_intent: Some(SaveIntent::Overwrite),
            close_pinned: true,
        }),
        VimCommand::new(
            ("exi", "t"),
            workspace::CloseActiveItem {
                save_intent: Some(SaveIntent::SaveAll),
                close_pinned: false,
            },
        )
        .bang(workspace::CloseActiveItem {
            save_intent: Some(SaveIntent::Overwrite),
            close_pinned: true,
        }),
        VimCommand::new(
            ("up", "date"),
            workspace::Save {
                save_intent: Some(SaveIntent::SaveAll),
            },
        ),
        VimCommand::new(
            ("wa", "ll"),
            workspace::SaveAll {
                save_intent: Some(SaveIntent::SaveAll),
            },
        )
        .bang(workspace::SaveAll {
            save_intent: Some(SaveIntent::Overwrite),
        }),
        VimCommand::new(
            ("qa", "ll"),
            workspace::CloseAllItemsAndPanes {
                save_intent: Some(SaveIntent::Close),
            },
        )
        .bang(workspace::CloseAllItemsAndPanes {
            save_intent: Some(SaveIntent::Skip),
        }),
        VimCommand::new(
            ("quita", "ll"),
            workspace::CloseAllItemsAndPanes {
                save_intent: Some(SaveIntent::Close),
            },
        )
        .bang(workspace::CloseAllItemsAndPanes {
            save_intent: Some(SaveIntent::Skip),
        }),
        VimCommand::new(
            ("xa", "ll"),
            workspace::CloseAllItemsAndPanes {
                save_intent: Some(SaveIntent::SaveAll),
            },
        )
        .bang(workspace::CloseAllItemsAndPanes {
            save_intent: Some(SaveIntent::Overwrite),
        }),
        VimCommand::new(
            ("wqa", "ll"),
            workspace::CloseAllItemsAndPanes {
                save_intent: Some(SaveIntent::SaveAll),
            },
        )
        .bang(workspace::CloseAllItemsAndPanes {
            save_intent: Some(SaveIntent::Overwrite),
        }),
        VimCommand::new(("cq", "uit"), mav_actions::Quit),
        VimCommand::new(
            ("bd", "elete"),
            workspace::CloseItemInAllPanes {
                save_intent: Some(SaveIntent::Close),
                close_pinned: false,
            },
        )
        .bang(workspace::CloseItemInAllPanes {
            save_intent: Some(SaveIntent::Skip),
            close_pinned: true,
        }),
        VimCommand::new(
            ("norm", "al"),
            VimNorm {
                command: "".into(),
                range: None,
                override_rows: None,
            },
        )
        .args(|_, args| {
            Some(
                VimNorm {
                    command: args,
                    range: None,
                    override_rows: None,
                }
                .boxed_clone(),
            )
        })
        .range(|action, range| {
            let mut action: VimNorm = action.as_any().downcast_ref::<VimNorm>().unwrap().clone();
            action.range.replace(range.clone());
            Some(Box::new(action))
        }),
        VimCommand::new(("bn", "ext"), workspace::ActivateNextItem::default()).count(),
        VimCommand::new(("bN", "ext"), workspace::ActivatePreviousItem::default()).count(),
        VimCommand::new(
            ("bp", "revious"),
            workspace::ActivatePreviousItem::default(),
        )
        .count(),
        VimCommand::new(("bf", "irst"), workspace::ActivateItem(0)),
        VimCommand::new(("br", "ewind"), workspace::ActivateItem(0)),
        VimCommand::new(("bl", "ast"), workspace::ActivateLastItem),
        VimCommand::str(("buffers", ""), "tab_switcher::ToggleAll"),
        VimCommand::str(("ls", ""), "tab_switcher::ToggleAll"),
        VimCommand::new(("new", ""), workspace::NewFileSplitHorizontal),
        VimCommand::new(("vne", "w"), workspace::NewFileSplitVertical),
        VimCommand::new(("tabn", "ext"), workspace::ActivateNextItem::default()).count(),
        VimCommand::new(
            ("tabp", "revious"),
            workspace::ActivatePreviousItem::default(),
        )
        .count(),
        VimCommand::new(("tabN", "ext"), workspace::ActivatePreviousItem::default()).count(),
        VimCommand::new(
            ("tabc", "lose"),
            workspace::CloseActiveItem {
                save_intent: Some(SaveIntent::Close),
                close_pinned: false,
            },
        ),
        VimCommand::new(
            ("tabo", "nly"),
            workspace::CloseOtherItems {
                save_intent: Some(SaveIntent::Close),
                close_pinned: false,
            },
        )
        .bang(workspace::CloseOtherItems {
            save_intent: Some(SaveIntent::Skip),
            close_pinned: false,
        }),
        VimCommand::new(
            ("on", "ly"),
            workspace::CloseInactiveTabsAndPanes {
                save_intent: Some(SaveIntent::Close),
            },
        )
        .bang(workspace::CloseInactiveTabsAndPanes {
            save_intent: Some(SaveIntent::Skip),
        }),
        VimCommand::str(("cl", "ist"), "diagnostics::Deploy"),
        VimCommand::new(("cc", ""), editor::actions::Hover),
        VimCommand::new(("ll", ""), editor::actions::Hover),
        VimCommand::new(("cn", "ext"), editor::actions::GoToDiagnostic::default())
            .range(wrap_count),
        VimCommand::new(
            ("cp", "revious"),
            editor::actions::GoToPreviousDiagnostic::default(),
        )
        .range(wrap_count),
        VimCommand::new(
            ("cN", "ext"),
            editor::actions::GoToPreviousDiagnostic::default(),
        )
        .range(wrap_count),
        VimCommand::new(
            ("lp", "revious"),
            editor::actions::GoToPreviousDiagnostic::default(),
        )
        .range(wrap_count),
        VimCommand::new(
            ("lN", "ext"),
            editor::actions::GoToPreviousDiagnostic::default(),
        )
        .range(wrap_count),
        VimCommand::new(("j", "oin"), JoinLines).range(select_range),
        VimCommand::new(("reflow", ""), Rewrap { line_length: None })
            .range(select_range)
            .args(|_action, args| {
                args.parse::<usize>().map_or(None, |length| {
                    Some(Box::new(Rewrap {
                        line_length: Some(length),
                    }))
                })
            }),
        VimCommand::new(("fo", "ld"), editor::actions::FoldSelectedRanges).range(act_on_range),
        VimCommand::new(("foldo", "pen"), editor::actions::UnfoldLines)
            .bang(editor::actions::UnfoldRecursive)
            .range(act_on_range),
        VimCommand::new(("foldc", "lose"), editor::actions::Fold)
            .bang(editor::actions::FoldRecursive)
            .range(act_on_range),
        VimCommand::new(("dif", "fupdate"), editor::actions::ToggleSelectedDiffHunks)
            .range(act_on_range),
        VimCommand::str(("rev", "ert"), "git::Restore").range(act_on_range),
        VimCommand::new(("d", "elete"), VisualDeleteLine).range(select_range),
        VimCommand::new(("y", "ank"), gpui::NoAction).range(|_, range| {
            Some(
                YankCommand {
                    range: range.clone(),
                }
                .boxed_clone(),
            )
        }),
        VimCommand::new(("reg", "isters"), ToggleRegistersView).bang(ToggleRegistersView),
        VimCommand::new(("di", "splay"), ToggleRegistersView).bang(ToggleRegistersView),
        VimCommand::new(("marks", ""), ToggleMarksView).bang(ToggleMarksView),
        VimCommand::new(("delm", "arks"), ArgumentRequired)
            .bang(DeleteMarks::AllLocal)
            .args(|_, args| Some(DeleteMarks::Marks(args).boxed_clone())),
        VimCommand::new(("sor", "t"), SortLinesCaseSensitive)
            .range(select_range)
            .default_range(CommandRange::buffer()),
        VimCommand::new(("sort i", ""), SortLinesCaseInsensitive)
            .range(select_range)
            .default_range(CommandRange::buffer()),
        VimCommand::str(("E", "xplore"), "project_panel::ToggleFocus"),
        VimCommand::str(("H", "explore"), "project_panel::ToggleFocus"),
        VimCommand::str(("L", "explore"), "project_panel::ToggleFocus"),
        VimCommand::str(("S", "explore"), "project_panel::ToggleFocus"),
        VimCommand::str(("Ve", "xplore"), "project_panel::ToggleFocus"),
        VimCommand::str(("te", "rm"), "workspace::NewTerminal"),
        VimCommand::str(("T", "erm"), "workspace::NewTerminal"),
        VimCommand::str(("C", "ollab"), "collab_panel::ToggleFocus"),
        VimCommand::str(("A", "I"), "agent::ToggleFocus"),
        VimCommand::str(("G", "it"), "git_panel::ToggleFocus"),
        VimCommand::str(("D", "ebug"), "debug_panel::ToggleFocus"),
        VimCommand::new(("noh", "lsearch"), search::buffer_search::Dismiss),
        VimCommand::new(("$", ""), EndOfDocument),
        VimCommand::new(("%", ""), EndOfDocument),
        VimCommand::new(("0", ""), StartOfDocument),
        VimCommand::new(("ex", ""), editor::actions::ReloadFile).bang(editor::actions::ReloadFile),
        VimCommand::new(("cpp", "link"), editor::actions::CopyPermalinkToLine).range(act_on_range),
        VimCommand::str(("opt", "ions"), "mav::OpenDefaultSettings"),
        VimCommand::str(("map", ""), "vim::OpenDefaultKeymap"),
        VimCommand::new(("h", "elp"), OpenDocs),
    ]
}

struct VimCommands(Vec<VimCommand>);
// safety: we only ever access this from the main thread (as ensured by the cx argument)
// actions are not Sync so we can't otherwise use a OnceLock.
unsafe impl Sync for VimCommands {}
impl Global for VimCommands {}

fn commands(cx: &App) -> &Vec<VimCommand> {
    static COMMANDS: OnceLock<VimCommands> = OnceLock::new();
    &COMMANDS
        .get_or_init(|| VimCommands(generate_commands(cx)))
        .0
}

fn act_on_range(action: Box<dyn Action>, range: &CommandRange) -> Option<Box<dyn Action>> {
    Some(
        WithRange {
            restore_selection: true,
            range: range.clone(),
            action: WrappedAction(action),
        }
        .boxed_clone(),
    )
}

fn select_range(action: Box<dyn Action>, range: &CommandRange) -> Option<Box<dyn Action>> {
    Some(
        WithRange {
            restore_selection: false,
            range: range.clone(),
            action: WrappedAction(action),
        }
        .boxed_clone(),
    )
}

fn wrap_count(action: Box<dyn Action>, range: &CommandRange) -> Option<Box<dyn Action>> {
    range.as_count().map(|count| {
        WithCount {
            count,
            action: WrappedAction(action),
        }
        .boxed_clone()
    })
}

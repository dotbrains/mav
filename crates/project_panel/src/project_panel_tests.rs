use super::*;
// use crate::undo::tests::{build_create_operation, build_rename_operation};
use collections::HashSet;
use editor::{Editor, MultiBufferOffset};
use git::{
    Oid,
    repository::{InitialGraphCommitData, LogSource, RepoPath},
};
use gpui::{Empty, Entity, TestAppContext, VisualTestContext};
use menu::Cancel;
use pretty_assertions::assert_eq;
use project::{FakeFs, ProjectPath};
use serde_json::json;
use settings::{ProjectPanelAutoOpenSettings, SettingsStore};
use smallvec::smallvec;
use std::path::{Path, PathBuf};
use util::{path, paths::PathStyle, rel_path::rel_path};
use workspace::{
    AppState, ItemHandle, MultiWorkspace, Pane, Workspace,
    item::{Item, ProjectItem, test::TestItem},
    register_project_item,
};

#[path = "tests/adding_entries.rs"]
mod adding_entries;
#[path = "tests/autoreveal_follow.rs"]
mod autoreveal_follow;
#[path = "tests/autoreveal_gitignored.rs"]
mod autoreveal_gitignored;
#[path = "tests/collapse_basic.rs"]
mod collapse_basic;
#[path = "tests/collapse_entry.rs"]
mod collapse_entry;
#[path = "tests/collapse_selected.rs"]
mod collapse_selected;
#[path = "tests/collapse_worktrees.rs"]
mod collapse_worktrees;
#[path = "tests/compare_reveal.rs"]
mod compare_reveal;
#[path = "tests/copy_paste_directories.rs"]
mod copy_paste_directories;
#[path = "tests/copy_paste_files.rs"]
mod copy_paste_files;
#[path = "tests/create_without_selection.rs"]
mod create_without_selection;
#[path = "tests/delete_prompt.rs"]
mod delete_prompt;
#[path = "tests/deletion_basic.rs"]
mod deletion_basic;
#[path = "tests/deletion_complex.rs"]
mod deletion_complex;
#[path = "tests/deletion_priority.rs"]
mod deletion_priority;
#[path = "tests/directory_selection.rs"]
mod directory_selection;
#[path = "tests/drag_highlighting.rs"]
mod drag_highlighting;
#[path = "tests/drag_operations.rs"]
mod drag_operations;
#[path = "tests/duplicate_items.rs"]
mod duplicate_items;
#[path = "tests/duplicate_items_history.rs"]
mod duplicate_items_history;
#[path = "tests/editing_files.rs"]
mod editing_files;
#[path = "tests/excluded_creation.rs"]
mod excluded_creation;
#[path = "tests/exclusions_auto_collapse.rs"]
mod exclusions_auto_collapse;
#[path = "tests/expand_all.rs"]
mod expand_all;
#[path = "tests/expand_entry.rs"]
mod expand_entry;
#[path = "tests/explicit_reveal.rs"]
mod explicit_reveal;
#[path = "tests/folded_creation.rs"]
mod folded_creation;
#[path = "tests/git_entry_selection.rs"]
mod git_entry_selection;
#[path = "tests/hide_hidden.rs"]
mod hide_hidden;
#[path = "tests/hide_root.rs"]
mod hide_root;
#[path = "tests/marked_entries_drag.rs"]
mod marked_entries_drag;
#[path = "tests/remove_auto_open.rs"]
mod remove_auto_open;
#[path = "tests/rename_history.rs"]
mod rename_history;
#[path = "tests/rename_move.rs"]
mod rename_move;
#[path = "tests/sort_modes.rs"]
mod sort_modes;
#[path = "tests/test_project_item.rs"]
mod test_project_item;
#[path = "tests/visible_open_history.rs"]
mod visible_open_history;

pub(crate) use test_project_item::TestProjectItemView;

pub(crate) fn select_path(panel: &Entity<ProjectPanel>, path: &str, cx: &mut VisualTestContext) {
    let path = rel_path(path);
    panel.update_in(cx, |panel, window, cx| {
        for worktree in panel.project.read(cx).worktrees(cx).collect::<Vec<_>>() {
            let worktree = worktree.read(cx);
            if let Ok(relative_path) = path.strip_prefix(worktree.root_name()) {
                let entry_id = worktree.entry_for_path(relative_path).unwrap().id;
                panel.update_visible_entries(
                    Some((worktree.id(), entry_id)),
                    false,
                    false,
                    window,
                    cx,
                );
                return;
            }
        }
        panic!("no worktree for path {:?}", path);
    });
    cx.run_until_parked();
}

pub(crate) fn select_path_with_mark(
    panel: &Entity<ProjectPanel>,
    path: &str,
    cx: &mut VisualTestContext,
) {
    let path = rel_path(path);
    panel.update(cx, |panel, cx| {
        for worktree in panel.project.read(cx).worktrees(cx).collect::<Vec<_>>() {
            let worktree = worktree.read(cx);
            if let Ok(relative_path) = path.strip_prefix(worktree.root_name()) {
                let entry_id = worktree.entry_for_path(relative_path).unwrap().id;
                let entry = crate::SelectedEntry {
                    worktree_id: worktree.id(),
                    entry_id,
                };
                if !panel.marked_entries.contains(&entry) {
                    panel.marked_entries.push(entry);
                }
                panel.selection = Some(entry);
                return;
            }
        }
        panic!("no worktree for path {:?}", path);
    });
}

/// `leaf_path` is the full path to the leaf entry (e.g., "root/a/b/c")
/// `active_ancestor_path` is the path to the folded component that should be active.
fn select_folded_path_with_mark(
    panel: &Entity<ProjectPanel>,
    leaf_path: &str,
    active_ancestor_path: &str,
    cx: &mut VisualTestContext,
) {
    select_path_with_mark(panel, leaf_path, cx);
    set_folded_active_ancestor(panel, leaf_path, active_ancestor_path, cx);
}

fn set_folded_active_ancestor(
    panel: &Entity<ProjectPanel>,
    leaf_path: &str,
    active_ancestor_path: &str,
    cx: &mut VisualTestContext,
) {
    let leaf_path = rel_path(leaf_path);
    let active_ancestor_path = rel_path(active_ancestor_path);
    panel.update(cx, |panel, cx| {
        let mut leaf_entry_id = None;
        let mut target_entry_id = None;

        for worktree in panel.project.read(cx).worktrees(cx).collect::<Vec<_>>() {
            let worktree = worktree.read(cx);
            if let Ok(relative_path) = leaf_path.strip_prefix(worktree.root_name()) {
                leaf_entry_id = worktree.entry_for_path(relative_path).map(|entry| entry.id);
            }
            if let Ok(relative_path) = active_ancestor_path.strip_prefix(worktree.root_name()) {
                target_entry_id = worktree.entry_for_path(relative_path).map(|entry| entry.id);
            }
        }

        let leaf_entry_id =
            leaf_entry_id.unwrap_or_else(|| panic!("no entry for leaf path {leaf_path:?}"));
        let target_entry_id = target_entry_id
            .unwrap_or_else(|| panic!("no entry for active path {active_ancestor_path:?}"));
        let folded_ancestors = panel
            .state
            .ancestors
            .get_mut(&leaf_entry_id)
            .unwrap_or_else(|| panic!("leaf path {leaf_path:?} should be folded"));
        let ancestor_ids = folded_ancestors.ancestors.clone();

        let mut depth_for_target = None;
        for depth in 0..ancestor_ids.len() {
            let resolved_entry_id = if depth == 0 {
                leaf_entry_id
            } else {
                ancestor_ids.get(depth).copied().unwrap_or(leaf_entry_id)
            };
            if resolved_entry_id == target_entry_id {
                depth_for_target = Some(depth);
                break;
            }
        }

        folded_ancestors.current_ancestor_depth = depth_for_target.unwrap_or_else(|| {
            panic!(
                "active path {active_ancestor_path:?} is not part of folded ancestors {ancestor_ids:?}"
            )
        });
    });
}

pub(crate) fn drag_selection_to(
    panel: &Entity<ProjectPanel>,
    target_path: &str,
    is_file: bool,
    cx: &mut VisualTestContext,
) {
    let target_entry = find_project_entry(panel, target_path, cx)
        .unwrap_or_else(|| panic!("no entry for target path {target_path:?}"));

    panel.update_in(cx, |panel, window, cx| {
        let selection = panel
            .selection
            .expect("a selection is required before dragging");
        let drag = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id: selection.worktree_id,
                entry_id: panel.resolve_entry(selection.entry_id),
            },
            marked_selections: Arc::from(panel.marked_entries.clone()),
            source_pane: None,
            active_selection_is_file: true,
        };
        panel.drag_onto(&drag, target_entry, is_file, window, cx);
    });
    cx.executor().run_until_parked();
}

pub(crate) fn find_project_entry(
    panel: &Entity<ProjectPanel>,
    path: &str,
    cx: &mut VisualTestContext,
) -> Option<ProjectEntryId> {
    let path = rel_path(path);
    panel.update(cx, |panel, cx| {
        for worktree in panel.project.read(cx).worktrees(cx).collect::<Vec<_>>() {
            let worktree = worktree.read(cx);
            if let Ok(relative_path) = path.strip_prefix(worktree.root_name()) {
                return worktree.entry_for_path(relative_path).map(|entry| entry.id);
            }
        }
        panic!("no worktree for path {path:?}");
    })
}

fn visible_entries_as_strings(
    panel: &Entity<ProjectPanel>,
    range: Range<usize>,
    cx: &mut VisualTestContext,
) -> Vec<String> {
    let mut result = Vec::new();
    let mut project_entries = HashSet::default();
    let mut has_editor = false;

    panel.update_in(cx, |panel, window, cx| {
        panel.for_each_visible_entry(range, window, cx, &mut |project_entry, details, _, _| {
            if details.is_editing {
                assert!(!has_editor, "duplicate editor entry");
                has_editor = true;
            } else {
                assert!(
                    project_entries.insert(project_entry),
                    "duplicate project entry {:?} {:?}",
                    project_entry,
                    details
                );
            }

            let indent = "    ".repeat(details.depth);
            let icon = if details.kind.is_dir() {
                if details.is_expanded { "v " } else { "> " }
            } else {
                "  "
            };
            #[cfg(windows)]
            let filename = details.filename.replace("\\", "/");
            #[cfg(not(windows))]
            let filename = details.filename;
            let name = if details.is_editing {
                format!("[EDITOR: '{}']", filename)
            } else if details.is_processing {
                format!("[PROCESSING: '{}']", filename)
            } else {
                filename
            };
            let selected = if details.is_selected {
                "  <== selected"
            } else {
                ""
            };
            let marked = if details.is_marked {
                "  <== marked"
            } else {
                ""
            };

            result.push(format!("{indent}{icon}{name}{selected}{marked}"));
        });
    });

    result
}

pub(crate) fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        crate::init(cx);

        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_fold_dirs = Some(false);
                settings.project.worktree.file_scan_exclusions = Some(Vec::new());
            });
        });
    });
}

fn init_test_with_editor(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let app_state = AppState::test(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        crate::init(cx);
        workspace::init(app_state, cx);

        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_fold_dirs = Some(false);
                settings.project.worktree.file_scan_exclusions = Some(Vec::new())
            });
        });
    });
}

fn init_test_with_git_ui(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let app_state = AppState::test(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        git_ui::init(cx);
        crate::init(cx);
        workspace::init(app_state, cx);

        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_fold_dirs = Some(false);
                settings.project.worktree.file_scan_exclusions = Some(Vec::new())
            });
        });
    });
}

fn set_auto_open_settings(
    cx: &mut TestAppContext,
    auto_open_settings: ProjectPanelAutoOpenSettings,
) {
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project_panel.get_or_insert_default().auto_open = Some(auto_open_settings);
            });
        })
    });
}

fn ensure_single_file_is_opened(
    workspace: &Entity<Workspace>,
    expected_path: &str,
    cx: &mut VisualTestContext,
) {
    workspace.update_in(cx, |workspace, _, cx| {
        let worktrees = workspace.worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1);
        let worktree_id = worktrees[0].read(cx).id();

        let open_project_paths = workspace
            .panes()
            .iter()
            .filter_map(|pane| pane.read(cx).active_item()?.project_path(cx))
            .collect::<Vec<_>>();
        assert_eq!(
            open_project_paths,
            vec![ProjectPath {
                worktree_id,
                path: Arc::from(rel_path(expected_path))
            }],
            "Should have opened file, selected in project panel"
        );
    });
}

fn submit_deletion(panel: &Entity<ProjectPanel>, cx: &mut VisualTestContext) {
    assert!(
        !cx.has_pending_prompt(),
        "Should have no prompts before the deletion"
    );
    panel.update_in(cx, |panel, window, cx| {
        panel.delete(&Delete { skip_prompt: false }, window, cx)
    });
    assert!(
        cx.has_pending_prompt(),
        "Should have a prompt after the deletion"
    );
    cx.simulate_prompt_answer("Delete");
    assert!(
        !cx.has_pending_prompt(),
        "Should have no prompts after prompt was replied to"
    );
    cx.executor().run_until_parked();
}

fn submit_deletion_skipping_prompt(panel: &Entity<ProjectPanel>, cx: &mut VisualTestContext) {
    assert!(
        !cx.has_pending_prompt(),
        "Should have no prompts before the deletion"
    );
    panel.update_in(cx, |panel, window, cx| {
        panel.delete(&Delete { skip_prompt: true }, window, cx)
    });
    assert!(!cx.has_pending_prompt(), "Should have received no prompts");
    cx.executor().run_until_parked();
}

fn ensure_no_open_items_and_panes(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    assert!(
        !cx.has_pending_prompt(),
        "Should have no prompts after deletion operation closes the file"
    );
    workspace.update_in(cx, |workspace, _window, cx| {
        let open_project_paths = workspace
            .panes()
            .iter()
            .filter_map(|pane| pane.read(cx).active_item()?.project_path(cx))
            .collect::<Vec<_>>();
        assert!(
            open_project_paths.is_empty(),
            "Deleted file's buffer should be closed, but got open files: {open_project_paths:?}"
        );
    });
}

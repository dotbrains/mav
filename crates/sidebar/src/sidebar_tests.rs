use super::*;
use acp_thread::{AcpThread, PermissionOptions, StubAgentConnection};
use agent::ThreadStore;
use agent_ui::{
    ThreadId,
    terminal_thread_metadata_store::{
        TerminalThreadMetadata, TerminalThreadMetadataStore, TestTerminalMetadataDbName,
    },
    test_support::{
        active_session_id, active_thread_id, open_thread_with_connection,
        open_thread_with_custom_connection, send_message,
    },
    thread_metadata_store::{ThreadMetadata, WorktreePaths},
};
use chrono::DateTime;
use fs::{FakeFs, Fs};
use gpui::TestAppContext;
use pretty_assertions::assert_eq;
use project::AgentId;
use settings::SettingsStore;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use util::{path_list::PathList, rel_path::rel_path};

#[path = "sidebar_tests/support/assertions.rs"]
mod support_assertions;
#[path = "sidebar_tests/support/metadata.rs"]
mod support_metadata;
#[path = "sidebar_tests/support/setup.rs"]
mod support_setup;
#[path = "sidebar_tests/support/ui.rs"]
mod support_ui;

use support_assertions::*;
use support_metadata::*;
use support_setup::*;
use support_ui::*;

#[path = "sidebar_tests/agent_panel_terminals.rs"]
mod agent_panel_terminals;
#[path = "sidebar_tests/archive_cross_window.rs"]
mod archive_cross_window;
#[path = "sidebar_tests/archive_diverged_worktree_paths.rs"]
mod archive_diverged_worktree_paths;
#[path = "sidebar_tests/archive_linked_worktree_threads.rs"]
mod archive_linked_worktree_threads;
#[path = "sidebar_tests/archive_mixed_workspace_items.rs"]
mod archive_mixed_workspace_items;
#[path = "sidebar_tests/archive_path_resolution.rs"]
mod archive_path_resolution;
#[path = "sidebar_tests/archive_terminal_worktrees.rs"]
mod archive_terminal_worktrees;
#[path = "sidebar_tests/archive_thread_worktrees.rs"]
mod archive_thread_worktrees;
#[path = "sidebar_tests/archive_visibility.rs"]
mod archive_visibility;
#[path = "sidebar_tests/archive_worktree_cleanup.rs"]
mod archive_worktree_cleanup;
#[path = "sidebar_tests/collab_and_header_activation.rs"]
mod collab_and_header_activation;
#[path = "sidebar_tests/discard_mixed_workspace_draft.rs"]
mod discard_mixed_workspace_draft;
#[path = "sidebar_tests/draft_activation_and_path_migration.rs"]
mod draft_activation_and_path_migration;
#[path = "sidebar_tests/draft_lifecycle.rs"]
mod draft_lifecycle;
#[path = "sidebar_tests/draft_removal.rs"]
mod draft_removal;
#[path = "sidebar_tests/draft_visibility.rs"]
mod draft_visibility;
#[path = "sidebar_tests/focused_thread.rs"]
mod focused_thread;
#[path = "sidebar_tests/historical_threads.rs"]
mod historical_threads;
#[path = "sidebar_tests/icon_parsing.rs"]
mod icon_parsing;
#[path = "sidebar_tests/keyboard_navigation.rs"]
mod keyboard_navigation;
#[path = "sidebar_tests/linked_worktree_archive.rs"]
mod linked_worktree_archive;
#[path = "sidebar_tests/linked_worktree_terminal_close.rs"]
mod linked_worktree_terminal_close;
#[path = "sidebar_tests/linked_worktree_thread_visibility.rs"]
mod linked_worktree_thread_visibility;
#[path = "sidebar_tests/remote_archive_active.rs"]
mod remote_archive_active;
#[path = "sidebar_tests/remote_archive_edge_cases.rs"]
mod remote_archive_edge_cases;
#[path = "sidebar_tests/remote_project_integration.rs"]
mod remote_project_integration;
#[path = "sidebar_tests/search.rs"]
mod search;
#[path = "sidebar_tests/sidebar_basic_entries.rs"]
mod sidebar_basic_entries;
#[path = "sidebar_tests/sidebar_measurement_serialization.rs"]
mod sidebar_measurement_serialization;
#[path = "sidebar_tests/startup_restoration.rs"]
mod startup_restoration;
#[path = "sidebar_tests/thread_rename.rs"]
mod thread_rename;
#[path = "sidebar_tests/thread_status_selection.rs"]
mod thread_status_selection;
#[path = "sidebar_tests/thread_switcher_ordering.rs"]
mod thread_switcher_ordering;
#[path = "sidebar_tests/thread_switcher_terminal_rows.rs"]
mod thread_switcher_terminal_rows;
#[path = "sidebar_tests/unarchive_existing_workspace.rs"]
mod unarchive_existing_workspace;
#[path = "sidebar_tests/unarchive_workspace_drafts.rs"]
mod unarchive_workspace_drafts;
#[path = "sidebar_tests/visible_entries_snapshot.rs"]
mod visible_entries_snapshot;
#[path = "sidebar_tests/workspace_lifecycle.rs"]
mod workspace_lifecycle;
#[path = "sidebar_tests/worktree_activation.rs"]
mod worktree_activation;
#[path = "sidebar_tests/worktree_chips.rs"]
mod worktree_chips;
#[path = "sidebar_tests/worktree_discovery.rs"]
mod worktree_discovery;
#[path = "sidebar_tests/worktree_info_unit.rs"]
mod worktree_info_unit;
#[path = "sidebar_tests/worktree_live_open.rs"]
mod worktree_live_open;
#[path = "sidebar_tests/worktree_reachability.rs"]
mod worktree_reachability;
#[path = "sidebar_tests/worktree_restore_git.rs"]
mod worktree_restore_git;
#[path = "sidebar_tests/worktree_restore_sidebar.rs"]
mod worktree_restore_sidebar;

#[path = "sidebar_tests/property_test.rs"]
mod property_test;

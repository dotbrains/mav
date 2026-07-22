// Allow blocking process commands in this binary - it's a synchronous test runner
#![allow(clippy::disallowed_methods)]

//! Visual Test Runner
//!
//! This binary runs visual regression tests for Mav's UI. It captures screenshots
//! of real Mav windows and compares them against baseline images.
//!
//! **Note: This tool is macOS-only** because it uses `VisualTestAppContext` which
//! depends on the macOS Metal renderer for accurate screenshot capture.
//!
//! ## How It Works
//!
//! This tool uses `VisualTestAppContext` which combines:
//! - Real Metal/compositor rendering for accurate screenshots
//! - Deterministic task scheduling via TestDispatcher
//! - Controllable time via `advance_clock` for testing time-based behaviors
//!
//! This approach:
//! - Does NOT require Screen Recording permission
//! - Does NOT require the window to be visible on screen
//! - Captures raw GPUI output without system window chrome
//! - Is fully deterministic - tooltips, animations, etc. work reliably
//!
//! ## Usage
//!
//! Run the visual tests:
//!   cargo run -p mav --bin mav_visual_test_runner --features visual-tests
//!
//! Update baseline images (when UI intentionally changes):
//!   UPDATE_BASELINE=1 cargo run -p mav --bin mav_visual_test_runner --features visual-tests
//!
//! ## Environment Variables
//!
//!   UPDATE_BASELINE - Set to update baseline images instead of comparing
//!   VISUAL_TEST_OUTPUT_DIR - Directory to save test output (default: target/visual_tests)

// Stub main for non-macOS platforms
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Visual test runner is only supported on macOS");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() {
    // Set MAV_STATELESS early to prevent file system access to real config directories
    // This must be done before any code accesses mav_env_vars::MAV_STATELESS
    // SAFETY: We're at the start of main(), before any threads are spawned
    unsafe {
        std::env::set_var("MAV_STATELESS", "1");
    }

    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let update_baseline = std::env::var("UPDATE_BASELINE").is_ok();

    // Create a temporary directory for test files
    // Canonicalize the path to resolve symlinks (on macOS, /var -> /private/var)
    // which prevents "path does not exist" errors during worktree scanning
    // Use keep() to prevent auto-cleanup - background worktree tasks may still be running
    // when tests complete, so we let the OS clean up temp directories on process exit
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let temp_path = temp_dir.keep();
    let canonical_temp = temp_path
        .canonicalize()
        .expect("Failed to canonicalize temp directory");
    let project_path = canonical_temp.join("project");
    std::fs::create_dir_all(&project_path).expect("Failed to create project directory");

    // Create test files in the real filesystem
    test_project::create_test_files(&project_path);

    let test_result = std::panic::catch_unwind(|| run_visual_tests(project_path, update_baseline));

    // Note: We don't delete temp_path here because background worktree tasks may still
    // be running. The directory will be cleaned up when the process exits or by the OS.

    match test_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            eprintln!("Visual tests failed: {}", e);
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("Visual tests panicked");
            std::process::exit(1);
        }
    }
}

// All macOS-specific imports grouped together
#[cfg(target_os = "macos")]
use {
    acp_thread::{AgentConnection, StubAgentConnection},
    agent_client_protocol::schema::v1 as acp,
    agent_servers::{AgentServer, AgentServerDelegate},
    anyhow::{Context as _, Result},
    assets::Assets,
    editor::display_map::DisplayRow,
    feature_flags::FeatureFlagAppExt as _,
    git_ui::project_diff::ProjectDiff,
    gpui::{
        App, AppContext as _, Bounds, Entity, KeyBinding, Modifiers, VisualTestAppContext,
        WindowBounds, WindowHandle, WindowOptions, point, px, size,
    },
    mav_actions::OpenSettingsAt,
    project::{AgentId, Project},
    project_panel::ProjectPanel,
    settings::{NotifyWhenAgentWaiting, PlaySoundWhenAgentDone, Settings as _},
    settings_ui::SettingsWindow,
    std::{
        any::Any,
        path::{Path, PathBuf},
        rc::Rc,
        sync::Arc,
        time::Duration,
    },
    util::ResultExt as _,
    workspace::{AppState, MultiWorkspace, Workspace},
};

// All macOS-specific constants grouped together
#[cfg(target_os = "macos")]
mod baselines;

mod constants {
    use std::time::Duration;

    /// Baseline images are stored relative to this file
    pub const BASELINE_DIR: &str = "crates/mav/test_fixtures/visual_tests";

    /// Embedded test image (Mav app icon) for visual tests.
    pub const EMBEDDED_TEST_IMAGE: &[u8] = include_bytes!("../resources/app-icon.png");

    /// Threshold for image comparison (0.0 to 1.0)
    /// Images must match at least this percentage to pass
    pub const MATCH_THRESHOLD: f64 = 0.99;

    /// Tooltip show delay - must match TOOLTIP_SHOW_DELAY in gpui/src/elements/div.rs
    pub const TOOLTIP_SHOW_DELAY: Duration = Duration::from_millis(500);
}

#[cfg(target_os = "macos")]
use baselines::{TestResult, run_visual_test};
use constants::*;

#[cfg(target_os = "macos")]
#[path = "visual_test_runner/agent_thread.rs"]
mod agent_thread;
#[cfg(target_os = "macos")]
#[path = "visual_test_runner/app_setup.rs"]
mod app_setup;
#[cfg(target_os = "macos")]
#[path = "visual_test_runner/breakpoint_hover.rs"]
mod breakpoint_hover;
#[cfg(target_os = "macos")]
#[path = "visual_test_runner/diff_review.rs"]
mod diff_review;
#[cfg(target_os = "macos")]
#[path = "visual_test_runner/error_wrapping.rs"]
mod error_wrapping;
#[cfg(target_os = "macos")]
#[path = "visual_test_runner/result_summary.rs"]
mod result_summary;
#[cfg(target_os = "macos")]
#[path = "visual_test_runner/settings_subpage.rs"]
mod settings_subpage;
#[cfg(target_os = "macos")]
#[path = "visual_test_runner/sidebar.rs"]
mod sidebar;
#[path = "visual_test_runner/sidebar_duplicate_projects.rs"]
mod sidebar_duplicate_projects;
#[cfg(target_os = "macos")]
#[path = "visual_test_runner/suite.rs"]
mod suite;
#[cfg(target_os = "macos")]
#[path = "visual_test_runner/test_project.rs"]
mod test_project;
#[cfg(target_os = "macos")]
#[path = "visual_test_runner/thread_item_branch.rs"]
mod thread_item_branch;
#[cfg(target_os = "macos")]
#[path = "visual_test_runner/thread_item_icons.rs"]
mod thread_item_icons;
#[cfg(target_os = "macos")]
#[path = "visual_test_runner/tool_permissions.rs"]
mod tool_permissions;
#[cfg(target_os = "macos")]
use agent_thread::run_agent_thread_view_test;
#[cfg(target_os = "macos")]
use breakpoint_hover::run_breakpoint_hover_visual_tests;
#[cfg(target_os = "macos")]
use diff_review::run_diff_review_visual_tests;
#[cfg(target_os = "macos")]
use error_wrapping::run_error_wrapping_visual_tests;
#[cfg(target_os = "macos")]
use settings_subpage::run_settings_ui_subpage_visual_tests;
#[cfg(target_os = "macos")]
use sidebar::run_sidebar_visual_tests;
use sidebar_duplicate_projects::run_sidebar_duplicate_project_names_visual_tests;
#[cfg(target_os = "macos")]
use suite::run_visual_tests;
#[cfg(target_os = "macos")]
use thread_item_branch::run_thread_item_branch_name_visual_tests;
#[cfg(target_os = "macos")]
use thread_item_icons::run_thread_item_icon_decorations_visual_tests;
#[cfg(target_os = "macos")]
use tool_permissions::run_tool_permissions_visual_tests;

mod app_menus;
pub mod edit_prediction_registry;
mod file_actions;
mod lifecycle_actions;
#[cfg(target_os = "macos")]
pub(crate) mod mac_only_instance;
mod migrate;
#[cfg(target_os = "macos")]
pub(crate) mod move_to_applications;
mod open_listener;
mod open_url_modal;
mod quick_action_bar;
pub mod remote_debug;
mod settings_files;
pub mod telemetry_log;
mod theme_loading;
mod workspace_actions;
mod workspace_environment;
mod workspace_initialization;
mod workspace_misc;
mod workspace_panels;
pub use settings_files::{handle_keymap_file_changes, watch_settings_files, watch_user_agents_md};
pub(crate) use theme_loading::eager_load_active_theme_and_icon_theme;
use workspace_actions::register_actions;
#[cfg(not(any(test, target_os = "macos")))]
use workspace_environment::initialize_file_watcher;
use workspace_environment::show_software_emulation_warning_if_needed;
pub use workspace_initialization::{build_window_options, initialize_workspace};
use workspace_misc::{init_cursor_hide_mode, open_log_file, open_new_ssh_project_from_project};
use workspace_panels::{initialize_pane, initialize_panels};
#[cfg(all(target_os = "macos", feature = "visual-tests"))]
pub mod visual_tests;
#[cfg(target_os = "windows")]
pub(crate) mod windows_only_instance;

use agent_settings::{UserAgentsMdState, init_user_agents_md};
use agent_ui::AgentDiffToolbar;
use anyhow::Context as _;
pub use app_menus::*;
use assets::Assets;

use breadcrumbs::Breadcrumbs;
use client::mav_urls;
use collections::VecDeque;
use debugger_ui::debugger_panel::DebugPanel;
use editor::{Editor, MultiBuffer};
use extension_host::ExtensionStore;
use feature_flags::{FeatureFlagAppExt as _, PanicFeatureFlag};
use file_actions::{
    open_bundled_file, open_project_debug_tasks_file, open_project_settings_file,
    open_project_tasks_file, open_settings_file,
};
use fs::Fs;
use futures::FutureExt as _;
use futures::{StreamExt, channel::mpsc, select_biased};
use git_ui::commit_view::CommitViewToolbar;
use git_ui::git_panel::GitPanel;
use git_ui::project_diff::{BranchDiffToolbar, ProjectDiffToolbar};
use git_ui::solo_diff_view::{SoloDiffGitToolbar, SoloDiffStyleToolbar};
use gpui::{
    Action, App, AppContext as _, AsyncWindowContext, ClipboardItem, Context, DismissEvent,
    Element, Entity, FocusHandle, Focusable, Image, ImageFormat, KeyBinding, ParentElement,
    PathPromptOptions, PromptLevel, ReadGlobal, SharedString, Size, Task, TaskExt, TitlebarOptions,
    UpdateGlobal, WeakEntity, Window, WindowBounds, WindowHandle, WindowKind, WindowOptions,
    actions, image_cache, img, point, px, retain_all,
};
use image_viewer::ImageInfo;
use language::Capability;
use language_onboarding::BasedPyrightBanner;
use language_tools::lsp_button::{self, LspButton};
use language_tools::lsp_log_view::LspLogToolbarItemView;
#[cfg(not(target_os = "windows"))]
use lifecycle_actions::install_cli;
use lifecycle_actions::{open_about_window, quit};
use markdown::{Markdown, MarkdownElement, MarkdownFont, MarkdownStyle};
use migrate::{MigrationBanner, MigrationEvent, MigrationNotification, MigrationType};
use migrator::migrate_keymap;
use onboarding::multibuffer_hint::MultibufferHint;
pub use open_listener::*;
use outline_panel::OutlinePanel;
use paths::{
    local_debug_file_relative_path, local_settings_file_relative_path,
    local_tasks_file_relative_path,
};
use project::{DirectoryLister, ProjectItem};
use project_panel::ProjectPanel;
use quick_action_bar::QuickActionBar;
use recent_projects::open_remote_project;
use release_channel::{AppCommitSha, AppVersion, ReleaseChannel};
use rope::Rope;
use search::project_search::ProjectSearchBar;
use settings::{
    BaseKeymap, DEFAULT_KEYMAP_PATH, DefaultOpenBehavior, InvalidSettingsError, KeybindSource,
    KeymapFile, KeymapFileLoadResult, MigrationStatus, SPECIFIC_OVERRIDES_KEYMAP_PATH, Settings,
    SettingsFile, SettingsStore, VIM_KEYMAP_PATH, initial_local_debug_tasks_content,
    initial_project_settings_content, initial_tasks_content, update_settings_file,
};
use sidebar::Sidebar;
#[cfg(debug_assertions)]
use workspace::workspace_error::{ErrorAction, ErrorSeverity, WorkspaceError};

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::Arc,
    sync::atomic::{self, AtomicBool},
};
use theme::{ActiveTheme, SystemAppearance, ThemeRegistry, deserialize_icon_theme};
use theme_settings::{ThemeSettings, load_user_theme};
use ui::{Navigable, NavigableEntry, PopoverMenuHandle, TintColor, prelude::*};
use util::markdown::MarkdownString;
use util::rel_path::RelPath;
use util::{ResultExt, asset_str, maybe};
use uuid::Uuid;
use vim_mode_setting::VimModeSetting;
use workspace::notifications::{NotificationId, dismiss_app_notification, show_app_notification};

use mav_actions::{
    About, OpenAccountSettings, OpenBrowser, OpenDocs, OpenMavUrl, OpenServerSettings,
    OpenSettingsFile, OpenStatusPage, Quit,
};
use workspace::{
    AppState, MultiWorkspace, NewFile, NewWindow, OpenLog, Toast, Workspace, WorkspaceSettings,
    create_and_open_local_file, notifications::simple_message_notification::MessageNotification,
    open_new,
};
use workspace::{
    CloseIntent, CloseProject, CloseWindow, RestoreBanner, with_active_or_new_workspace,
};
use workspace::{Pane, notifications::DetachAndPromptErr};

const DOCS_URL: &str = "https://mav.dev/docs/";
const STATUS_URL: &str = "https://status.mav.dev";

pub struct CrashHandler(pub Arc<crashes::Client>);

impl gpui::Global for CrashHandler {}

actions!(
    mav,
    [
        /// Opens the element inspector for debugging UI.
        DebugElements,
        /// Hides the application window.
        Hide,
        /// Hides all other application windows.
        HideOthers,
        /// Minimizes the current window.
        Minimize,
        /// Opens the default settings file.
        OpenDefaultSettings,
        /// Opens project-specific settings file.
        OpenProjectSettingsFile,
        /// Opens the project tasks configuration.
        OpenProjectTasks,
        /// Opens the tasks panel.
        OpenTasks,
        /// Opens debug tasks configuration.
        OpenDebugTasks,
        /// Shows the default semantic token rules (read-only).
        ShowDefaultSemanticTokenRules,
        /// Resets the application database.
        ResetDatabase,
        /// Shows all hidden windows.
        ShowAll,
        /// Toggles fullscreen mode.
        ToggleFullScreen,
        /// Zooms the window.
        Zoom,
        /// Triggers a test panic for debugging.
        TestPanic,
        /// Triggers a hard crash for debugging.
        TestCrash,
    ]
);

actions!(
    dev,
    [
        /// Opens a prompt to enter a URL to open.
        OpenUrlPrompt,
    ]
);

#[cfg(debug_assertions)]
actions!(
    dev,
    [
        /// Show an error on the workspace level.
        ShowWorkspaceError
    ]
);

pub fn init(cx: &mut App) {
    #[cfg(target_os = "macos")]
    cx.on_action(|_: &Hide, cx| cx.hide());
    #[cfg(target_os = "macos")]
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    #[cfg(target_os = "macos")]
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(quit);

    cx.on_action(|_: &RestoreBanner, cx| title_bar::restore_banner(cx));

    cx.observe_flag::<PanicFeatureFlag, _>({
        let mut added = false;
        move |flag, cx| {
            if added || !*flag {
                return;
            }
            added = true;
            cx.on_action(|_: &TestPanic, _| panic!("Ran the TestPanic action"))
                .on_action(|_: &TestCrash, _| {
                    unsafe extern "C" {
                        fn puts(s: *const i8);
                    }
                    unsafe {
                        puts(0xabad1d3a as *const i8);
                    }
                });
        }
    })
    .detach();

    // When Mav logs to stdout rather than the log file, avoid registering
    // handlers for both `OpenLog` and `RevealLogInFileManager`, as the log file
    // does not exist in that scenario and these actions would error.
    if !crate::stdout_is_a_pty() {
        cx.on_action(|_: &OpenLog, cx| {
            with_active_or_new_workspace(cx, |workspace, window, cx| {
                open_log_file(workspace, window, cx);
            });
        })
        .on_action(|_: &workspace::RevealLogInFileManager, cx| {
            cx.reveal_path(paths::log_file().as_path());
        });
    }

    cx.on_action(|_: &mav_actions::OpenLicenses, cx| {
        with_active_or_new_workspace(cx, |workspace, window, cx| {
            open_bundled_file(
                workspace,
                asset_str::<Assets>("licenses.md"),
                "Open Source License Attribution",
                "Markdown",
                window,
                cx,
            );
        });
    })
    .on_action(|&mav_actions::OpenKeymapFile, cx| {
        with_active_or_new_workspace(cx, |_, window, cx| {
            open_settings_file(
                paths::keymap_file(),
                || settings::initial_keymap_content().as_ref().into(),
                window,
                cx,
            );
        });
    })
    .on_action(|_: &OpenSettingsFile, cx| {
        with_active_or_new_workspace(cx, |_, window, cx| {
            open_settings_file(
                paths::settings_file(),
                || settings::initial_user_settings_content().as_ref().into(),
                window,
                cx,
            );
        });
    })
    .on_action(|_: &OpenAccountSettings, cx| {
        with_active_or_new_workspace(cx, |_, _, cx| {
            cx.open_url(&mav_urls::account_url(cx));
        });
    })
    .on_action(|_: &OpenTasks, cx| {
        with_active_or_new_workspace(cx, |_, window, cx| {
            open_settings_file(
                paths::tasks_file(),
                || settings::initial_tasks_content().as_ref().into(),
                window,
                cx,
            );
        });
    })
    .on_action(|_: &OpenDebugTasks, cx| {
        with_active_or_new_workspace(cx, |_, window, cx| {
            open_settings_file(
                paths::debug_scenarios_file(),
                || settings::initial_debug_tasks_content().as_ref().into(),
                window,
                cx,
            );
        });
    })
    .on_action(|_: &ShowDefaultSemanticTokenRules, cx| {
        with_active_or_new_workspace(cx, |workspace, window, cx| {
            open_bundled_file(
                workspace,
                settings::default_semantic_token_rules(),
                "Default Semantic Token Rules",
                "JSONC",
                window,
                cx,
            );
        });
    })
    .on_action(|_: &OpenDefaultSettings, cx| {
        with_active_or_new_workspace(cx, |workspace, window, cx| {
            open_bundled_file(
                workspace,
                settings::default_settings(),
                "Default Settings",
                "JSON",
                window,
                cx,
            );
        });
    })
    .on_action(|_: &mav_actions::OpenDefaultKeymap, cx| {
        with_active_or_new_workspace(cx, |workspace, window, cx| {
            open_bundled_file(
                workspace,
                settings::default_keymap(),
                "Default Key Bindings",
                "JSON",
                window,
                cx,
            );
        });
    })
    .on_action(|_: &About, cx| {
        open_about_window(cx);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::Assets;
    use collections::HashSet;
    use editor::{
        DisplayPoint, Editor, MultiBufferOffset, SelectionEffects, display_map::DisplayRow,
    };
    use gpui::{
        Action, AnyWindowHandle, App, AssetSource, BorrowAppContext, Modifiers, TestAppContext,
        UpdateGlobal, VisualTestContext, WindowHandle, actions, point, px,
    };
    use language::LanguageRegistry;
    use languages::{markdown_lang, rust_lang};
    use pretty_assertions::{assert_eq, assert_ne};
    use project::{Project, ProjectPath};
    use prompt_store::PromptBuilder;
    use semver::Version;
    use serde_json::json;
    use settings::{SaturatingBool, SettingsStore, watch_config_file};
    use std::{
        path::{Path, PathBuf},
        sync::Arc,
        time::Duration,
    };
    use theme::ThemeRegistry;
    use util::{
        path,
        rel_path::{RelPath, rel_path},
    };
    use workspace::MultiWorkspace;
    use workspace::{
        NewFile, OpenOptions, OpenVisible, SERIALIZATION_THROTTLE_TIME, SaveIntent, SplitDirection,
        WorkspaceHandle,
        item::SaveOptions,
        item::{Item, ItemHandle},
        open_new, open_paths, pane,
    };

    async fn flush_workspace_serialization(
        window: &WindowHandle<MultiWorkspace>,
        cx: &mut TestAppContext,
    ) {
        let all_tasks = window
            .update(cx, |multi_workspace, window, cx| {
                let mut tasks = multi_workspace
                    .workspaces()
                    .map(|workspace| {
                        workspace.update(cx, |workspace, cx| {
                            workspace.flush_serialization(window, cx)
                        })
                    })
                    .collect::<Vec<_>>();
                tasks.push(multi_workspace.flush_serialization());
                tasks
            })
            .unwrap();

        futures::future::join_all(all_tasks).await;
    }

    #[path = "open_path_tests.rs"]
    mod open_path_tests;

    #[path = "edit_state_tests.rs"]
    mod edit_state_tests;

    #[path = "workspace_open_tests.rs"]
    mod workspace_open_tests;

    #[path = "save_file_tests.rs"]
    mod save_file_tests;

    #[path = "pane_editor_tests.rs"]
    mod pane_editor_tests;

    #[path = "navigation_tests.rs"]
    mod navigation_tests;

    #[path = "reopen_tests.rs"]
    mod reopen_tests;

    #[path = "keymap_action_tests.rs"]
    mod keymap_action_tests;

    #[path = "bundled_file_tests.rs"]
    mod bundled_file_tests;

    #[path = "project_focus_tests.rs"]
    mod project_focus_tests;

    #[path = "best_workspace_tests.rs"]
    mod best_workspace_tests;

    #[path = "quit_dirty_tests.rs"]
    mod quit_dirty_tests;

    #[path = "session_restore_tests.rs"]
    mod session_restore_tests;

    #[path = "project_group_tests.rs"]
    mod project_group_tests;
}

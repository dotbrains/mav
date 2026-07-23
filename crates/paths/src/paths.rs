//! Paths to locations used by Mav.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, OnceLock};

pub use util::paths::home_dir;
use util::rel_path::RelPath;

/// A default editorconfig file name to use when resolving project settings.
pub const EDITORCONFIG_NAME: &str = ".editorconfig";

/// The application name, used to derive platform-specific data, config, cache,
/// and state directory paths.
///
/// Forks should change this to avoid colliding with Mav's user data.
pub const APP_NAME: &str = "mav";

/// Lowercased form of [`APP_NAME`], for use in XDG-style paths on
/// Linux/FreeBSD and the macOS `~/.config` fallback.
pub const APP_NAME_LOWERCASE: &str = {
    assert!(!APP_NAME.is_empty(), "APP_NAME must not be empty");
    assert!(APP_NAME.as_bytes().is_ascii(), "APP_NAME must be ASCII");
    const BYTES: [u8; APP_NAME.len()] = {
        let mut bytes = [0u8; APP_NAME.len()];
        let mut i = 0;
        while i < APP_NAME.len() {
            assert!(
                APP_NAME.as_bytes()[i] != b'/' && APP_NAME.as_bytes()[i] != b'\\',
                "APP_NAME must not contain path separators",
            );
            assert!(
                APP_NAME.as_bytes()[i] >= 0x20,
                "APP_NAME must not contain control characters"
            );
            bytes[i] = APP_NAME.as_bytes()[i];
            i += 1;
        }
        bytes.make_ascii_lowercase();
        bytes
    };
    match std::str::from_utf8(&BYTES) {
        Ok(s) => s,
        Err(_) => unreachable!(),
    }
};

#[path = "paths/base_dirs.rs"]
mod base_dirs;

pub use base_dirs::{
    config_dir, data_dir, remote_server_dir_relative, remote_wsl_server_dir_relative,
    set_custom_data_dir, state_dir, temp_dir,
};

/// Returns the path to the hang traces directory.
pub fn hang_traces_dir() -> &'static PathBuf {
    static LOGS_DIR: OnceLock<PathBuf> = OnceLock::new();
    LOGS_DIR.get_or_init(|| data_dir().join("hang_traces"))
}

/// Returns the path to the logs directory.
pub fn logs_dir() -> &'static PathBuf {
    static LOGS_DIR: OnceLock<PathBuf> = OnceLock::new();
    LOGS_DIR.get_or_init(|| {
        if cfg!(target_os = "macos") {
            home_dir().join("Library/Logs").join(APP_NAME)
        } else {
            data_dir().join("logs")
        }
    })
}

/// Returns the path to the Mav server directory on this SSH host.
pub fn remote_server_state_dir() -> &'static PathBuf {
    static REMOTE_SERVER_STATE: OnceLock<PathBuf> = OnceLock::new();
    REMOTE_SERVER_STATE.get_or_init(|| data_dir().join("server_state"))
}

/// Returns the path to the `Mav.log` file.
pub fn log_file() -> &'static PathBuf {
    static LOG_FILE: OnceLock<PathBuf> = OnceLock::new();
    LOG_FILE.get_or_init(|| logs_dir().join(format!("{}.log", APP_NAME)))
}

/// Returns the path to the `Mav.log.old` file.
pub fn old_log_file() -> &'static PathBuf {
    static OLD_LOG_FILE: OnceLock<PathBuf> = OnceLock::new();
    OLD_LOG_FILE.get_or_init(|| logs_dir().join(format!("{}.log.old", APP_NAME)))
}

/// Returns the path to the database directory.
pub fn database_dir() -> &'static PathBuf {
    static DATABASE_DIR: OnceLock<PathBuf> = OnceLock::new();
    DATABASE_DIR.get_or_init(|| data_dir().join("db"))
}

/// Returns the path to the crashes directory, if it exists for the current platform.
pub fn crashes_dir() -> &'static Option<PathBuf> {
    static CRASHES_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    CRASHES_DIR.get_or_init(|| {
        cfg!(target_os = "macos").then_some(home_dir().join("Library/Logs/DiagnosticReports"))
    })
}

/// Returns the path to the retired crashes directory, if it exists for the current platform.
pub fn crashes_retired_dir() -> &'static Option<PathBuf> {
    static CRASHES_RETIRED_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    CRASHES_RETIRED_DIR.get_or_init(|| crashes_dir().as_ref().map(|dir| dir.join("Retired")))
}

/// Returns the path to the `settings.json` file.
pub fn settings_file() -> &'static PathBuf {
    static SETTINGS_FILE: OnceLock<PathBuf> = OnceLock::new();
    SETTINGS_FILE.get_or_init(|| config_dir().join("settings.json"))
}

/// Returns the path to the global settings file.
pub fn global_settings_file() -> &'static PathBuf {
    static GLOBAL_SETTINGS_FILE: OnceLock<PathBuf> = OnceLock::new();
    GLOBAL_SETTINGS_FILE.get_or_init(|| config_dir().join("global_settings.json"))
}

/// Returns the path to the `settings_backup.json` file.
pub fn settings_backup_file() -> &'static PathBuf {
    static SETTINGS_FILE: OnceLock<PathBuf> = OnceLock::new();
    SETTINGS_FILE.get_or_init(|| config_dir().join("settings_backup.json"))
}

/// Returns the path to the `keymap.json` file.
pub fn keymap_file() -> &'static PathBuf {
    static KEYMAP_FILE: OnceLock<PathBuf> = OnceLock::new();
    KEYMAP_FILE.get_or_init(|| config_dir().join("keymap.json"))
}

/// Returns the path to the `keymap_backup.json` file.
pub fn keymap_backup_file() -> &'static PathBuf {
    static KEYMAP_FILE: OnceLock<PathBuf> = OnceLock::new();
    KEYMAP_FILE.get_or_init(|| config_dir().join("keymap_backup.json"))
}

/// Returns the path to the `tasks.json` file.
pub fn tasks_file() -> &'static PathBuf {
    static TASKS_FILE: OnceLock<PathBuf> = OnceLock::new();
    TASKS_FILE.get_or_init(|| config_dir().join("tasks.json"))
}

/// Returns the path to the `debug.json` file.
pub fn debug_scenarios_file() -> &'static PathBuf {
    static DEBUG_SCENARIOS_FILE: OnceLock<PathBuf> = OnceLock::new();
    DEBUG_SCENARIOS_FILE.get_or_init(|| config_dir().join("debug.json"))
}

/// Returns the path to the user-global `AGENTS.md` file.
///
/// This file holds personal agent instructions that apply to every project the
/// user opens, and is loaded into the native Mav agent's system prompt.
pub fn agents_file() -> &'static PathBuf {
    static AGENTS_FILE: OnceLock<PathBuf> = OnceLock::new();
    AGENTS_FILE.get_or_init(|| config_dir().join("AGENTS.md"))
}

/// User-facing display form of the user-global `AGENTS.md` file path —
/// i.e. what a human should see in messages and prompts, with the
/// platform's native path separator and home/config directory shorthand.
///
/// Windows doesn't recognize `~` as the home directory, so the env-var
/// form (`%APPDATA%`) is used there instead. Note that this is the
/// *typical* location: a user with `XDG_CONFIG_HOME` set or running in a
/// Flatpak sandbox would see a different `agents_file()` at runtime than
/// this displays. The display string trades that precision for
/// readability in announcement copy.
#[cfg(target_os = "windows")]
pub const GLOBAL_AGENTS_FILE_DISPLAY: &str =
    const_format::concatcp!("%APPDATA%\\", APP_NAME, "\\AGENTS.md");
#[cfg(not(target_os = "windows"))]
pub const GLOBAL_AGENTS_FILE_DISPLAY: &str =
    const_format::concatcp!("~/.config/", APP_NAME_LOWERCASE, "/AGENTS.md");

/// Returns the path to the extensions directory.
///
/// This is where installed extensions are stored.
pub fn extensions_dir() -> &'static PathBuf {
    static EXTENSIONS_DIR: OnceLock<PathBuf> = OnceLock::new();
    EXTENSIONS_DIR.get_or_init(|| data_dir().join("extensions"))
}

/// Returns the path to the extensions directory.
///
/// This is where installed extensions are stored on a remote.
pub fn remote_extensions_dir() -> &'static PathBuf {
    static EXTENSIONS_DIR: OnceLock<PathBuf> = OnceLock::new();
    EXTENSIONS_DIR.get_or_init(|| data_dir().join("remote_extensions"))
}

/// Returns the path to the extensions directory.
///
/// This is where installed extensions are stored on a remote.
pub fn remote_extensions_uploads_dir() -> &'static PathBuf {
    static UPLOAD_DIR: OnceLock<PathBuf> = OnceLock::new();
    UPLOAD_DIR.get_or_init(|| remote_extensions_dir().join("uploads"))
}

/// Returns the path to the themes directory.
///
/// This is where themes that are not provided by extensions are stored.
pub fn themes_dir() -> &'static PathBuf {
    static THEMES_DIR: OnceLock<PathBuf> = OnceLock::new();
    THEMES_DIR.get_or_init(|| config_dir().join("themes"))
}

/// Returns the path to the snippets directory.
pub fn snippets_dir() -> &'static PathBuf {
    static SNIPPETS_DIR: OnceLock<PathBuf> = OnceLock::new();
    SNIPPETS_DIR.get_or_init(|| config_dir().join("snippets"))
}

/// Returns the path to the contexts directory.
///
/// This is where the prompts for use with the Assistant are stored.
pub fn prompts_dir() -> &'static PathBuf {
    static PROMPTS_DIR: OnceLock<PathBuf> = OnceLock::new();
    PROMPTS_DIR.get_or_init(|| {
        if cfg!(target_os = "macos") {
            config_dir().join("prompts")
        } else {
            data_dir().join("prompts")
        }
    })
}

/// Returns the path to the prompt templates directory.
///
/// This is where the prompt templates for core features can be overridden with templates.
///
/// # Arguments
///
/// * `dev_mode` - If true, assumes the current working directory is the Mav repository.
pub fn prompt_overrides_dir(repo_path: Option<&Path>) -> PathBuf {
    if let Some(path) = repo_path {
        let dev_path = path.join("assets").join("prompts");
        if dev_path.exists() {
            return dev_path;
        }
    }

    static PROMPT_TEMPLATES_DIR: OnceLock<PathBuf> = OnceLock::new();
    PROMPT_TEMPLATES_DIR
        .get_or_init(|| {
            if cfg!(target_os = "macos") {
                config_dir().join("prompt_overrides")
            } else {
                data_dir().join("prompt_overrides")
            }
        })
        .clone()
}

/// Returns the path to the semantic search's embeddings directory.
///
/// This is where the embeddings used to power semantic search are stored.
pub fn embeddings_dir() -> &'static PathBuf {
    static EMBEDDINGS_DIR: OnceLock<PathBuf> = OnceLock::new();
    EMBEDDINGS_DIR.get_or_init(|| {
        if cfg!(target_os = "macos") {
            config_dir().join("embeddings")
        } else {
            data_dir().join("embeddings")
        }
    })
}

/// Returns the path to the languages directory.
///
/// This is where language servers are downloaded to for languages built-in to Mav.
pub fn languages_dir() -> &'static PathBuf {
    static LANGUAGES_DIR: OnceLock<PathBuf> = OnceLock::new();
    LANGUAGES_DIR.get_or_init(|| data_dir().join("languages"))
}

/// Returns the path to the debug adapters directory
///
/// This is where debug adapters are downloaded to for DAPs that are built-in to Mav.
pub fn debug_adapters_dir() -> &'static PathBuf {
    static DEBUG_ADAPTERS_DIR: OnceLock<PathBuf> = OnceLock::new();
    DEBUG_ADAPTERS_DIR.get_or_init(|| data_dir().join("debug_adapters"))
}

/// Returns the path to the external agents directory
///
/// This is where agent servers are downloaded to
pub fn external_agents_dir() -> &'static PathBuf {
    static EXTERNAL_AGENTS_DIR: OnceLock<PathBuf> = OnceLock::new();
    EXTERNAL_AGENTS_DIR.get_or_init(|| data_dir().join("external_agents"))
}

/// Returns the path to the Copilot directory.
pub fn copilot_dir() -> &'static PathBuf {
    static COPILOT_DIR: OnceLock<PathBuf> = OnceLock::new();
    COPILOT_DIR.get_or_init(|| data_dir().join("copilot"))
}

/// Returns the path to the default Prettier directory.
pub fn default_prettier_dir() -> &'static PathBuf {
    static DEFAULT_PRETTIER_DIR: OnceLock<PathBuf> = OnceLock::new();
    DEFAULT_PRETTIER_DIR.get_or_init(|| data_dir().join("prettier"))
}

/// Returns the path to the remote server binaries directory.
pub fn remote_servers_dir() -> &'static PathBuf {
    static REMOTE_SERVERS_DIR: OnceLock<PathBuf> = OnceLock::new();
    REMOTE_SERVERS_DIR.get_or_init(|| data_dir().join("remote_servers"))
}

/// Returns the path to the directory where the devcontainer CLI is installed.
pub fn devcontainer_dir() -> &'static PathBuf {
    static DEVCONTAINER_DIR: OnceLock<PathBuf> = OnceLock::new();
    DEVCONTAINER_DIR.get_or_init(|| data_dir().join("devcontainer"))
}

/// Returns the relative path to a `.mav` folder within a project.
pub fn local_settings_folder_name() -> &'static str {
    ".mav"
}

/// Returns the relative path to a `.vscode` folder within a project.
pub fn local_vscode_folder_name() -> &'static str {
    ".vscode"
}

/// Returns the relative path to a `settings.json` file within a project.
pub fn local_settings_file_relative_path() -> &'static RelPath {
    static CACHED: LazyLock<&'static RelPath> =
        LazyLock::new(|| RelPath::unix(".mav/settings.json").unwrap());
    *CACHED
}

/// Returns the relative path to a `tasks.json` file within a project.
pub fn local_tasks_file_relative_path() -> &'static RelPath {
    static CACHED: LazyLock<&'static RelPath> =
        LazyLock::new(|| RelPath::unix(".mav/tasks.json").unwrap());
    *CACHED
}

/// Returns the relative path to a `.vscode/tasks.json` file within a project.
pub fn local_vscode_tasks_file_relative_path() -> &'static RelPath {
    static CACHED: LazyLock<&'static RelPath> =
        LazyLock::new(|| RelPath::unix(".vscode/tasks.json").unwrap());
    *CACHED
}

pub fn debug_task_file_name() -> &'static str {
    "debug.json"
}

pub fn task_file_name() -> &'static str {
    "tasks.json"
}

/// Returns the relative path to a `debug.json` file within a project.
/// .mav/debug.json
pub fn local_debug_file_relative_path() -> &'static RelPath {
    static CACHED: LazyLock<&'static RelPath> =
        LazyLock::new(|| RelPath::unix(".mav/debug.json").unwrap());
    *CACHED
}

/// Returns the relative path to a `.vscode/launch.json` file within a project.
pub fn local_vscode_launch_file_relative_path() -> &'static RelPath {
    static CACHED: LazyLock<&'static RelPath> =
        LazyLock::new(|| RelPath::unix(".vscode/launch.json").unwrap());
    *CACHED
}

pub fn user_ssh_config_file() -> PathBuf {
    home_dir().join(".ssh/config")
}

pub fn global_ssh_config_file() -> Option<&'static Path> {
    if cfg!(windows) {
        None
    } else {
        Some(Path::new("/etc/ssh/ssh_config"))
    }
}

#[path = "paths/editor_imports.rs"]
mod editor_imports;

pub use editor_imports::{cursor_settings_file_paths, vscode_settings_file_paths};

#[cfg(any(test, feature = "test-support"))]
pub fn global_gitignore_path() -> Option<PathBuf> {
    Some(home_dir().join(".config").join("git").join("ignore"))
}

#[cfg(not(any(test, feature = "test-support")))]
pub fn global_gitignore_path() -> Option<PathBuf> {
    static GLOBAL_GITIGNORE_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    GLOBAL_GITIGNORE_PATH
        .get_or_init(::ignore::gitignore::gitconfig_excludes_path)
        .clone()
}

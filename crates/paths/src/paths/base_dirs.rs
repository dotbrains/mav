use super::*;
use util::paths::SanitizedPath;

/// A custom data directory override, set only by `set_custom_data_dir`.
/// This is used to override the default data directory location.
/// The directory will be created if it doesn't exist when set.
static CUSTOM_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The resolved data directory, combining custom override or platform defaults.
/// This is set once and cached for subsequent calls.
/// On macOS, this is `~/Library/Application Support/Mav`.
/// On Linux/FreeBSD, this is `$XDG_DATA_HOME/mav`.
/// On Windows, this is `%LOCALAPPDATA%\Mav`.
static CURRENT_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The resolved config directory, combining custom override or platform defaults.
/// This is set once and cached for subsequent calls.
/// On macOS, this is `~/.config/mav`.
/// On Linux/FreeBSD, this is `$XDG_CONFIG_HOME/mav`.
/// On Windows, this is `%APPDATA%\Mav`.
static CONFIG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Returns the relative path to the mav_server directory on the ssh host.
pub fn remote_server_dir_relative() -> &'static RelPath {
    static CACHED: LazyLock<&'static RelPath> =
        LazyLock::new(|| RelPath::unix(".mav_server").unwrap());
    *CACHED
}

// Remove this once 223 goes stable
/// Returns the relative path to the mav_wsl_server directory on the wsl host.
pub fn remote_wsl_server_dir_relative() -> &'static RelPath {
    static CACHED: LazyLock<&'static RelPath> =
        LazyLock::new(|| RelPath::unix(".mav_wsl_server").unwrap());
    *CACHED
}

/// Sets a custom directory for all user data, overriding the default data directory.
/// This function must be called before any other path operations that depend on the data directory.
/// The directory's path will be canonicalized to an absolute path by a blocking FS operation.
/// The directory will be created if it doesn't exist.
///
/// # Arguments
///
/// * `dir` - The path to use as the custom data directory. This will be used as the base
///   directory for all user data, including databases, extensions, and logs.
///
/// # Returns
///
/// A reference to the static `PathBuf` containing the custom data directory path.
///
/// # Panics
///
/// Panics if:
/// * Called after the data directory has been initialized (e.g., via `data_dir` or `config_dir`)
/// * The directory's path cannot be canonicalized to an absolute path
/// * The directory cannot be created
pub fn set_custom_data_dir(dir: &str) -> &'static PathBuf {
    if CURRENT_DATA_DIR.get().is_some() || CONFIG_DIR.get().is_some() {
        panic!("set_custom_data_dir called after data_dir or config_dir was initialized");
    }
    CUSTOM_DATA_DIR.get_or_init(|| {
        let path = PathBuf::from(dir);
        std::fs::create_dir_all(&path).expect("failed to create custom data directory");
        let canonicalized = path
            .canonicalize()
            .expect("failed to canonicalize custom data directory's path to an absolute path");
        // On Windows, `canonicalize` produces extended-length paths prefixed
        // with `\\?\`. Strip that prefix so downstream consumers (e.g.
        // Node.js language servers) that receive derived paths as arguments
        // don't choke on the verbatim syntax.
        SanitizedPath::new(&canonicalized).as_path().to_path_buf()
    })
}

/// Returns the path to the configuration directory used by Mav.
pub fn config_dir() -> &'static PathBuf {
    CONFIG_DIR.get_or_init(|| {
        if let Some(custom_dir) = CUSTOM_DATA_DIR.get() {
            custom_dir.join("config")
        } else if cfg!(target_os = "windows") {
            dirs::config_dir()
                .expect("failed to determine RoamingAppData directory")
                .join(APP_NAME)
        } else if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            if let Ok(flatpak_xdg_config) = std::env::var("FLATPAK_XDG_CONFIG_HOME") {
                flatpak_xdg_config.into()
            } else {
                dirs::config_dir().expect("failed to determine XDG_CONFIG_HOME directory")
            }
            .join(APP_NAME_LOWERCASE)
        } else {
            home_dir().join(".config").join(APP_NAME_LOWERCASE)
        }
    })
}

/// Returns the path to the data directory used by Mav.
pub fn data_dir() -> &'static PathBuf {
    CURRENT_DATA_DIR.get_or_init(|| {
        if let Some(custom_dir) = CUSTOM_DATA_DIR.get() {
            custom_dir.clone()
        } else if cfg!(target_os = "macos") {
            home_dir()
                .join("Library/Application Support")
                .join(APP_NAME)
        } else if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            if let Ok(flatpak_xdg_data) = std::env::var("FLATPAK_XDG_DATA_HOME") {
                flatpak_xdg_data.into()
            } else {
                dirs::data_local_dir().expect("failed to determine XDG_DATA_HOME directory")
            }
            .join(APP_NAME_LOWERCASE)
        } else if cfg!(target_os = "windows") {
            dirs::data_local_dir()
                .expect("failed to determine LocalAppData directory")
                .join(APP_NAME)
        } else {
            config_dir().clone() // Fallback
        }
    })
}

pub fn state_dir() -> &'static PathBuf {
    static STATE_DIR: OnceLock<PathBuf> = OnceLock::new();
    STATE_DIR.get_or_init(|| {
        if cfg!(target_os = "macos") {
            return home_dir().join(".local").join("state").join(APP_NAME);
        }

        if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            return if let Ok(flatpak_xdg_state) = std::env::var("FLATPAK_XDG_STATE_HOME") {
                flatpak_xdg_state.into()
            } else {
                dirs::state_dir().expect("failed to determine XDG_STATE_HOME directory")
            }
            .join(APP_NAME_LOWERCASE);
        } else {
            // Windows
            return dirs::data_local_dir()
                .expect("failed to determine LocalAppData directory")
                .join(APP_NAME);
        }
    })
}

/// Returns the path to the temp directory used by Mav.
pub fn temp_dir() -> &'static PathBuf {
    static TEMP_DIR: OnceLock<PathBuf> = OnceLock::new();
    TEMP_DIR.get_or_init(|| {
        if cfg!(target_os = "macos") {
            return dirs::cache_dir()
                .expect("failed to determine cachesDirectory directory")
                .join(APP_NAME);
        }

        if cfg!(target_os = "windows") {
            return dirs::cache_dir()
                .expect("failed to determine LocalAppData directory")
                .join(APP_NAME);
        }

        if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            return if let Ok(flatpak_xdg_cache) = std::env::var("FLATPAK_XDG_CACHE_HOME") {
                flatpak_xdg_cache.into()
            } else {
                dirs::cache_dir().expect("failed to determine XDG_CACHE_HOME directory")
            }
            .join(APP_NAME_LOWERCASE);
        }

        home_dir().join(".cache").join(APP_NAME_LOWERCASE)
    })
}

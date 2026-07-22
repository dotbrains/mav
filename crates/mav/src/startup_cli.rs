use super::*;

pub(crate) static FORCE_CLI_MODE: LazyLock<bool> = LazyLock::new(|| {
    let env_var = std::env::var(FORCE_CLI_MODE_ENV_VAR_NAME).ok().is_some();
    unsafe { std::env::remove_var(FORCE_CLI_MODE_ENV_VAR_NAME) };
    env_var
});

fn stdout_is_a_pty() -> bool {
    !*FORCE_CLI_MODE && io::stdout().is_terminal()
}

#[derive(Parser, Debug)]
#[command(name = "mav", disable_version_flag = true, max_term_width = 100)]
struct Args {
    /// A sequence of space-separated paths or urls that you want to open.
    ///
    /// Use `path:line:row` syntax to open a file at a specific location.
    /// Non-existing paths and directories will ignore `:line:row` suffix.
    ///
    /// URLs can either be `file://` or `mav://` scheme, or relative to <https://mav.dev>.
    paths_or_urls: Vec<String>,

    /// Pairs of file paths to diff. Can be specified multiple times.
    /// When directories are provided, recurses into them and shows all changed files in a single multi-diff view.
    #[arg(long, action = clap::ArgAction::Append, num_args = 2, value_names = ["OLD_PATH", "NEW_PATH"])]
    diff: Vec<String>,

    /// Sets a custom directory for all user data (e.g., database, extensions, logs).
    ///
    /// This overrides the default platform-specific data directory location.
    /// On macOS, the default is `~/Library/Application Support/Mav`.
    /// On Linux/FreeBSD, the default is `$XDG_DATA_HOME/mav`.
    /// On Windows, the default is `%LOCALAPPDATA%\Mav`.
    #[arg(long, value_name = "DIR", verbatim_doc_comment)]
    user_data_dir: Option<String>,

    /// The username and WSL distribution to use when opening paths. If not specified,
    /// Mav will attempt to open the paths directly.
    ///
    /// The username is optional, and if not specified, the default user for the distribution
    /// will be used.
    ///
    /// Example: `me@Ubuntu` or `Ubuntu`.
    ///
    /// WARN: You should not fill in this field by hand.
    #[cfg(target_os = "windows")]
    #[arg(long, value_name = "USER@DISTRO")]
    wsl: Option<String>,

    /// Open the project in a dev container.
    ///
    /// Automatically triggers "Reopen in Dev Container" if a `.devcontainer/`
    /// configuration is found in the project directory.
    #[arg(long)]
    dev_container: bool,

    /// Instructs mav to run as a dev server on this machine. (not implemented)
    #[arg(long)]
    dev_server_token: Option<String>,

    /// Prints system specs.
    ///
    /// Useful for submitting issues on GitHub when encountering a bug that
    /// prevents Mav from starting, so you can't run `mav: copy system specs to
    /// clipboard`
    #[arg(long)]
    system_specs: bool,

    /// Used for recording minidumps on crashes by having Mav run a separate
    /// process communicating over a socket.
    #[arg(long, hide = true)]
    crash_handler: Option<PathBuf>,

    /// Run mav in the foreground, only used on Windows, to match the behavior on macOS.
    #[arg(long)]
    #[cfg(target_os = "windows")]
    #[arg(hide = true)]
    foreground: bool,

    /// The dock action to perform. This is used on Windows only.
    #[arg(long)]
    #[cfg(target_os = "windows")]
    #[arg(hide = true)]
    dock_action: Option<usize>,

    /// Used for SSH/Git password authentication, to remove the need for netcat as a dependency,
    /// by having Mav act like netcat communicating over a Unix socket.
    #[arg(long)]
    #[cfg(not(target_os = "windows"))]
    #[arg(hide = true)]
    askpass: Option<String>,

    #[arg(long, hide = true)]
    dump_all_actions: bool,

    /// Output current environment variables as JSON to stdout
    #[arg(long, hide = true)]
    printenv: bool,

    /// Record an ETW trace. Must be run as administrator.
    #[cfg(target_os = "windows")]
    #[arg(long, hide = true)]
    record_etw_trace: bool,

    /// The PID of the Mav process to trace for heap analysis.
    #[cfg(target_os = "windows")]
    #[arg(long, hide = true, allow_hyphen_values = true)]
    etw_mav_pid: Option<i64>,

    /// Output path for the ETW trace file.
    #[cfg(target_os = "windows")]
    #[arg(long, hide = true)]
    etw_output: Option<PathBuf>,

    /// Unix socket path for IPC with the parent Mav process.
    #[cfg(target_os = "windows")]
    #[arg(long, hide = true)]
    etw_socket: Option<String>,
}

#[derive(Clone, Debug)]
enum IdType {
    New(String),
    Existing(String),
}

impl ToString for IdType {
    fn to_string(&self) -> String {
        match self {
            IdType::New(id) | IdType::Existing(id) => id.clone(),
        }
    }
}

fn parse_url_arg(arg: &str, cx: &App) -> String {
    match std::fs::canonicalize(Path::new(&arg)) {
        Ok(path) => format!("file://{}", path.display()),
        Err(_) => {
            if arg.starts_with("file://")
                || arg.starts_with("mav://")
                || arg.starts_with("mav-cli://")
                || arg.starts_with("ssh://")
                || parse_mav_link(arg, cx).is_some()
            {
                arg.into()
            } else {
                format!("file://{arg}")
            }
        }
    }
}

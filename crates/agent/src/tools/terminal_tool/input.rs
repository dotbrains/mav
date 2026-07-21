use super::output::TerminalOutputSelection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Executes a shell one-liner and returns the combined output.
///
/// This tool spawns a process using the user's shell, reads from stdout and stderr (preserving the order of writes), and returns a string with the combined output result.
///
/// The output results will be shown to the user already, only list it again if necessary, avoid being redundant.
///
/// Make sure you use the `cd` parameter to navigate to one of the root directories of the project. NEVER do it as part of the `command` itself, otherwise it will error.
///
/// Do not generate terminal commands that use shell substitutions or interpolations such as `$VAR`, `${VAR}`, `$(...)`, backticks, `$((...))`, `<(...)`, or `>(...)`. Resolve those values yourself before calling this tool, or ask the user for the literal value to use.
///
/// Do not pipe output to `head`, `tail`, or similar output-filtering commands just to reduce what you receive. Instead, use `head_lines` and/or `tail_lines`; this keeps the terminal output visible to the user in real time while limiting only the final output sent back to you. When both are specified, the first `head_lines` lines are returned, then a blank line, then the last `tail_lines` lines. Avoid requesting too many lines, or the response may waste tokens or exceed the context window.
///
/// Do not use this tool for commands that run indefinitely, such as servers (like `npm run start`, `npm run dev`, `python -m http.server`, etc) or file watchers that don't terminate on their own.
///
/// For potentially long-running commands, prefer specifying `timeout_ms` to bound runtime and prevent indefinite hangs.
///
/// Remember that each invocation of this tool will spawn a new shell process, so you can't rely on any state from previous invocations.
///
/// The terminal is an interactive pty, so any command that blocks waiting for input will hang the tool until it times out. To avoid this:
///
/// - Always insert `--no-pager` immediately after `git` for any read-only git command, including `git log`, `git diff`, `git show`, `git blame`, and `git stash show`. Example: `git --no-pager log -n 5` (NOT `git log -n 5`).
/// - Always prepend `GIT_EDITOR=true ` to any git command that may invoke an editor, including `git rebase`, `git commit`, `git merge`, and `git tag`. Example: `GIT_EDITOR=true git rebase origin/main` (NOT `git rebase origin/main`).
/// - For other commands that may open a pager or editor, set `PAGER=cat` and/or `EDITOR=true` similarly.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct TerminalToolInput {
    /// The one-liner command to execute. Do not include shell substitutions or interpolations such as `$VAR`, `${VAR}`, `$(...)`, backticks, `$((...))`, `<(...)`, or `>(...)`; resolve those values first or ask the user for the literal value to use.
    ///
    /// REMINDER: read-only git commands (`git log`, `git diff`, `git show`, `git blame`) MUST include `--no-pager` (e.g. `git --no-pager log`). Git commands that may open an editor (`git rebase`, `git commit`, `git merge`, `git tag`) MUST be prefixed with `GIT_EDITOR=true ` (e.g. `GIT_EDITOR=true git rebase origin/main`). Otherwise the terminal will hang.
    pub command: String,
    /// Working directory for the command. This must be one of the root directories of the project.
    pub cd: String,
    /// Optional maximum runtime (in milliseconds). If exceeded, the running terminal task is killed.
    pub timeout_ms: Option<u64>,
    /// Return only the first N lines of terminal output to the model after the command finishes. Do not pipe output to `head`; use this parameter instead so the user can still see live output. Avoid requesting too many lines, or the response may waste tokens or exceed the context window.
    #[serde(default)]
    pub head_lines: Option<usize>,
    /// Return only the last N lines of terminal output to the model after the command finishes. Do not pipe output to `tail`; use this parameter instead so the user can still see live output. Avoid requesting too many lines, or the response may waste tokens or exceed the context window.
    #[serde(default)]
    pub tail_lines: Option<usize>,
}

/// Executes a shell one-liner and returns the combined output.
///
/// This tool spawns a process using the user's shell, reads from stdout and stderr (preserving the order of writes), and returns a string with the combined output result.
///
/// The output results will be shown to the user already, only list it again if necessary, avoid being redundant.
///
/// Make sure you use the `cd` parameter to navigate to one of the root directories of the project. NEVER do it as part of the `command` itself, otherwise it will error.
///
/// Do not generate terminal commands that use shell substitutions or interpolations such as `$VAR`, `${VAR}`, `$(...)`, backticks, `$((...))`, `<(...)`, or `>(...)`. Resolve those values first or ask the user for the literal value to use.
///
/// Do not pipe output to `head`, `tail`, or similar output-filtering commands just to reduce what you receive. Instead, use `head_lines` and/or `tail_lines`; this keeps the terminal output visible to the user in real time while limiting only the final output sent back to you. When both are specified, the first `head_lines` lines are returned, then a blank line, then the last `tail_lines` lines. Avoid requesting too many lines, or the response may waste tokens or exceed the context window.
///
/// Do not use this tool for commands that run indefinitely, such as servers (like `npm run start`, `npm run dev`, `python -m http.server`, etc) or file watchers that don't terminate on their own.
///
/// For potentially long-running commands, prefer specifying `timeout_ms` to bound runtime and prevent indefinite hangs.
///
/// Remember that each invocation of this tool will spawn a new shell process, so you can't rely on any state from previous invocations.
///
/// The terminal is an interactive pty, so any command that blocks waiting for input will hang the tool until it times out. To avoid this:
///
/// - Always insert `--no-pager` immediately after `git` for any read-only git command, including `git log`, `git diff`, `git show`, `git blame`, and `git stash show`. Example: `git --no-pager log -n 5` (NOT `git log -n 5`).
/// - Always prepend `GIT_EDITOR=true ` to any git command that may invoke an editor, including `git rebase`, `git commit`, `git merge`, and `git tag`. Example: `GIT_EDITOR=true git rebase origin/main` (NOT `git rebase origin/main`).
/// - For other commands that may open a pager or editor, set `PAGER=cat` and/or `EDITOR=true` similarly.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct SandboxedTerminalToolInput {
    /// The one-liner command to execute. Do not include shell substitutions or interpolations such as `$VAR`, `${VAR}`, `$(...)`, backticks, `$((...))`, `<(...)`, or `>(...)`; resolve those values first or ask the user for the literal value to use.
    ///
    /// REMINDER: read-only git commands (`git log`, `git diff`, `git show`, `git blame`) MUST include `--no-pager` (e.g. `git --no-pager log`). Git commands that may open an editor (`git rebase`, `git commit`, `git merge`, `git tag`) MUST be prefixed with `GIT_EDITOR=true ` (e.g. `GIT_EDITOR=true git rebase origin/main`). Otherwise the terminal will hang.
    pub command: String,
    /// Working directory for the command. This must be one of the root directories of the project.
    pub cd: String,
    /// Optional maximum runtime (in milliseconds). If exceeded, the running terminal task is killed.
    pub timeout_ms: Option<u64>,
    /// Return only the first N lines of terminal output to the model after the command finishes. Do not pipe output to `head`; use this parameter instead so the user can still see live output. Avoid requesting too many lines, or the response may waste tokens or exceed the context window.
    #[serde(default)]
    pub head_lines: Option<usize>,
    /// Return only the last N lines of terminal output to the model after the command finishes. Do not pipe output to `tail`; use this parameter instead so the user can still see live output. Avoid requesting too many lines, or the response may waste tokens or exceed the context window.
    #[serde(default)]
    pub tail_lines: Option<usize>,
    /// Hosts the command needs outbound network access to.
    ///
    /// Sandboxed commands cannot reach the network by default. List the hosts
    /// the command needs (e.g. `["github.com", "*.npmjs.org"]`) when running
    /// commands that fetch or upload (installing dependencies, cloning,
    /// pushing, downloading, etc.). Each entry must be a hostname or a
    /// leading-`*.` subdomain wildcard; IP literals and other wildcards are
    /// rejected. Requesting network access triggers a user approval prompt, so
    /// only list hosts you expect the command to need.
    #[cfg_attr(
        any(target_os = "macos", target_os = "linux"),
        doc = "\nHost-specific access is enforced by an HTTP/HTTPS proxy, so use \
        `https://` URLs rather than `git@`/`ssh://`."
    )]
    #[cfg_attr(
        target_os = "windows",
        doc = "\nNOTE: on Windows the sandbox cannot currently restrict network \
        access to specific hosts. Do not set `allow_hosts` on Windows; request \
        `allow_all_hosts: true` if the command needs network access, or omit \
        network permissions entirely."
    )]
    #[serde(default)]
    pub allow_hosts: Vec<String>,
    /// Set to `true` only if the command needs outbound network access to
    /// hosts you can't enumerate up front.
    ///
    /// This grants unrestricted outbound network access. Prefer `allow_hosts`
    /// with specific hostnames whenever possible, so the user knows what's
    /// being approved. Requesting it triggers a user approval prompt.
    #[serde(default)]
    pub allow_all_hosts: Option<bool>,
    /// Paths the command needs to write to outside the default-writable
    /// locations.
    ///
    #[cfg_attr(
        target_os = "macos",
        doc = "Sandboxed commands can already write to the project worktree \
        directories and a per-command temporary directory, so only list paths \
        outside those."
    )]
    /// Provide absolute or worktree-relative paths; each
    /// directory grants write access to its whole subtree. Prefer this over
    /// `allow_fs_write_all` whenever you can enumerate the paths. Requesting
    /// paths triggers a user approval prompt.
    #[cfg_attr(
        target_os = "linux",
        doc = "\nOn Linux, every path here must be a directory that already exists. \
        Requesting a file, or a path that does not exist yet, is an error. To create new \
        files, request write access to the existing directory that will contain them."
    )]
    #[serde(default)]
    pub fs_write_paths: Vec<String>,
    /// Set to `true` only when the command needs to write outside the
    /// default-writable locations but the specific paths cannot be
    /// enumerated up front.
    ///
    /// This is a broad escape hatch — prefer `fs_write_paths` whenever the
    /// set of paths is known. Requesting it triggers a user approval prompt.
    #[serde(default, alias = "allow_fs_write")]
    pub allow_fs_write_all: Option<bool>,
    /// Set to `true` when the command needs access to protected Git metadata.
    ///
    /// By default sandboxed commands can't write to the `.git` directories of
    /// opened worktrees and discovered repositories. On macOS, `.git` file
    /// contents are also hidden while metadata stays visible; on Linux and
    /// Windows/WSL, `.git` contents remain readable but are mounted read-only.
    /// Set this for Git operations that need to write those paths (commit,
    /// fetch, rebase, …). Requesting it triggers a user approval prompt.
    #[serde(default)]
    pub allow_git_access: Option<bool>,
    /// Set to `true` only as a last resort, to run the command fully outside
    /// the sandbox.
    ///
    /// First try the narrower options (`allow_hosts`, `fs_write_paths`,
    /// `allow_fs_write_all`, `allow_git_access`); use this only when the command
    /// needs behavior the sandbox can't grant on a per-permission basis.
    /// Requesting it triggers a user approval prompt.
    #[cfg_attr(
        target_os = "windows",
        doc = "\nOn Windows, running unsandboxed also switches the shell. Sandboxed \
        commands run under WSL's Linux bash; an unsandboxed command instead runs in the \
        host's default shell — Git Bash (or scoop's bash) when one is installed, otherwise \
        PowerShell/cmd. Path conventions change accordingly (e.g. `C:\\...` or `/c/...` \
        rather than WSL's `/mnt/c/...`), so a command written for the sandboxed shell may \
        behave differently here."
    )]
    #[serde(default)]
    pub unsandboxed: Option<bool>,
    /// A short justification for why this command needs the sandbox
    /// permission(s) it requests (`allow_network`, `fs_write_paths`,
    /// `allow_fs_write_all`, or `unsandboxed`).
    ///
    /// Required whenever you request any of those permissions; omit it for
    /// ordinary commands that request none. Write it in your own voice — it
    /// is shown to the user, attributed to you, when they're asked to approve
    /// the request.
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct TerminalSandboxInput {
    pub(super) allow_hosts: Vec<String>,
    pub(super) allow_all_hosts: Option<bool>,
    pub(super) fs_write_paths: Vec<String>,
    pub(super) allow_fs_write_all: Option<bool>,
    pub(super) allow_git_access: Option<bool>,
    pub(super) unsandboxed: Option<bool>,
    pub(super) reason: Option<String>,
}

pub(super) struct TerminalToolRequest {
    pub(super) command: String,
    pub(super) cd: String,
    pub(super) timeout_ms: Option<u64>,
    pub(super) selection: TerminalOutputSelection,
    pub(super) sandbox: Option<TerminalSandboxInput>,
}

impl From<TerminalToolInput> for TerminalToolRequest {
    fn from(input: TerminalToolInput) -> Self {
        Self {
            command: input.command,
            cd: input.cd,
            timeout_ms: input.timeout_ms,
            selection: TerminalOutputSelection {
                head_lines: input.head_lines,
                tail_lines: input.tail_lines,
            },
            sandbox: None,
        }
    }
}

impl From<SandboxedTerminalToolInput> for TerminalToolRequest {
    fn from(input: SandboxedTerminalToolInput) -> Self {
        Self {
            command: input.command,
            cd: input.cd,
            timeout_ms: input.timeout_ms,
            selection: TerminalOutputSelection {
                head_lines: input.head_lines,
                tail_lines: input.tail_lines,
            },
            sandbox: Some(TerminalSandboxInput {
                allow_hosts: input.allow_hosts,
                allow_all_hosts: input.allow_all_hosts,
                fs_write_paths: input.fs_write_paths,
                allow_fs_write_all: input.allow_fs_write_all,
                allow_git_access: input.allow_git_access,
                unsandboxed: input.unsandboxed,
                reason: input.reason,
            }),
        }
    }
}

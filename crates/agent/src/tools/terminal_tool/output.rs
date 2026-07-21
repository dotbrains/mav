use super::*;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TerminalOutputSelection {
    pub(super) head_lines: Option<usize>,
    pub(super) tail_lines: Option<usize>,
}

impl TerminalOutputSelection {
    pub(super) fn is_enabled(self) -> bool {
        self.head_lines.is_some() || self.tail_lines.is_some()
    }
}

pub(super) fn select_terminal_output_lines(
    output: &str,
    selection: TerminalOutputSelection,
) -> String {
    match (selection.head_lines, selection.tail_lines) {
        (None, None) => output.to_string(),
        (Some(head_lines), None) => output
            .lines()
            .take(head_lines)
            .collect::<Vec<_>>()
            .join("\n"),
        (None, Some(tail_lines)) => {
            let lines = output.lines().collect::<Vec<_>>();
            let start = lines.len().saturating_sub(tail_lines);
            lines[start..].join("\n")
        }
        (Some(head_lines), Some(tail_lines)) => {
            let lines = output.lines().collect::<Vec<_>>();
            let head = lines
                .iter()
                .take(head_lines)
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            let tail_start = lines.len().saturating_sub(tail_lines);
            let tail = lines[tail_start..].join("\n");
            format!("{head}\n\n{tail}")
        }
    }
}

/// Explanation appended to the model-facing result when a sandboxed command
/// fails because it tried to use WSL's Windows interop (see
/// [`wsl_interop_blocked`]).
const WSL_INTEROP_BLOCKED_NOTE: &str = "This command tried to launch a Windows \
executable, which the sandbox blocks: WSL Windows interop is disabled so \
sandboxed commands can't escape to the Windows host. The noisy `WSL ... ERROR` \
lines below are from that blocked attempt, not a bug in the command. If you \
genuinely need to run a Windows program, re-run with `unsandboxed: true`.";

/// Whether terminal output contains the kernel-style diagnostics WSL prints
/// when a Windows executable is launched inside our pid-namespaced sandbox
/// (interop init fails to parse `/proc/1/stat`, which is now `bwrap`). These
/// markers don't appear for ordinary Linux commands.
#[cfg(target_os = "windows")]
fn wsl_interop_blocked(content: &str) -> bool {
    content.contains("UtilGetPpid") || content.contains("Failed to parse: /proc/1/stat")
}

pub(super) fn process_content(
    output: acp::TerminalOutputResponse,
    command: &str,
    timed_out: bool,
    user_stopped: bool,
    selection: TerminalOutputSelection,
) -> String {
    let content = output.output.trim();
    let content = select_terminal_output_lines(content, selection);
    let is_empty = content.is_empty();

    // On Windows, recognize the kernel-style diagnostics WSL prints when a
    // command tries to launch a Windows executable inside the sandbox (where
    // interop is deliberately disabled). They're noise the model can't act on,
    // so we explain what actually happened.
    #[cfg(target_os = "windows")]
    let interop_blocked = wsl_interop_blocked(&content);
    #[cfg(not(target_os = "windows"))]
    let interop_blocked = false;

    let content = format!("```\n{content}\n```");
    let content = if output.truncated {
        format!(
            "Command output too long. The first {} bytes:\n\n{content}",
            content.len(),
        )
    } else {
        content
    };

    let content = if user_stopped {
        if is_empty {
            "The user stopped this command. No output was captured before stopping.\n\n\
            Since the user intentionally interrupted this command, ask them what they would like to do next \
            rather than automatically retrying or assuming something went wrong.".to_string()
        } else {
            format!(
                "The user stopped this command. Output captured before stopping:\n\n{}\n\n\
                Since the user intentionally interrupted this command, ask them what they would like to do next \
                rather than automatically retrying or assuming something went wrong.",
                content
            )
        }
    } else if timed_out {
        if is_empty {
            format!("Command \"{command}\" timed out. No output was captured.")
        } else {
            format!(
                "Command \"{command}\" timed out. Output captured before timeout:\n\n{}",
                content
            )
        }
    } else {
        let exit_code = output.exit_status.as_ref().and_then(|s| s.exit_code);
        match exit_code {
            Some(0) => {
                if is_empty {
                    "Command executed successfully.".to_string()
                } else {
                    content
                }
            }
            Some(exit_code) if interop_blocked => {
                format!(
                    "Command \"{command}\" failed with exit code {exit_code}. {WSL_INTEROP_BLOCKED_NOTE}\n\n{content}"
                )
            }
            Some(exit_code) => {
                if is_empty {
                    format!("Command \"{command}\" failed with exit code {}.", exit_code)
                } else {
                    format!(
                        "Command \"{command}\" failed with exit code {}.\n\n{content}",
                        exit_code
                    )
                }
            }
            None => {
                if is_empty {
                    "Command terminated unexpectedly. No output was captured.".to_string()
                } else {
                    format!(
                        "Command terminated unexpectedly. Output captured:\n\n{}",
                        content
                    )
                }
            }
        }
    };
    content
}

use super::*;

/// Returns the leading native slash command name if `text` begins with one. A
/// native command is handled by the host instead of being echoed as a user
/// message.
pub(super) fn leading_native_command(
    text: &str,
    available_commands: &[acp::AvailableCommand],
) -> Option<String> {
    let rest = text.trim_start().strip_prefix('/')?;
    let name_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let name = &rest[..name_end];
    let is_native = available_commands.iter().any(|command| {
        command.name == name
            && acp_thread::command_category_from_meta(&command.meta)
                == Some(acp_thread::CommandCategory::Native)
    });
    is_native.then(|| name.to_string())
}

/// Removes a leading `/command_name` token from `text`, returning the trimmed
/// remainder. Falls back to the trimmed input if the prefix isn't present.
pub(super) fn strip_leading_command(text: &str, command_name: &str) -> String {
    let trimmed = text.trim_start();
    trimmed
        .strip_prefix('/')
        .and_then(|rest| rest.strip_prefix(command_name))
        .map(|rest| rest.trim_start().to_string())
        .unwrap_or_else(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native_command(name: &str) -> acp::AvailableCommand {
        acp::AvailableCommand::new(name, "").meta(acp_thread::meta_with_command_category(
            acp_thread::CommandCategory::Native,
        ))
    }

    fn mcp_command(name: &str) -> acp::AvailableCommand {
        acp::AvailableCommand::new(name, "").meta(acp_thread::meta_with_command_category(
            acp_thread::CommandCategory::Mcp,
        ))
    }

    #[test]
    fn test_leading_native_command_matches_bare_and_with_remainder() {
        let commands = [native_command("compact"), mcp_command("deploy")];

        assert_eq!(
            leading_native_command("/compact summarize the API work", &commands),
            Some("compact".to_string())
        );
        assert_eq!(
            leading_native_command("  /compact   do x  ", &commands),
            Some("compact".to_string())
        );
        assert_eq!(
            leading_native_command("/compact", &commands),
            Some("compact".to_string())
        );
        assert_eq!(
            leading_native_command("/compact   ", &commands),
            Some("compact".to_string())
        );
        assert_eq!(leading_native_command("/deploy prod", &commands), None);
        assert_eq!(leading_native_command("/deploy", &commands), None);
        assert_eq!(leading_native_command("/unknown foo", &commands), None);
        assert_eq!(leading_native_command("just a message", &commands), None);
    }

    #[test]
    fn test_strip_leading_command() {
        assert_eq!(strip_leading_command("/compact do x", "compact"), "do x");
        assert_eq!(
            strip_leading_command("  /compact  do x ", "compact"),
            "do x "
        );
        assert_eq!(strip_leading_command("hello", "compact"), "hello");
    }
}

use super::*;

pub(super) struct Command<'a> {
    pub(super) prompt_name: &'a str,
    pub(super) arg_value: &'a str,
    /// MCP server prefix from `/<server>.<prompt>` syntax. Mutually
    /// exclusive with `skill_scope` — the two grammars use different
    /// delimiters (`.` for MCP, `:` for skill scopes) so they can't
    /// collide.
    pub(super) explicit_server_id: Option<&'a str>,
    /// Skill scope qualifier from `/<scope>:<name>` syntax, where
    /// `<scope>` is either the literal `global` or a worktree root
    /// name. The `:` separator namespaces these against MCP server
    /// prefixes (which use `.`) so an MCP server literally named
    /// `global` or named after a worktree still parses unambiguously.
    pub(super) skill_scope: Option<&'a str>,
}

impl<'a> Command<'a> {
    pub(super) fn is_unqualified(&self, prompt_name: &str) -> bool {
        self.prompt_name == prompt_name
            && self.explicit_server_id.is_none()
            && self.skill_scope.is_none()
    }

    pub(super) fn parse(prompt: &'a [acp::ContentBlock]) -> Option<Self> {
        let acp::ContentBlock::Text(text_content) = prompt.first()? else {
            return None;
        };
        let text = text_content.text.trim();
        let command = text.strip_prefix('/')?;
        let (command, arg_value) = command
            .split_once(char::is_whitespace)
            .unwrap_or((command, ""));

        // Skill scope qualifier: `/<scope>:<name>`. Checked before the
        // MCP `.` grammar because `:` and `.` are different delimiters
        // — the two namespaces can't collide. Skill names are
        // restricted to `[a-z0-9-]+` (no colons), so the LAST `:` is
        // always the scope/name boundary; using `rsplit_once` lets
        // scope labels (e.g. a worktree root name) themselves contain
        // colons without breaking the parse.
        //
        // An empty scope (`/:<name>`) is the qualified form for a
        // global skill — see `SkillSource::scope_prefix`. The name
        // must be non-empty for the colon to be meaningful.
        if let Some((scope, prompt_name)) = command.rsplit_once(':')
            && !prompt_name.is_empty()
        {
            return Some(Self {
                prompt_name,
                arg_value,
                explicit_server_id: None,
                skill_scope: Some(scope),
            });
        }

        if let Some((server_id, prompt_name)) = command.split_once('.') {
            Some(Self {
                prompt_name,
                arg_value,
                explicit_server_id: Some(server_id),
                skill_scope: None,
            })
        } else {
            Some(Self {
                prompt_name: command,
                arg_value,
                explicit_server_id: None,
                skill_scope: None,
            })
        }
    }
}

/// Strip a leading `/cmd` slash command from the start of a text block,
/// returning whatever text comes after it. Mirrors the parsing in
/// [`Command::parse`]: leading whitespace is ignored when locating the `/`,
/// then everything up to (and including) the first whitespace inside the
/// stripped text is dropped. The remainder is preserved verbatim — including
/// any embedded newlines — because users may format their continuation
/// intentionally.
///
/// If the input doesn't begin with `/`, it is returned unchanged so callers
/// degrade gracefully rather than silently mangling unrelated text.
pub(super) fn strip_slash_command_prefix(text: &str) -> String {
    let trimmed_start = text.trim_start();
    let Some(rest) = trimmed_start.strip_prefix('/') else {
        return text.to_string();
    };
    rest.split_once(char::is_whitespace)
        .map(|(_, after)| after.to_string())
        .unwrap_or_default()
}

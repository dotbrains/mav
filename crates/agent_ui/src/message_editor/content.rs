use super::*;

impl MessageEditor {
    fn validate_slash_commands(
        text: &str,
        available_commands: &[acp::AvailableCommand],
        available_skills: &[AvailableSkill],
        agent_id: &AgentId,
    ) -> Result<()> {
        if let Some(parsed_command) = SlashCommandCompletion::try_parse(text, 0) {
            if parsed_command.source_range.start != 0 {
                return Ok(());
            }
            if let Some(command_name) = parsed_command.command {
                // Two acceptance paths:
                //
                // 1. Direct name match. Covers bare slash commands
                //    (`/help`), MCP prompts that were prefixed at the
                //    agent because of a server-name collision
                //    (`/github.create_pr`), and skills (whose bare name
                //    is registered for the unqualified `/<name>` form).
                //
                // 2. Trusted native skill scope qualifier `/<scope>:<name>`. The popup
                //    inserts this colon-separated form to disambiguate
                //    same-named skills, so the validator splits on the
                //    LAST `:` to recover scope + bare name. Skill
                //    names are restricted to `[a-z0-9-]+` (no colons),
                //    so the rightmost colon is always the scope/name
                //    boundary — this lets scope labels (e.g. worktree
                //    root names) themselves contain colons. The
                //    scope is allowed to be empty: `/:<name>` is the
                //    qualified form for a global skill (see
                //    `SkillSource::scope_prefix`). The validator then
                //    checks the `available_skills` slice for an entry
                //    whose `skill.name` matches the bare name and
                //    whose `skill.source` equals the typed scope
                //    (including empty for globals). Without this
                //    branch, every autocomplete pick of a same-named
                //    skill would be rejected as "not supported"
                //    before reaching the resolver.
                let direct_match = available_commands
                    .iter()
                    .any(|available_command| available_command.name == command_name)
                    || available_skills
                        .iter()
                        .any(|skill| skill.name.as_ref() == command_name);
                let scope_match = !direct_match
                    && command_name.rsplit_once(':').is_some_and(|(scope, bare)| {
                        !bare.is_empty()
                            && available_skills.iter().any(|skill| {
                                skill.name.as_ref() == bare && skill.source.as_ref() == scope
                            })
                    });

                if !direct_match && !scope_match {
                    return Err(anyhow!(indoc::formatdoc!(
                        "/{command_name} is not a recognized command in {agent_id}. \
                         Messages that start with `/` are interpreted as commands.

                         If you are trying to send a message and not run a command, \
                         try preceding the `/` with a space.

                         Available commands for {agent_id}: {commands}",
                        commands =
                            Self::format_available_commands(available_commands, available_skills),
                    )));
                }
            }
        }
        Ok(())
    }

    /// Render the available-commands list for error messages. Trusted native skills
    /// are shown in their qualified `/<scope>:<name>` form so users
    /// see the exact text the popup would insert — otherwise the
    /// listing would contain confusing duplicates like `/foo, /foo`
    /// when both a global and a project-local skill share a name.
    /// Globals carry an empty scope and so render as `/:<name>`.
    fn format_available_commands(
        commands: &[acp::AvailableCommand],
        skills: &[AvailableSkill],
    ) -> String {
        if commands.is_empty() && skills.is_empty() {
            return "none".to_string();
        }
        skills
            .iter()
            .map(|skill| format!("/{}:{}", skill.source, skill.name))
            .chain(commands.iter().map(|command| format!("/{}", command.name)))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn contents(
        &self,
        full_mention_content: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<(Vec<acp::ContentBlock>, Vec<Entity<Buffer>>)>> {
        let text = self.editor.read(cx).text(cx);
        let (available_commands, available_skills) = {
            let session_capabilities = self.session_capabilities.read();
            (
                session_capabilities.available_commands().to_vec(),
                session_capabilities.available_skills().to_vec(),
            )
        };
        let agent_id = self.agent_id.clone();
        let build_task = self.build_content_blocks(full_mention_content, cx);

        cx.spawn(async move |_, _cx| {
            Self::validate_slash_commands(
                &text,
                &available_commands,
                &available_skills,
                &agent_id,
            )?;
            build_task.await
        })
    }

    pub fn draft_contents(&self, cx: &mut Context<Self>) -> Task<Result<Vec<acp::ContentBlock>>> {
        let build_task = self.build_content_blocks(false, cx);
        cx.spawn(async move |_, _cx| {
            let (blocks, _tracked_buffers) = build_task.await?;
            Ok(blocks)
        })
    }

    fn build_content_blocks(
        &self,
        full_mention_content: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<(Vec<acp::ContentBlock>, Vec<Entity<Buffer>>)>> {
        let contents = self
            .mention_set
            .update(cx, |store, cx| store.contents(full_mention_content, cx));
        let editor = self.editor.clone();
        let supports_embedded_context =
            self.session_capabilities.read().supports_embedded_context();

        cx.spawn(async move |_, cx| {
            let mut contents = contents.await?;
            Ok(editor.update(cx, |editor, cx| {
                let crease_snapshot = editor.display_map.read(cx).crease_snapshot();
                let buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
                let text = editor.text(cx);
                build_chunks_from_creases(
                    &text,
                    &crease_snapshot,
                    &buffer_snapshot,
                    supports_embedded_context,
                    |crease_id| {
                        contents
                            .remove(crease_id)
                            .map(|(uri, mention)| (uri, Some(mention)))
                    },
                )
            }))
        })
    }

    /// Snapshots the editor's current draft into a list of `ContentBlock`s
    /// without awaiting any pending mention resolution.
    pub fn draft_content_blocks_snapshot(&self, cx: &App) -> Vec<acp::ContentBlock> {
        let editor = self.editor.read(cx);
        let crease_snapshot = editor.display_map.read(cx).crease_snapshot();
        let buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
        let text = editor.text(cx);
        let mention_set = self.mention_set.read(cx);
        let supports_embedded_context =
            self.session_capabilities.read().supports_embedded_context();
        let (chunks, _tracked_buffers) = build_chunks_from_creases(
            &text,
            &crease_snapshot,
            &buffer_snapshot,
            supports_embedded_context,
            |crease_id| mention_set.resolved_mention_for_crease(crease_id),
        );
        chunks
    }
}

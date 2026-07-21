#[test]
fn test_terminal_tool_description_mentions_forbidden_substitutions() {
        let description = <TerminalTool as crate::AgentTool>::description().to_string();

        assert!(
            description.contains("$VAR"),
            "missing $VAR example: {description}"
        );
        assert!(
            description.contains("${VAR}"),
            "missing ${{VAR}} example: {description}"
        );
        assert!(
            description.contains("$(...)"),
            "missing $(...) example: {description}"
        );
        assert!(
            description.contains("backticks"),
            "missing backticks example: {description}"
        );
        assert!(
            description.contains("$((...))"),
            "missing $((...)) example: {description}"
        );
        assert!(
            description.contains("<(...)") && description.contains(">(...)"),
            "missing process substitution examples: {description}"
        );
    }

    #[test]
    fn test_terminal_tool_input_schema_mentions_forbidden_substitutions() {
        let schema = <TerminalTool as crate::AgentTool>::input_schema(
            language_model::LanguageModelToolSchemaFormat::JsonSchema,
        );
        let schema_json = serde_json::to_value(schema).expect("schema should serialize");
        let schema_text = schema_json.to_string();

        assert!(
            schema_text.contains("$VAR"),
            "missing $VAR example: {schema_text}"
        );
        assert!(
            schema_text.contains("${VAR}"),
            "missing ${{VAR}} example: {schema_text}"
        );
        assert!(
            schema_text.contains("$(...)"),
            "missing $(...) example: {schema_text}"
        );
        assert!(
            schema_text.contains("backticks"),
            "missing backticks example: {schema_text}"
        );
        assert!(
            schema_text.contains("$((...))"),
            "missing $((...)) example: {schema_text}"
        );
        assert!(
            schema_text.contains("<(...)") && schema_text.contains(">(...)"),
            "missing process substitution examples: {schema_text}"
        );
    }

    #[test]
    fn test_terminal_tool_description_mentions_head_and_tail_parameters() {
        let description = <TerminalTool as crate::AgentTool>::description().to_string();

        assert!(description.contains("head_lines"));
        assert!(description.contains("tail_lines"));
        assert!(description.contains("Do not pipe output to `head`, `tail`, or similar"));
        assert!(description.contains("visible to the user in real time"));
        assert!(description.contains("waste tokens or exceed the context window"));
    }

    #[test]
    fn test_terminal_tool_input_schema_mentions_head_and_tail_parameters() {
        let schema = <TerminalTool as crate::AgentTool>::input_schema(
            language_model::LanguageModelToolSchemaFormat::JsonSchema,
        );
        let schema_json = serde_json::to_value(schema).expect("schema should serialize");
        let schema_text = schema_json.to_string();

        assert!(schema_text.contains("head_lines"));
        assert!(schema_text.contains("tail_lines"));
        assert!(schema_text.contains("Do not pipe output to `head`"));
        assert!(schema_text.contains("Do not pipe output to `tail`"));
        assert!(schema_text.contains("waste tokens or exceed the context window"));
    }

    async fn assert_rejected_before_terminal_creation(
        command: &str,
        cx: &mut gpui::TestAppContext,
    ) {
        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree("/root", serde_json::json!({})).await;
        let project = project::Project::test(fs, ["/root".as_ref()], cx).await;

        let environment = std::rc::Rc::new(cx.update(|cx| {
            crate::tests::FakeThreadEnvironment::default()
                .with_terminal(crate::tests::FakeTerminalHandle::new_never_exits(cx))
        }));

        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Confirm;
            settings.tool_permissions.tools.remove(TerminalTool::NAME);
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        #[allow(clippy::arc_with_non_send_sync)]
        let tool = std::sync::Arc::new(TerminalTool::new(project, environment.clone()));
        let (event_stream, mut rx) = crate::ToolCallEventStream::test();

        let task = cx.update(|cx| {
            tool.run(
                crate::ToolInput::resolved(TerminalToolInput {
                    command: command.to_string(),
                    cd: "root".to_string(),
                    timeout_ms: None,
                    ..Default::default()
                }),
                event_stream,
                cx,
            )
        });

        let result = task.await;
        let error = result.unwrap_err();
        assert!(
            error.contains("does not allow shell substitutions or interpolations"),
            "command {command:?} should be rejected with substitution message, got: {error}"
        );
        assert!(
            environment.terminal_creation_count() == 0,
            "no terminal should be created for rejected command {command:?}"
        );
        assert!(
            !matches!(
                rx.try_recv(),
                Ok(Ok(crate::ThreadEvent::ToolCallAuthorization(_)))
            ),
            "rejected command {command:?} should not request authorization"
        );
    }

    #[gpui::test]
    async fn test_rejects_variable_expansion(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("echo ${HOME}", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_positional_parameter(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("echo $1", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_special_parameter_question(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("echo $?", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_special_parameter_dollar(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("echo $$", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_special_parameter_at(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("echo $@", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_command_substitution_dollar_parens(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("echo $(whoami)", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_command_substitution_backticks(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("echo `whoami`", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_arithmetic_expansion(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("echo $((1 + 1))", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_process_substitution_input(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("cat <(ls)", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_process_substitution_output(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("ls >(cat)", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_env_prefix_with_variable(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("PAGER=$HOME git log", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_env_prefix_with_command_substitution(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("PAGER=$(whoami) git log", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_env_prefix_with_brace_expansion(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation(
            "GIT_SEQUENCE_EDITOR=${EDITOR} git rebase -i HEAD~2",
            cx,
        )
        .await;
    }

    #[gpui::test]
    async fn test_rejects_multiline_with_forbidden_on_second_line(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("echo ok\necho $HOME", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_multiline_with_forbidden_mixed(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("PAGER=less git log\necho $(whoami)", cx).await;
    }

    #[gpui::test]
    async fn test_rejects_nested_command_substitution(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);
        assert_rejected_before_terminal_creation("echo $(cat $(whoami).txt)", cx).await;
    }

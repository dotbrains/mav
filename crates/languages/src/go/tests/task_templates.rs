use super::*;

#[gpui::test]
async fn test_go_test_templates_run_arg_is_shell_escaped(cx: &mut TestAppContext) {
    let templates = cx
        .update(|cx| GoContextProvider.associated_tasks(None, cx))
        .await
        .expect("Go context provider returns associated tasks");

    // `resolve_task` returns `None` for any `MAV_` variable a template
    // references but the context omits, so supply all of them.
    let context = TaskContext {
        cwd: None,
        task_variables: TaskVariables::from_iter([
            (VariableName::Symbol, "TestFoo".to_string()),
            (GO_SUBTEST_NAME_TASK_VARIABLE, "simple_subtest".to_string()),
            (
                GO_TABLE_TEST_CASE_NAME_TASK_VARIABLE,
                "table_case".to_string(),
            ),
            (GO_SUITE_NAME_TASK_VARIABLE, "Suite".to_string()),
            (GO_PACKAGE_TASK_VARIABLE, ".".to_string()),
            (VariableName::Dirname, "/tmp".to_string()),
        ]),
        project_env: HashMap::default(),
    };

    // `go-benchmark` is excluded: its `-run='^$'` form intentionally quotes.
    let escaped_run_arg_tags = [
        "go-test",
        "go-example",
        "go-subtest",
        "go-fuzz",
        "go-testify-suite",
        "go-table-test-case",
    ];

    for tag in escaped_run_arg_tags {
        let template = templates
            .0
            .iter()
            .find(|template| template.tags.iter().any(|template_tag| template_tag == tag))
            .unwrap_or_else(|| panic!("`{tag}` task template exists"));

        let resolved = template
            .resolve_task("go", &context)
            .unwrap_or_else(|| panic!("`{tag}` template resolves"));

        let run_index = resolved
            .resolved
            .args
            .iter()
            .position(|arg| arg == "-run")
            .unwrap_or_else(|| panic!("`{tag}` resolved args contain a `-run` flag"));
        let run_arg = &resolved.resolved.args[run_index + 1];

        assert!(
            !run_arg.contains('\''),
            "`{tag}` -run arg must not shell-quote the regex; single quotes leak \
             literally into Delve's regex and break Debug Test (#53230), got {run_arg:?}"
        );
        assert!(
            run_arg.starts_with("\\^") && run_arg.ends_with("\\$"),
            "`{tag}` -run arg must escape its regex anchors as \\^...\\$ so the shell \
             and GoLocator both strip them, got {run_arg:?}"
        );
    }
}

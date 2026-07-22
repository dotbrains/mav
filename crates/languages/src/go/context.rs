use super::*;

pub(super) fn adjust_runs(
    delta: usize,
    mut runs: Vec<(Range<usize>, HighlightId)>,
) -> Vec<(Range<usize>, HighlightId)> {
    for (range, _) in &mut runs {
        range.start += delta;
        range.end += delta;
    }
    runs
}

pub(crate) struct GoContextProvider;

pub(crate) struct GoRunnableResolver;

impl RunnableResolver for GoRunnableResolver {
    fn resolve(
        &self,
        local_captures: &[RunnableMatchCapture],
        shared_captures: &[RunnableMatchCapture],
        buffer: &BufferSnapshot,
    ) -> Option<ResolvedRunnable> {
        const FIELD_CHECK: &str = "_field_check";
        const FIELD_NAME: &str = "_field_name";
        const TABLE_TEST_CASE_NAME: &str = "_table_test_case_name";

        // A row may declare several string fields (e.g. `{ name: "x", label: "y" }`), so
        // the query emits one `@_field_name` + `@run` pair per field, in source order.
        //
        // When the loop body calls `t.Run(tc.<field>, ...)`, `@_field_check` names that
        // field; pick the pair whose `@_field_name` matches it. Without a `@_field_check`
        // (e.g. map-keyed tables) the first pair wins.
        let reference_text = shared_captures
            .iter()
            .find(|capture| capture.name() == Some(FIELD_CHECK))
            .map(|capture| buffer.text_for_range(capture.range()).collect::<String>());
        let pair_index = match &reference_text {
            Some(reference) => local_captures
                .iter()
                .filter(|capture| capture.name() == Some(FIELD_NAME))
                .position(|capture| buffer.text_for_range(capture.range()).equals_str(reference))?,
            None => 0,
        };

        // `@run` and `@_table_test_case_name` tag the same string literal, so the chosen
        // run's text is the case name.
        let run_capture = local_captures
            .iter()
            .filter(|capture| capture.is_run())
            .nth(pair_index)?;
        Some(ResolvedRunnable {
            run_range: run_capture.range(),
            extra_captures: smallvec::smallvec![(
                TABLE_TEST_CASE_NAME.to_string(),
                buffer.text_for_range(run_capture.range()).collect(),
            )],
        })
    }
}

const GO_PACKAGE_TASK_VARIABLE: VariableName = VariableName::Custom(Cow::Borrowed("GO_PACKAGE"));
const GO_MODULE_ROOT_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("GO_MODULE_ROOT"));
const GO_SUBTEST_NAME_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("GO_SUBTEST_NAME"));
const GO_TABLE_TEST_CASE_NAME_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("GO_TABLE_TEST_CASE_NAME"));
const GO_SUITE_NAME_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("GO_SUITE_NAME"));

impl ContextProvider for GoContextProvider {
    fn build_context(
        &self,
        variables: &TaskVariables,
        location: ContextLocation<'_>,
        _: Option<HashMap<String, String>>,
        _: Arc<dyn LanguageToolchainStore>,
        cx: &mut gpui::App,
    ) -> Task<Result<TaskVariables>> {
        let local_abs_path = location
            .file_location
            .buffer
            .read(cx)
            .file()
            .and_then(|file| Some(file.as_local()?.abs_path(cx)));

        let go_package_variable = local_abs_path
            .as_deref()
            .and_then(|local_abs_path| local_abs_path.parent())
            .map(|buffer_dir| {
                // Prefer the relative form `./my-nested-package/is-here` over
                // absolute path, because it's more readable in the modal, but
                // the absolute path also works.
                let package_name = variables
                    .get(&VariableName::WorktreeRoot)
                    .and_then(|worktree_abs_path| buffer_dir.strip_prefix(worktree_abs_path).ok())
                    .map(|relative_pkg_dir| {
                        if relative_pkg_dir.as_os_str().is_empty() {
                            ".".into()
                        } else {
                            format!("./{}", relative_pkg_dir.to_string_lossy())
                        }
                    })
                    .unwrap_or_else(|| format!("{}", buffer_dir.to_string_lossy()));

                (GO_PACKAGE_TASK_VARIABLE.clone(), package_name)
            });

        let go_module_root_variable = local_abs_path
            .as_deref()
            .and_then(|local_abs_path| local_abs_path.parent())
            .map(|buffer_dir| {
                // Walk dirtree up until getting the first go.mod file
                let module_dir = buffer_dir
                    .ancestors()
                    .find(|dir| dir.join("go.mod").is_file())
                    .map(|dir| dir.to_string_lossy().into_owned())
                    .unwrap_or_else(|| ".".to_string());

                (GO_MODULE_ROOT_TASK_VARIABLE.clone(), module_dir)
            });

        let _subtest_name = variables.get(&VariableName::Custom(Cow::Borrowed("_subtest_name")));

        let go_subtest_variable = extract_subtest_name(_subtest_name.unwrap_or(""))
            .map(|subtest_name| (GO_SUBTEST_NAME_TASK_VARIABLE.clone(), subtest_name));

        let _table_test_case_name = variables.get(&VariableName::Custom(Cow::Borrowed(
            "_table_test_case_name",
        )));

        let go_table_test_case_variable = _table_test_case_name
            .and_then(extract_subtest_name)
            .map(|case_name| (GO_TABLE_TEST_CASE_NAME_TASK_VARIABLE.clone(), case_name));

        let _suite_name = variables.get(&VariableName::Custom(Cow::Borrowed("_suite_name")));

        let go_suite_variable = _suite_name
            .and_then(extract_subtest_name)
            .map(|suite_name| (GO_SUITE_NAME_TASK_VARIABLE.clone(), suite_name));

        Task::ready(Ok(TaskVariables::from_iter(
            [
                go_package_variable,
                go_subtest_variable,
                go_table_test_case_variable,
                go_suite_variable,
                go_module_root_variable,
            ]
            .into_iter()
            .flatten(),
        )))
    }

    fn associated_tasks(&self, _: Option<Entity<Buffer>>, _: &App) -> Task<Option<TaskTemplates>> {
        let package_cwd = if GO_PACKAGE_TASK_VARIABLE.template_value() == "." {
            None
        } else {
            Some("$MAV_DIRNAME".to_string())
        };
        let module_cwd = Some(GO_MODULE_ROOT_TASK_VARIABLE.template_value());

        Task::ready(Some(TaskTemplates(vec![
            TaskTemplate {
                label: format!(
                    "go test {} -v -run Test{}/{}",
                    GO_PACKAGE_TASK_VARIABLE.template_value(),
                    GO_SUITE_NAME_TASK_VARIABLE.template_value(),
                    VariableName::Symbol.template_value(),
                ),
                command: "go".into(),
                args: vec![
                    "test".into(),
                    "-v".into(),
                    "-run".into(),
                    format!(
                        "\\^Test{}\\$/\\^{}\\$",
                        GO_SUITE_NAME_TASK_VARIABLE.template_value(),
                        VariableName::Symbol.template_value(),
                    ),
                ],
                cwd: package_cwd.clone(),
                tags: vec!["go-testify-suite".to_owned()],
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!(
                    "go test {} -v -run {}/{}",
                    GO_PACKAGE_TASK_VARIABLE.template_value(),
                    VariableName::Symbol.template_value(),
                    GO_TABLE_TEST_CASE_NAME_TASK_VARIABLE.template_value(),
                ),
                command: "go".into(),
                args: vec![
                    "test".into(),
                    "-v".into(),
                    "-run".into(),
                    format!(
                        "\\^{}\\$/\\^{}\\$",
                        VariableName::Symbol.template_value(),
                        GO_TABLE_TEST_CASE_NAME_TASK_VARIABLE.template_value(),
                    ),
                ],
                cwd: package_cwd.clone(),
                tags: vec![
                    "go-table-test-case".to_owned(),
                    "go-table-test-case-without-explicit-variable".to_owned(),
                ],
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!(
                    "go test {} -run {}",
                    GO_PACKAGE_TASK_VARIABLE.template_value(),
                    VariableName::Symbol.template_value(),
                ),
                command: "go".into(),
                args: vec![
                    "test".into(),
                    "-run".into(),
                    format!("\\^{}\\$", VariableName::Symbol.template_value(),),
                ],
                tags: vec!["go-test".to_owned()],
                cwd: package_cwd.clone(),
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!(
                    "go test {} -run {}",
                    GO_PACKAGE_TASK_VARIABLE.template_value(),
                    VariableName::Symbol.template_value(),
                ),
                command: "go".into(),
                args: vec![
                    "test".into(),
                    "-run".into(),
                    format!("\\^{}\\$", VariableName::Symbol.template_value(),),
                ],
                tags: vec!["go-example".to_owned()],
                cwd: package_cwd.clone(),
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!("go test {}", GO_PACKAGE_TASK_VARIABLE.template_value()),
                command: "go".into(),
                args: vec!["test".into()],
                cwd: package_cwd.clone(),
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: "go test ./...".into(),
                command: "go".into(),
                args: vec!["test".into(), "./...".into()],
                cwd: module_cwd.clone(),
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!(
                    "go test {} -v -run {}/{}",
                    GO_PACKAGE_TASK_VARIABLE.template_value(),
                    VariableName::Symbol.template_value(),
                    GO_SUBTEST_NAME_TASK_VARIABLE.template_value(),
                ),
                command: "go".into(),
                args: vec![
                    "test".into(),
                    "-v".into(),
                    "-run".into(),
                    format!(
                        "\\^{}\\$/\\^{}\\$",
                        VariableName::Symbol.template_value(),
                        GO_SUBTEST_NAME_TASK_VARIABLE.template_value(),
                    ),
                ],
                cwd: package_cwd.clone(),
                tags: vec!["go-subtest".to_owned()],
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!(
                    "go test {} -bench {}",
                    GO_PACKAGE_TASK_VARIABLE.template_value(),
                    VariableName::Symbol.template_value()
                ),
                command: "go".into(),
                args: vec![
                    "test".into(),
                    "-benchmem".into(),
                    "-run='^$'".into(),
                    "-bench".into(),
                    format!("\\^{}\\$", VariableName::Symbol.template_value()),
                ],
                cwd: package_cwd.clone(),
                tags: vec!["go-benchmark".to_owned()],
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!(
                    "go test {} -fuzz=Fuzz -run {}",
                    GO_PACKAGE_TASK_VARIABLE.template_value(),
                    VariableName::Symbol.template_value(),
                ),
                command: "go".into(),
                args: vec![
                    "test".into(),
                    "-fuzz=Fuzz".into(),
                    "-run".into(),
                    format!("\\^{}\\$", VariableName::Symbol.template_value(),),
                ],
                tags: vec!["go-fuzz".to_owned()],
                cwd: package_cwd.clone(),
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!("go run {}", GO_PACKAGE_TASK_VARIABLE.template_value(),),
                command: "go".into(),
                args: vec!["run".into(), ".".into()],
                cwd: package_cwd.clone(),
                tags: vec!["go-main".to_owned()],
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!("go generate {}", GO_PACKAGE_TASK_VARIABLE.template_value()),
                command: "go".into(),
                args: vec!["generate".into()],
                cwd: package_cwd,
                tags: vec!["go-generate".to_owned()],
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: "go generate ./...".into(),
                command: "go".into(),
                args: vec!["generate".into(), "./...".into()],
                cwd: module_cwd,
                ..TaskTemplate::default()
            },
        ])))
    }

    fn runnable_resolver(&self) -> Option<Arc<dyn RunnableResolver>> {
        Some(Arc::new(GoRunnableResolver))
    }
}

pub(super) fn extract_subtest_name(input: &str) -> Option<String> {
    let content = if input.starts_with('`') && input.ends_with('`') {
        input.trim_matches('`')
    } else {
        input.trim_matches('"')
    };

    let processed = content
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .collect::<String>();

    Some(
        GO_ESCAPE_SUBTEST_NAME_REGEX
            .replace_all(&processed, |caps: &regex::Captures| {
                format!("\\{}", &caps[0])
            })
            .to_string(),
    )
}

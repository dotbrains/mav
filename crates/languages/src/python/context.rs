use super::*;

pub(crate) struct PythonContextProvider;

const PYTHON_TEST_TARGET_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("PYTHON_TEST_TARGET"));

const PYTHON_ACTIVE_TOOLCHAIN_PATH: VariableName =
    VariableName::Custom(Cow::Borrowed("PYTHON_ACTIVE_MAV_TOOLCHAIN"));

const PYTHON_MODULE_NAME_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("PYTHON_MODULE_NAME"));

impl ContextProvider for PythonContextProvider {
    fn build_context(
        &self,
        variables: &task::TaskVariables,
        location: ContextLocation<'_>,
        _: Option<HashMap<String, String>>,
        toolchains: Arc<dyn LanguageToolchainStore>,
        cx: &mut gpui::App,
    ) -> Task<Result<task::TaskVariables>> {
        let test_target = match selected_test_runner(Some(&location.file_location.buffer), cx) {
            TestRunner::UNITTEST => self.build_unittest_target(variables),
            TestRunner::PYTEST => self.build_pytest_target(variables),
        };

        let module_target = self.build_module_target(variables);
        let location_file = location.file_location.buffer.read(cx).file().cloned();
        let worktree_id = location_file.as_ref().map(|f| f.worktree_id(cx));

        cx.spawn(async move |cx| {
            let active_toolchain = if let Some(worktree_id) = worktree_id {
                let file_path = location_file
                    .as_ref()
                    .and_then(|f| f.path().parent())
                    .map(Arc::from)
                    .unwrap_or_else(|| RelPath::empty_arc());

                toolchains
                    .active_toolchain(worktree_id, file_path, "Python".into(), cx)
                    .await
                    .map_or_else(
                        || String::from("python3"),
                        |toolchain| toolchain.path.to_string(),
                    )
            } else {
                String::from("python3")
            };

            let toolchain = (PYTHON_ACTIVE_TOOLCHAIN_PATH, active_toolchain);

            Ok(task::TaskVariables::from_iter(
                test_target
                    .into_iter()
                    .chain(module_target)
                    .chain([toolchain]),
            ))
        })
    }

    fn associated_tasks(
        &self,
        buffer: Option<Entity<Buffer>>,
        cx: &App,
    ) -> Task<Option<TaskTemplates>> {
        let test_runner = selected_test_runner(buffer.as_ref(), cx);

        let mut tasks = vec![
            // Execute a selection
            TaskTemplate {
                label: "execute selection".to_owned(),
                command: PYTHON_ACTIVE_TOOLCHAIN_PATH.template_value(),
                args: vec![
                    "-c".to_owned(),
                    VariableName::SelectedText.template_value_with_whitespace(),
                ],
                cwd: Some(VariableName::WorktreeRoot.template_value()),
                ..TaskTemplate::default()
            },
            // Execute an entire file
            TaskTemplate {
                label: format!("run '{}'", VariableName::File.template_value()),
                command: PYTHON_ACTIVE_TOOLCHAIN_PATH.template_value(),
                args: vec![VariableName::File.template_value_with_whitespace()],
                cwd: Some(VariableName::WorktreeRoot.template_value()),
                ..TaskTemplate::default()
            },
            // Execute a file as module
            TaskTemplate {
                label: format!("run module '{}'", VariableName::File.template_value()),
                command: PYTHON_ACTIVE_TOOLCHAIN_PATH.template_value(),
                args: vec![
                    "-m".to_owned(),
                    PYTHON_MODULE_NAME_TASK_VARIABLE.template_value(),
                ],
                cwd: Some(VariableName::WorktreeRoot.template_value()),
                tags: vec!["python-module-main-method".to_owned()],
                ..TaskTemplate::default()
            },
        ];

        tasks.extend(match test_runner {
            TestRunner::UNITTEST => {
                [
                    // Run tests for an entire file
                    TaskTemplate {
                        label: format!("unittest '{}'", VariableName::File.template_value()),
                        command: PYTHON_ACTIVE_TOOLCHAIN_PATH.template_value(),
                        args: vec![
                            "-m".to_owned(),
                            "unittest".to_owned(),
                            VariableName::File.template_value_with_whitespace(),
                        ],
                        cwd: Some(VariableName::WorktreeRoot.template_value()),
                        ..TaskTemplate::default()
                    },
                    // Run test(s) for a specific target within a file
                    TaskTemplate {
                        label: "unittest $MAV_CUSTOM_PYTHON_TEST_TARGET".to_owned(),
                        command: PYTHON_ACTIVE_TOOLCHAIN_PATH.template_value(),
                        args: vec![
                            "-m".to_owned(),
                            "unittest".to_owned(),
                            PYTHON_TEST_TARGET_TASK_VARIABLE.template_value_with_whitespace(),
                        ],
                        tags: vec![
                            "python-unittest-class".to_owned(),
                            "python-unittest-method".to_owned(),
                        ],
                        cwd: Some(VariableName::WorktreeRoot.template_value()),
                        ..TaskTemplate::default()
                    },
                ]
            }
            TestRunner::PYTEST => {
                [
                    // Run tests for an entire file
                    TaskTemplate {
                        label: format!("pytest '{}'", VariableName::File.template_value()),
                        command: PYTHON_ACTIVE_TOOLCHAIN_PATH.template_value(),
                        args: vec![
                            "-m".to_owned(),
                            "pytest".to_owned(),
                            VariableName::File.template_value_with_whitespace(),
                        ],
                        cwd: Some(VariableName::WorktreeRoot.template_value()),
                        ..TaskTemplate::default()
                    },
                    // Run test(s) for a specific target within a file
                    TaskTemplate {
                        label: "pytest $MAV_CUSTOM_PYTHON_TEST_TARGET".to_owned(),
                        command: PYTHON_ACTIVE_TOOLCHAIN_PATH.template_value(),
                        args: vec![
                            "-m".to_owned(),
                            "pytest".to_owned(),
                            PYTHON_TEST_TARGET_TASK_VARIABLE.template_value_with_whitespace(),
                        ],
                        cwd: Some(VariableName::WorktreeRoot.template_value()),
                        tags: vec![
                            "python-pytest-class".to_owned(),
                            "python-pytest-method".to_owned(),
                        ],
                        ..TaskTemplate::default()
                    },
                ]
            }
        });

        Task::ready(Some(TaskTemplates(tasks)))
    }
}

fn selected_test_runner(location: Option<&Entity<Buffer>>, cx: &App) -> TestRunner {
    const TEST_RUNNER_VARIABLE: &str = "TEST_RUNNER";
    let language = LanguageName::new_static("Python");
    let settings = LanguageSettings::resolve(location.map(|b| b.read(cx)), Some(&language), cx);
    settings
        .tasks
        .variables
        .get(TEST_RUNNER_VARIABLE)
        .and_then(|val| TestRunner::from_str(val).ok())
        .unwrap_or(TestRunner::PYTEST)
}

impl PythonContextProvider {
    fn build_unittest_target(
        &self,
        variables: &task::TaskVariables,
    ) -> Option<(VariableName, String)> {
        let python_module_name =
            python_module_name_from_relative_path(variables.get(&VariableName::RelativeFile)?)?;

        let unittest_class_name =
            variables.get(&VariableName::Custom(Cow::Borrowed("_unittest_class_name")));

        let unittest_method_name = variables.get(&VariableName::Custom(Cow::Borrowed(
            "_unittest_method_name",
        )));

        let unittest_target_str = match (unittest_class_name, unittest_method_name) {
            (Some(class_name), Some(method_name)) => {
                format!("{python_module_name}.{class_name}.{method_name}")
            }
            (Some(class_name), None) => format!("{python_module_name}.{class_name}"),
            (None, None) => python_module_name,
            // should never happen, a TestCase class is the unit of testing
            (None, Some(_)) => return None,
        };

        Some((
            PYTHON_TEST_TARGET_TASK_VARIABLE.clone(),
            unittest_target_str,
        ))
    }

    fn build_pytest_target(
        &self,
        variables: &task::TaskVariables,
    ) -> Option<(VariableName, String)> {
        let file_path = variables.get(&VariableName::RelativeFile)?;

        let pytest_class_name =
            variables.get(&VariableName::Custom(Cow::Borrowed("_pytest_class_name")));

        let pytest_method_name =
            variables.get(&VariableName::Custom(Cow::Borrowed("_pytest_method_name")));

        let pytest_target_str = match (pytest_class_name, pytest_method_name) {
            (Some(class_name), Some(method_name)) => {
                format!("{file_path}::{class_name}::{method_name}")
            }
            (Some(class_name), None) => {
                format!("{file_path}::{class_name}")
            }
            (None, Some(method_name)) => {
                format!("{file_path}::{method_name}")
            }
            (None, None) => file_path.to_string(),
        };

        Some((PYTHON_TEST_TARGET_TASK_VARIABLE.clone(), pytest_target_str))
    }

    fn build_module_target(
        &self,
        variables: &task::TaskVariables,
    ) -> Result<(VariableName, String)> {
        let python_module_name = variables
            .get(&VariableName::RelativeFile)
            .and_then(|module| python_module_name_from_relative_path(module))
            .unwrap_or_default();

        let module_target = (PYTHON_MODULE_NAME_TASK_VARIABLE.clone(), python_module_name);

        Ok(module_target)
    }
}

pub(crate) fn python_module_name_from_relative_path(relative_path: &str) -> Option<String> {
    let rel_path = RelPath::new(relative_path.as_ref(), PathStyle::local()).ok()?;
    let path_with_dots = rel_path.display(PathStyle::Posix).replace('/', ".");
    Some(
        path_with_dots
            .strip_suffix(".py")
            .map(ToOwned::to_owned)
            .unwrap_or(path_with_dots),
    )
}

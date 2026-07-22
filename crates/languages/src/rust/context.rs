use super::*;

pub(crate) struct RustContextProvider;

const RUST_PACKAGE_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("RUST_PACKAGE"));

/// The bin name corresponding to the current file in Cargo.toml
pub(super) const RUST_BIN_NAME_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("RUST_BIN_NAME"));

/// The bin kind (bin/example) corresponding to the current file in Cargo.toml
pub(super) const RUST_BIN_KIND_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("RUST_BIN_KIND"));

/// The flag to list required features for executing a bin, if any
const RUST_BIN_REQUIRED_FEATURES_FLAG_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("RUST_BIN_REQUIRED_FEATURES_FLAG"));

/// The list of required features for executing a bin, if any
const RUST_BIN_REQUIRED_FEATURES_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("RUST_BIN_REQUIRED_FEATURES"));

const RUST_TEST_FRAGMENT_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("RUST_TEST_FRAGMENT"));

const RUST_DOC_TEST_NAME_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("RUST_DOC_TEST_NAME"));

const RUST_TEST_NAME_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("RUST_TEST_NAME"));

const RUST_MANIFEST_DIRNAME_TASK_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("RUST_MANIFEST_DIRNAME"));

impl ContextProvider for RustContextProvider {
    fn build_context(
        &self,
        task_variables: &TaskVariables,
        location: ContextLocation<'_>,
        project_env: Option<HashMap<String, String>>,
        _: Arc<dyn LanguageToolchainStore>,
        cx: &mut gpui::App,
    ) -> Task<Result<TaskVariables>> {
        let local_abs_path = location
            .file_location
            .buffer
            .read(cx)
            .file()
            .and_then(|file| Some(file.as_local()?.abs_path(cx)));

        let mut variables = TaskVariables::default();

        if let (Some(path), Some(stem)) = (&local_abs_path, task_variables.get(&VariableName::Stem))
        {
            let fragment = test_fragment(&variables, path, stem);
            variables.insert(RUST_TEST_FRAGMENT_TASK_VARIABLE, fragment);
        };
        if let Some(test_name) =
            task_variables.get(&VariableName::Custom(Cow::Borrowed("_test_name")))
        {
            variables.insert(RUST_TEST_NAME_TASK_VARIABLE, test_name.into());
        }
        if let Some(doc_test_name) =
            task_variables.get(&VariableName::Custom(Cow::Borrowed("_doc_test_name")))
        {
            variables.insert(RUST_DOC_TEST_NAME_TASK_VARIABLE, doc_test_name.into());
        }
        cx.background_spawn(async move {
            if let Some(path) = local_abs_path
                .as_deref()
                .and_then(|local_abs_path| local_abs_path.parent())
                && let Some(package_name) =
                    human_readable_package_name(path, project_env.as_ref()).await
            {
                variables.insert(RUST_PACKAGE_TASK_VARIABLE.clone(), package_name);
            }
            if let Some(path) = local_abs_path.as_ref()
                && let Some((target, manifest_path)) =
                    target_info_from_abs_path(path, project_env.as_ref()).await?
            {
                if let Some(target) = target {
                    variables.extend(TaskVariables::from_iter([
                        (RUST_PACKAGE_TASK_VARIABLE.clone(), target.package_name),
                        (RUST_BIN_NAME_TASK_VARIABLE.clone(), target.target_name),
                        (
                            RUST_BIN_KIND_TASK_VARIABLE.clone(),
                            target.target_kind.to_string(),
                        ),
                    ]));
                    if target.required_features.is_empty() {
                        variables.insert(RUST_BIN_REQUIRED_FEATURES_FLAG_TASK_VARIABLE, "".into());
                        variables.insert(RUST_BIN_REQUIRED_FEATURES_TASK_VARIABLE, "".into());
                    } else {
                        variables.insert(
                            RUST_BIN_REQUIRED_FEATURES_FLAG_TASK_VARIABLE.clone(),
                            "--features".to_string(),
                        );
                        variables.insert(
                            RUST_BIN_REQUIRED_FEATURES_TASK_VARIABLE.clone(),
                            target.required_features.join(","),
                        );
                    }
                }
                variables.extend(TaskVariables::from_iter([(
                    RUST_MANIFEST_DIRNAME_TASK_VARIABLE.clone(),
                    manifest_path.to_string_lossy().into_owned(),
                )]));
            }
            Ok(variables)
        })
    }

    fn associated_tasks(
        &self,
        buffer: Option<Entity<Buffer>>,
        cx: &App,
    ) -> Task<Option<TaskTemplates>> {
        const DEFAULT_RUN_NAME_STR: &str = "RUST_DEFAULT_PACKAGE_RUN";
        const CUSTOM_TARGET_DIR: &str = "RUST_TARGET_DIR";

        let language = LanguageName::new_static("Rust");
        let settings = LanguageSettings::resolve(buffer.map(|b| b.read(cx)), Some(&language), cx);
        let package_to_run = settings.tasks.variables.get(DEFAULT_RUN_NAME_STR).cloned();
        let custom_target_dir = settings.tasks.variables.get(CUSTOM_TARGET_DIR).cloned();
        let run_task_args = if let Some(package_to_run) = package_to_run {
            vec!["run".into(), "-p".into(), package_to_run]
        } else {
            vec!["run".into()]
        };
        let mut task_templates = vec![
            TaskTemplate {
                label: format!(
                    "Check (package: {})",
                    RUST_PACKAGE_TASK_VARIABLE.template_value(),
                ),
                command: "cargo".into(),
                args: vec![
                    "check".into(),
                    "-p".into(),
                    RUST_PACKAGE_TASK_VARIABLE.template_value(),
                ],
                cwd: Some("$MAV_DIRNAME".to_owned()),
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: "Check all targets (workspace)".into(),
                command: "cargo".into(),
                args: vec!["check".into(), "--workspace".into(), "--all-targets".into()],
                cwd: Some("$MAV_DIRNAME".to_owned()),
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!(
                    "Test '{}' (package: {})",
                    RUST_TEST_NAME_TASK_VARIABLE.template_value(),
                    RUST_PACKAGE_TASK_VARIABLE.template_value(),
                ),
                command: "cargo".into(),
                args: vec![
                    "test".into(),
                    "-p".into(),
                    RUST_PACKAGE_TASK_VARIABLE.template_value(),
                    "--".into(),
                    "--nocapture".into(),
                    "--include-ignored".into(),
                    RUST_TEST_NAME_TASK_VARIABLE.template_value(),
                ],
                tags: vec!["rust-test".to_owned()],
                cwd: Some(RUST_MANIFEST_DIRNAME_TASK_VARIABLE.template_value()),
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!(
                    "Doc test '{}' (package: {})",
                    RUST_DOC_TEST_NAME_TASK_VARIABLE.template_value(),
                    RUST_PACKAGE_TASK_VARIABLE.template_value(),
                ),
                command: "cargo".into(),
                args: vec![
                    "test".into(),
                    "--doc".into(),
                    "-p".into(),
                    RUST_PACKAGE_TASK_VARIABLE.template_value(),
                    "--".into(),
                    "--nocapture".into(),
                    "--include-ignored".into(),
                    RUST_DOC_TEST_NAME_TASK_VARIABLE.template_value(),
                ],
                tags: vec!["rust-doc-test".to_owned()],
                cwd: Some(RUST_MANIFEST_DIRNAME_TASK_VARIABLE.template_value()),
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!(
                    "Test mod '{}' (package: {})",
                    VariableName::Stem.template_value(),
                    RUST_PACKAGE_TASK_VARIABLE.template_value(),
                ),
                command: "cargo".into(),
                args: vec![
                    "test".into(),
                    "-p".into(),
                    RUST_PACKAGE_TASK_VARIABLE.template_value(),
                    "--".into(),
                    RUST_TEST_FRAGMENT_TASK_VARIABLE.template_value(),
                ],
                tags: vec!["rust-mod-test".to_owned()],
                cwd: Some(RUST_MANIFEST_DIRNAME_TASK_VARIABLE.template_value()),
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!(
                    "Run {} {} (package: {})",
                    RUST_BIN_KIND_TASK_VARIABLE.template_value(),
                    RUST_BIN_NAME_TASK_VARIABLE.template_value(),
                    RUST_PACKAGE_TASK_VARIABLE.template_value(),
                ),
                command: "cargo".into(),
                args: vec![
                    "run".into(),
                    "-p".into(),
                    RUST_PACKAGE_TASK_VARIABLE.template_value(),
                    format!("--{}", RUST_BIN_KIND_TASK_VARIABLE.template_value()),
                    RUST_BIN_NAME_TASK_VARIABLE.template_value(),
                    RUST_BIN_REQUIRED_FEATURES_FLAG_TASK_VARIABLE.template_value(),
                    RUST_BIN_REQUIRED_FEATURES_TASK_VARIABLE.template_value(),
                ],
                cwd: Some(RUST_MANIFEST_DIRNAME_TASK_VARIABLE.template_value()),
                tags: vec!["rust-main".to_owned()],
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: format!(
                    "Test (package: {})",
                    RUST_PACKAGE_TASK_VARIABLE.template_value()
                ),
                command: "cargo".into(),
                args: vec![
                    "test".into(),
                    "-p".into(),
                    RUST_PACKAGE_TASK_VARIABLE.template_value(),
                ],
                cwd: Some(RUST_MANIFEST_DIRNAME_TASK_VARIABLE.template_value()),
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: "Run".into(),
                command: "cargo".into(),
                args: run_task_args,
                cwd: Some(RUST_MANIFEST_DIRNAME_TASK_VARIABLE.template_value()),
                ..TaskTemplate::default()
            },
            TaskTemplate {
                label: "Clean".into(),
                command: "cargo".into(),
                args: vec!["clean".into()],
                cwd: Some(RUST_MANIFEST_DIRNAME_TASK_VARIABLE.template_value()),
                ..TaskTemplate::default()
            },
        ];

        if let Some(custom_target_dir) = custom_target_dir {
            task_templates = task_templates
                .into_iter()
                .map(|mut task_template| {
                    let mut args = task_template.args.split_off(1);
                    task_template.args.append(&mut vec![
                        "--target-dir".to_string(),
                        custom_target_dir.clone(),
                    ]);
                    task_template.args.append(&mut args);

                    task_template
                })
                .collect();
        }

        Task::ready(Some(TaskTemplates(task_templates)))
    }

    fn lsp_task_source(&self) -> Option<LanguageServerName> {
        Some(SERVER_NAME)
    }
}

pub(super) fn test_fragment(variables: &TaskVariables, path: &Path, stem: &str) -> String {
    let fragment = if stem == "lib" {
        // This isn't quite right---it runs the tests for the entire library, rather than
        // just for the top-level `mod tests`. But we don't really have the means here to
        // filter out just that module.
        Some("--lib".to_owned())
    } else if stem == "mod" {
        maybe!({ Some(path.parent()?.file_name()?.to_string_lossy().into_owned()) })
    } else if stem == "main" {
        if let (Some(bin_name), Some(bin_kind)) = (
            variables.get(&RUST_BIN_NAME_TASK_VARIABLE),
            variables.get(&RUST_BIN_KIND_TASK_VARIABLE),
        ) {
            Some(format!("--{bin_kind}={bin_name}"))
        } else {
            None
        }
    } else {
        Some(stem.to_owned())
    };
    fragment.unwrap_or_else(|| "--".to_owned())
}

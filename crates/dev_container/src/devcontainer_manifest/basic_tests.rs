use super::*;

#[gpui::test]
async fn should_get_remote_user_from_devcontainer_if_available(cx: &mut TestAppContext) {
    let (_, devcontainer_manifest) = init_default_devcontainer_manifest(
        cx,
        r#"
// These are some external comments. serde_lenient should handle them
{
// These are some internal comments
"image": "image",
"remoteUser": "root",
}
        "#,
    )
    .await
    .unwrap();

    let mut metadata = HashMap::new();
    metadata.insert(
        "remoteUser".to_string(),
        serde_json_lenient::Value::String("vsCode".to_string()),
    );
    let given_docker_config = DockerInspect {
        id: "docker_id".to_string(),
        config: DockerInspectConfig {
            labels: DockerConfigLabels {
                metadata: Some(vec![metadata]),
            },
            image_user: None,
            env: Vec::new(),
        },
        mounts: None,
        state: None,
    };

    let remote_user =
        get_remote_user_from_config(&given_docker_config, &devcontainer_manifest).unwrap();

    assert_eq!(remote_user, "root".to_string())
}

#[gpui::test]
async fn should_get_remote_user_from_docker_config(cx: &mut TestAppContext) {
    let (_, devcontainer_manifest) = init_default_devcontainer_manifest(cx, "{}").await.unwrap();
    let mut metadata = HashMap::new();
    metadata.insert(
        "remoteUser".to_string(),
        serde_json_lenient::Value::String("vsCode".to_string()),
    );
    let given_docker_config = DockerInspect {
        id: "docker_id".to_string(),
        config: DockerInspectConfig {
            labels: DockerConfigLabels {
                metadata: Some(vec![metadata]),
            },
            image_user: None,
            env: Vec::new(),
        },
        mounts: None,
        state: None,
    };

    let remote_user = get_remote_user_from_config(&given_docker_config, &devcontainer_manifest);

    assert!(remote_user.is_ok());
    let remote_user = remote_user.expect("ok");
    assert_eq!(&remote_user, "vsCode")
}

#[test]
fn should_extract_feature_id_from_references() {
    assert_eq!(
        extract_feature_id("ghcr.io/devcontainers/features/aws-cli:1"),
        "aws-cli"
    );
    assert_eq!(
        extract_feature_id("ghcr.io/devcontainers/features/go"),
        "go"
    );
    assert_eq!(extract_feature_id("ghcr.io/user/repo/node:18.0.0"), "node");
    assert_eq!(extract_feature_id("./myFeature"), "myFeature");
    assert_eq!(
        extract_feature_id("ghcr.io/devcontainers/features/rust@sha256:abc123"),
        "rust"
    );
}

#[test]
fn should_identify_local_feature_refs() {
    assert!(is_local_feature_ref("./lsp-devtools"));
    assert!(is_local_feature_ref("./some/nested/feature"));
    assert!(is_local_feature_ref("../sibling-feature"));
    assert!(!is_local_feature_ref("ghcr.io/devcontainers/features/go:1"));
    assert!(!is_local_feature_ref("ghcr.io/user/repo/node:18.0.0"));
    assert!(!is_local_feature_ref("https://example.com/feature.tgz"));
}

#[gpui::test]
async fn should_create_correct_docker_run_command(cx: &mut TestAppContext) {
    let mut metadata = HashMap::new();
    metadata.insert(
        "remoteUser".to_string(),
        serde_json_lenient::Value::String("vsCode".to_string()),
    );

    let (_, devcontainer_manifest) = init_default_devcontainer_manifest(
        cx,
        r#"{
                "name": "TODO"
            }"#,
    )
    .await
    .unwrap();
    let build_resources = DockerBuildResources {
        image: DockerInspect {
            id: "mcr.microsoft.com/devcontainers/base:ubuntu".to_string(),
            config: DockerInspectConfig {
                labels: DockerConfigLabels {
                    metadata: None,
                    },
                image_user: None,
                env: Vec::new(),
            },
            mounts: None,
            state: None,
        },
        additional_mounts: vec![],
        privileged: false,
        entrypoint_script: Some("echo Container started\n    trap \"exit 0\" 15\n    exec \"$@\"\n    while sleep 1 & wait $!; do :; done".to_string()),
    };
    let docker_run_command = devcontainer_manifest.create_docker_run_command(build_resources);

    assert!(docker_run_command.is_ok());
    let docker_run_command = docker_run_command.expect("ok");

    assert_eq!(docker_run_command.get_program(), "docker");
    let expected_config_file_label = PathBuf::from(TEST_PROJECT_PATH)
        .join(".devcontainer")
        .join("devcontainer.json");
    let expected_config_file_label = expected_config_file_label.display();
    assert_eq!(
        docker_run_command.get_args().collect::<Vec<&OsStr>>(),
        vec![
            OsStr::new("run"),
            OsStr::new("--sig-proxy=false"),
            OsStr::new("-d"),
            OsStr::new("--mount"),
            OsStr::new(&format!(
                "type=bind,source={TEST_PROJECT_PATH},target=/workspaces/project,consistency=cached"
            )),
            OsStr::new("-l"),
            OsStr::new(&format!("devcontainer.local_folder={TEST_PROJECT_PATH}")),
            OsStr::new("-l"),
            OsStr::new(&format!(
                "devcontainer.config_file={expected_config_file_label}"
            )),
            OsStr::new("--entrypoint"),
            OsStr::new("/bin/sh"),
            OsStr::new("mcr.microsoft.com/devcontainers/base:ubuntu"),
            OsStr::new("-c"),
            OsStr::new(
                "
echo Container started
trap \"exit 0\" 15
exec \"$@\"
while sleep 1 & wait $!; do :; done
                    "
                .trim()
            ),
            OsStr::new("-"),
        ]
    )
}

#[gpui::test]
async fn should_not_override_entrypoint_when_override_command_is_false(cx: &mut TestAppContext) {
    let (_, mut devcontainer_manifest) = init_default_devcontainer_manifest(
        cx,
        r#"{
            "name": "test",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
            "overrideCommand": false
        }"#,
    )
    .await
    .unwrap();

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let base_image = DockerInspect {
        id: "mcr.microsoft.com/devcontainers/base:ubuntu".to_string(),
        config: DockerInspectConfig {
            labels: DockerConfigLabels { metadata: None },
            image_user: None,
            env: Vec::new(),
        },
        mounts: None,
        state: None,
    };

    let resources = devcontainer_manifest
        .build_merged_resources(base_image)
        .unwrap();
    assert!(
        resources.entrypoint_script.is_none(),
        "overrideCommand: false must not produce an entrypoint script"
    );

    let docker_run_command = devcontainer_manifest
        .create_docker_run_command(resources)
        .unwrap();
    let args: Vec<&OsStr> = docker_run_command.get_args().collect();
    assert!(
        !args.contains(&OsStr::new("--entrypoint")),
        "overrideCommand: false must not pass --entrypoint to docker run"
    );
    assert!(
        args.contains(&OsStr::new("mcr.microsoft.com/devcontainers/base:ubuntu")),
        "image id must still be present in docker run command"
    );
}

#[gpui::test]
async fn should_find_primary_service_in_docker_compose(cx: &mut TestAppContext) {
    // State where service not defined in dev container
    let (_, given_dev_container) = init_default_devcontainer_manifest(cx, "{}").await.unwrap();
    let given_docker_compose_config = DockerComposeResources {
        config: DockerComposeConfig {
            name: Some("devcontainers".to_string()),
            services: HashMap::new(),
            ..Default::default()
        },
        ..Default::default()
    };

    let bad_result = find_primary_service(&given_docker_compose_config, &given_dev_container);

    assert!(bad_result.is_err());

    // State where service defined in devcontainer, not found in DockerCompose config
    let (_, given_dev_container) =
        init_default_devcontainer_manifest(cx, r#"{"service": "not_found_service"}"#)
            .await
            .unwrap();
    let given_docker_compose_config = DockerComposeResources {
        config: DockerComposeConfig {
            name: Some("devcontainers".to_string()),
            services: HashMap::new(),
            ..Default::default()
        },
        ..Default::default()
    };

    let bad_result = find_primary_service(&given_docker_compose_config, &given_dev_container);

    assert!(bad_result.is_err());
    // State where service defined in devcontainer and in DockerCompose config

    let (_, given_dev_container) =
        init_default_devcontainer_manifest(cx, r#"{"service": "found_service"}"#)
            .await
            .unwrap();
    let given_docker_compose_config = DockerComposeResources {
        config: DockerComposeConfig {
            name: Some("devcontainers".to_string()),
            services: HashMap::from([(
                "found_service".to_string(),
                DockerComposeService {
                    ..Default::default()
                },
            )]),
            ..Default::default()
        },
        ..Default::default()
    };

    let (service_name, _) =
        find_primary_service(&given_docker_compose_config, &given_dev_container).unwrap();

    assert_eq!(service_name, "found_service".to_string());
}

#[gpui::test]
async fn test_nonremote_variable_replacement_with_default_mount(cx: &mut TestAppContext) {
    let fs = FakeFs::new(cx.executor());
    let given_devcontainer_contents = r#"
// These are some external comments. serde_lenient should handle them
{
// These are some internal comments
"image": "mcr.microsoft.com/devcontainers/base:ubuntu",
"name": "myDevContainer-${devcontainerId}",
"remoteUser": "root",
"remoteEnv": {
    "DEVCONTAINER_ID": "${devcontainerId}",
    "MYVAR2": "myvarothervalue",
    "REMOTE_WORKSPACE_FOLDER_BASENAME": "${containerWorkspaceFolderBasename}",
    "LOCAL_WORKSPACE_FOLDER_BASENAME": "${localWorkspaceFolderBasename}",
    "REMOTE_WORKSPACE_FOLDER": "${containerWorkspaceFolder}",
    "LOCAL_WORKSPACE_FOLDER": "${localWorkspaceFolder}",
    "LOCAL_ENV_VAR_1": "${localEnv:local_env_1}",
    "LOCAL_ENV_VAR_2": "${localEnv:my_other_env}",
    "LOCAL_ENV_VAR_3": "before-${localEnv:missing_local_env}-after",
    "LOCAL_ENV_VAR_4": "${localEnv:with_defaults:default}"

}
}
                "#;
    let (_, mut devcontainer_manifest) = init_devcontainer_manifest(
        cx,
        fs,
        fake_http_client(),
        Arc::new(FakeDocker::new()),
        Arc::new(TestCommandRunner::new()),
        HashMap::from([
            ("local_env_1".to_string(), "local_env_value1".to_string()),
            ("my_other_env".to_string(), "THISVALUEHERE".to_string()),
        ]),
        given_devcontainer_contents,
    )
    .await
    .unwrap();

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let ConfigStatus::VariableParsed(variable_replaced_devcontainer) =
        &devcontainer_manifest.config
    else {
        panic!("Config not parsed");
    };

    // ${devcontainerId}
    let devcontainer_id = devcontainer_manifest.devcontainer_id();
    assert_eq!(
        variable_replaced_devcontainer.name,
        Some(format!("myDevContainer-{devcontainer_id}"))
    );
    assert_eq!(
        variable_replaced_devcontainer
            .remote_env
            .as_ref()
            .and_then(|env| env.get("DEVCONTAINER_ID")),
        Some(&devcontainer_id)
    );

    // ${containerWorkspaceFolderBasename}
    assert_eq!(
        variable_replaced_devcontainer
            .remote_env
            .as_ref()
            .and_then(|env| env.get("REMOTE_WORKSPACE_FOLDER_BASENAME")),
        Some(&test_project_filename())
    );

    // ${localWorkspaceFolderBasename}
    assert_eq!(
        variable_replaced_devcontainer
            .remote_env
            .as_ref()
            .and_then(|env| env.get("LOCAL_WORKSPACE_FOLDER_BASENAME")),
        Some(&test_project_filename())
    );

    // ${containerWorkspaceFolder}
    assert_eq!(
        variable_replaced_devcontainer
            .remote_env
            .as_ref()
            .and_then(|env| env.get("REMOTE_WORKSPACE_FOLDER")),
        Some(&format!("/workspaces/{}", test_project_filename()))
    );

    // ${localWorkspaceFolder}
    assert_eq!(
        variable_replaced_devcontainer
            .remote_env
            .as_ref()
            .and_then(|env| env.get("LOCAL_WORKSPACE_FOLDER")),
        // We replace backslashes with forward slashes during variable replacement for JSON safety
        Some(&TEST_PROJECT_PATH.replace("\\", "/"))
    );

    // ${localEnv:VARIABLE_NAME}
    assert_eq!(
        variable_replaced_devcontainer
            .remote_env
            .as_ref()
            .and_then(|env| env.get("LOCAL_ENV_VAR_1")),
        Some(&"local_env_value1".to_string())
    );
    assert_eq!(
        variable_replaced_devcontainer
            .remote_env
            .as_ref()
            .and_then(|env| env.get("LOCAL_ENV_VAR_2")),
        Some(&"THISVALUEHERE".to_string())
    );
    assert_eq!(
        variable_replaced_devcontainer
            .remote_env
            .as_ref()
            .and_then(|env| env.get("LOCAL_ENV_VAR_3")),
        Some(&"before--after".to_string())
    );
    assert_eq!(
        variable_replaced_devcontainer
            .remote_env
            .as_ref()
            .and_then(|env| env.get("LOCAL_ENV_VAR_4")),
        Some(&"default".to_string())
    );
}

#[test]
fn test_replace_environment_variables() {
    let replaced = DevContainerManifest::replace_environment_variables(
        "before ${containerEnv:FOUND} middle ${containerEnv:MISSING:default-value} after${containerEnv:MISSING2}",
        "containerEnv",
        &HashMap::from([("FOUND".to_string(), "value".to_string())]),
    );

    assert_eq!(replaced, "before value middle default-value after");
}

#[test]
fn test_replace_environment_variables_supports_defaults_with_colons() {
    let replaced = DevContainerManifest::replace_environment_variables(
        "before ${containerEnv:MISSING:one:two} after",
        "containerEnv",
        &HashMap::new(),
    );

    assert_eq!(replaced, "before one:two after");
}

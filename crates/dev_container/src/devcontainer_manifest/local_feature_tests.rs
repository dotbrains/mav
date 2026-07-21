use super::*;

#[cfg(not(target_os = "windows"))]
use crate::docker::DockerComposeServicePort;
use crate::{
    DevContainerConfig, DevContainerContext,
    devcontainer_api::DevContainerError,
    devcontainer_json::MountDefinition,
    devcontainer_manifest::{
        ConfigStatus, DevContainerManifest, DockerBuildResources, DockerComposeResources,
        DockerInspect, extract_feature_id, find_primary_service, get_remote_user_from_config,
        image_from_dockerfile, is_local_feature_ref, resolve_compose_dockerfile,
    },
    docker::{
        DockerComposeConfig, DockerComposeService, DockerComposeServiceBuild, DockerConfigLabels,
        DockerInspectConfig,
    },
};

#[path = "compose_project_tests.rs"]
mod compose_project_tests;

#[path = "basic_tests.rs"]
mod basic_tests;

#[path = "dockerfile_feature_tests.rs"]
mod dockerfile_feature_tests;

#[path = "compose_spawn_tests.rs"]
mod compose_spawn_tests;

#[path = "compose_no_uid_tests.rs"]
mod compose_no_uid_tests;

#[path = "compose_services_tests.rs"]
mod compose_services_tests;

#[path = "compose_podman_tests.rs"]
mod compose_podman_tests;

#[path = "dockerfile_no_uid_tests.rs"]
mod dockerfile_no_uid_tests;

#[gpui::test]
async fn test_spawns_devcontainer_with_local_feature(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();
    let given_devcontainer_contents = r#"
        {
          "name": "cli-local-feature-test",
          "image": "test_image:latest",
          "features": {
            "./lsp-devtools": {
              "version": "0.1.0"
            }
          }
        }
        "#;

    let (test_dependencies, mut devcontainer_manifest) =
        init_default_devcontainer_manifest(cx, given_devcontainer_contents)
            .await
            .unwrap();

    test_dependencies
        .fs
        .insert_tree(
            format!("{TEST_PROJECT_PATH}/.devcontainer/lsp-devtools"),
            serde_json::json!({
                "devcontainer-feature.json": r#"{
                    "id": "lsp-devtools",
                    "version": "0.1.0",
                    "name": "LSP Devtools",
                    "options": {
                        "version": {
                            "type": "string",
                            "default": "latest"
                        }
                    }
                }"#,
                "install.sh": "#!/bin/sh\nset -e\necho 'Installing lsp-devtools'",
            }),
        )
        .await;

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let _devcontainer_up = devcontainer_manifest.build_and_run().await.unwrap();

    let files = test_dependencies.fs.files();

    let feature_dockerfile = files
        .iter()
        .find(|f| {
            f.file_name()
                .is_some_and(|s| s.display().to_string() == "Dockerfile.extended")
        })
        .expect("Dockerfile.extended should be generated");
    let feature_dockerfile = test_dependencies.fs.load(feature_dockerfile).await.unwrap();

    assert!(
        feature_dockerfile.contains("lsp-devtools_0"),
        "Dockerfile.extended should reference the local feature. Got:\n{}",
        feature_dockerfile
    );

    let install_wrapper = files
        .iter()
        .find(|f| {
            f.file_name()
                .is_some_and(|s| s.display().to_string() == "devcontainer-features-install.sh")
                && f.to_str().is_some_and(|s| s.contains("/lsp-devtools_"))
        })
        .expect("Install wrapper should be generated for local feature");
    let install_wrapper = test_dependencies.fs.load(install_wrapper).await.unwrap();
    assert!(
        install_wrapper.contains("./lsp-devtools"),
        "Install wrapper should reference the local feature path. Got:\n{}",
        install_wrapper
    );

    let feature_env = files
        .iter()
        .find(|f| {
            f.file_name()
                .is_some_and(|s| s.display().to_string() == "devcontainer-features.env")
                && f.to_str().is_some_and(|s| s.contains("/lsp-devtools_"))
        })
        .expect("Feature env file should be generated for local feature");
    let feature_env = test_dependencies.fs.load(feature_env).await.unwrap();
    assert!(
        feature_env.contains("VERSION=0.1.0"),
        "Feature env should contain user-provided version override. Got:\n{}",
        feature_env
    );

    let install_sh = files
        .iter()
        .find(|f| {
            f.file_name()
                .is_some_and(|s| s.display().to_string() == "install.sh")
                && f.to_str().is_some_and(|s| s.contains("/lsp-devtools_"))
        })
        .expect("install.sh should be copied from the local feature directory");
    let install_sh = test_dependencies.fs.load(install_sh).await.unwrap();
    assert!(
        install_sh.contains("Installing lsp-devtools"),
        "install.sh should have the original content. Got:\n{}",
        install_sh
    );
}

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

#[path = "local_feature_tests.rs"]
mod local_feature_tests;

#[path = "plain_image_tests.rs"]
mod plain_image_tests;

#[path = "dockerfile_parse_tests.rs"]
mod dockerfile_parse_tests;

#[gpui::test]
async fn check_for_existing_container_errors_when_multiple_match(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (test_dependencies, devcontainer_manifest) =
        init_default_devcontainer_manifest(cx, r#"{"image": "image"}"#)
            .await
            .unwrap();
    test_dependencies
        .docker
        .set_duplicate_container_ids(vec!["abc123".to_string(), "def456".to_string()]);

    let result = devcontainer_manifest
        .check_for_existing_devcontainer()
        .await;

    let Err(DevContainerError::MultipleMatchingContainers(ids)) = result else {
        panic!("expected MultipleMatchingContainers, got {result:?}");
    };
    assert_eq!(ids, vec!["abc123".to_string(), "def456".to_string()]);
}

#[gpui::test]
async fn trim_non_alphanumeric_chars_from_image_tag(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();
    let given_devcontainer_contents = r#"
        {
          "name": "abcde test",
          "image": "test_image:latest",
        }
        "#;

    let (_, devcontainer_manifest) =
        init_default_devcontainer_manifest(cx, given_devcontainer_contents)
            .await
            .unwrap();

    let image_tag = devcontainer_manifest.generate_features_image_tag("Dockerfile".to_string());

    assert!(
        image_tag.starts_with("abcde-"),
        "expected prefix 'abcde-', got: {image_tag}"
    );
    assert!(
        image_tag.ends_with("-features"),
        "expected suffix '-features', got: {image_tag}"
    );
}

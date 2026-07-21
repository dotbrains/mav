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

#[gpui::test]
async fn test_spawns_devcontainer_with_plain_image(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();
    let given_devcontainer_contents = r#"
        {
          "name": "cli-${devcontainerId}",
          "image": "test_image:latest",
        }
        "#;

    let (test_dependencies, mut devcontainer_manifest) =
        init_default_devcontainer_manifest(cx, given_devcontainer_contents)
            .await
            .unwrap();

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let _devcontainer_up = devcontainer_manifest.build_and_run().await.unwrap();

    let files = test_dependencies.fs.files();
    let uid_dockerfile = files
        .iter()
        .find(|f| {
            f.file_name()
                .is_some_and(|s| s.display().to_string() == "updateUID.Dockerfile")
        })
        .expect("to be found");
    let uid_dockerfile = test_dependencies.fs.load(uid_dockerfile).await.unwrap();

    assert_eq!(
        &uid_dockerfile,
        r#"ARG BASE_IMAGE
FROM $BASE_IMAGE

USER root

ARG REMOTE_USER
ARG NEW_UID
ARG NEW_GID
SHELL ["/bin/sh", "-c"]
RUN eval $(sed -n "s/${REMOTE_USER}:[^:]*:\([^:]*\):\([^:]*\):[^:]*:\([^:]*\).*/OLD_UID=\1;OLD_GID=\2;HOME_FOLDER=\3/p" /etc/passwd); \
	eval $(sed -n "s/\([^:]*\):[^:]*:${NEW_UID}:.*/EXISTING_USER=\1/p" /etc/passwd); \
	eval $(sed -n "s/\([^:]*\):[^:]*:${NEW_GID}:.*/EXISTING_GROUP=\1/p" /etc/group); \
	if [ -z "$OLD_UID" ]; then \
		echo "Remote user not found in /etc/passwd ($REMOTE_USER)."; \
	elif [ "$OLD_UID" = "$NEW_UID" -a "$OLD_GID" = "$NEW_GID" ]; then \
		echo "UIDs and GIDs are the same ($NEW_UID:$NEW_GID)."; \
	elif [ "$OLD_UID" != "$NEW_UID" -a -n "$EXISTING_USER" ]; then \
		echo "User with UID exists ($EXISTING_USER=$NEW_UID)."; \
	else \
		if [ "$OLD_GID" != "$NEW_GID" -a -n "$EXISTING_GROUP" ]; then \
			FREE_GID=65532; \
			while grep -q ":[^:]*:${FREE_GID}:" /etc/group; do FREE_GID=$((FREE_GID - 1)); done; \
			echo "Reassigning group $EXISTING_GROUP from GID $NEW_GID to $FREE_GID."; \
			sed -i -e "s/\(${EXISTING_GROUP}:[^:]*:\)${NEW_GID}:/\1${FREE_GID}:/" /etc/group; \
		fi; \
		echo "Updating UID:GID from $OLD_UID:$OLD_GID to $NEW_UID:$NEW_GID."; \
		sed -i -e "s/\(${REMOTE_USER}:[^:]*:\)[^:]*:[^:]*/\1${NEW_UID}:${NEW_GID}/" /etc/passwd; \
		if [ "$OLD_GID" != "$NEW_GID" ]; then \
			sed -i -e "s/\([^:]*:[^:]*:\)${OLD_GID}:/\1${NEW_GID}:/" /etc/group; \
		fi; \
		chown -R $NEW_UID:$NEW_GID $HOME_FOLDER; \
	fi;

ARG IMAGE_USER
USER $IMAGE_USER

# Ensure that /etc/profile does not clobber the existing path
RUN sed -i -E 's/((^|\s)PATH=)([^\$]*)$/\1\${PATH:-\3}/g' /etc/profile || true
"#
    );
}

#[cfg(target_os = "windows")]
#[gpui::test]
async fn test_spawns_devcontainer_with_plain_image(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();
    let given_devcontainer_contents = r#"
        {
          "name": "cli-${devcontainerId}",
          "image": "test_image:latest",
        }
        "#;

    let (_, mut devcontainer_manifest) =
        init_default_devcontainer_manifest(cx, given_devcontainer_contents)
            .await
            .unwrap();

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let devcontainer_up = devcontainer_manifest.build_and_run().await.unwrap();

    assert_eq!(
        devcontainer_up.remote_workspace_folder,
        "/workspaces/project"
    );
}

#[cfg(not(target_os = "windows"))]
#[gpui::test]
async fn test_spawns_devcontainer_with_docker_compose_and_plain_image(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();
    let given_devcontainer_contents = r#"
        {
          "name": "cli-${devcontainerId}",
          "dockerComposeFile": "docker-compose-plain.yml",
          "service": "app",
        }
        "#;

    let (test_dependencies, mut devcontainer_manifest) =
        init_default_devcontainer_manifest(cx, given_devcontainer_contents)
            .await
            .unwrap();

    test_dependencies
        .fs
        .atomic_write(
            PathBuf::from(TEST_PROJECT_PATH).join(".devcontainer/docker-compose-plain.yml"),
            r#"
services:
app:
    image: test_image:latest
    command: sleep infinity
    volumes:
        - ..:/workspace:cached
            "#
            .trim()
            .to_string(),
        )
        .await
        .unwrap();

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let _devcontainer_up = devcontainer_manifest.build_and_run().await.unwrap();

    let files = test_dependencies.fs.files();
    let uid_dockerfile = files
        .iter()
        .find(|f| {
            f.file_name()
                .is_some_and(|s| s.display().to_string() == "updateUID.Dockerfile")
        })
        .expect("to be found");
    let uid_dockerfile = test_dependencies.fs.load(uid_dockerfile).await.unwrap();

    assert_eq!(
        &uid_dockerfile,
        r#"ARG BASE_IMAGE
FROM $BASE_IMAGE

USER root

ARG REMOTE_USER
ARG NEW_UID
ARG NEW_GID
SHELL ["/bin/sh", "-c"]
RUN eval $(sed -n "s/${REMOTE_USER}:[^:]*:\([^:]*\):\([^:]*\):[^:]*:\([^:]*\).*/OLD_UID=\1;OLD_GID=\2;HOME_FOLDER=\3/p" /etc/passwd); \
	eval $(sed -n "s/\([^:]*\):[^:]*:${NEW_UID}:.*/EXISTING_USER=\1/p" /etc/passwd); \
	eval $(sed -n "s/\([^:]*\):[^:]*:${NEW_GID}:.*/EXISTING_GROUP=\1/p" /etc/group); \
	if [ -z "$OLD_UID" ]; then \
		echo "Remote user not found in /etc/passwd ($REMOTE_USER)."; \
	elif [ "$OLD_UID" = "$NEW_UID" -a "$OLD_GID" = "$NEW_GID" ]; then \
		echo "UIDs and GIDs are the same ($NEW_UID:$NEW_GID)."; \
	elif [ "$OLD_UID" != "$NEW_UID" -a -n "$EXISTING_USER" ]; then \
		echo "User with UID exists ($EXISTING_USER=$NEW_UID)."; \
	else \
		if [ "$OLD_GID" != "$NEW_GID" -a -n "$EXISTING_GROUP" ]; then \
			FREE_GID=65532; \
			while grep -q ":[^:]*:${FREE_GID}:" /etc/group; do FREE_GID=$((FREE_GID - 1)); done; \
			echo "Reassigning group $EXISTING_GROUP from GID $NEW_GID to $FREE_GID."; \
			sed -i -e "s/\(${EXISTING_GROUP}:[^:]*:\)${NEW_GID}:/\1${FREE_GID}:/" /etc/group; \
		fi; \
		echo "Updating UID:GID from $OLD_UID:$OLD_GID to $NEW_UID:$NEW_GID."; \
		sed -i -e "s/\(${REMOTE_USER}:[^:]*:\)[^:]*:[^:]*/\1${NEW_UID}:${NEW_GID}/" /etc/passwd; \
		if [ "$OLD_GID" != "$NEW_GID" ]; then \
			sed -i -e "s/\([^:]*:[^:]*:\)${OLD_GID}:/\1${NEW_GID}:/" /etc/group; \
		fi; \
		chown -R $NEW_UID:$NEW_GID $HOME_FOLDER; \
	fi;

ARG IMAGE_USER
USER $IMAGE_USER

# Ensure that /etc/profile does not clobber the existing path
RUN sed -i -E 's/((^|\s)PATH=)([^\$]*)$/\1\${PATH:-\3}/g' /etc/profile || true
"#
    );
}

use super::*;

// updateRemoteUserUID is treated as false in Windows, so this test will fail
// It is covered by test_spawns_devcontainer_with_dockerfile_and_no_update_uid
#[cfg(not(target_os = "windows"))]
#[gpui::test]
async fn test_spawns_devcontainer_with_dockerfile_and_features(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();
    let given_devcontainer_contents = r#"
        /*---------------------------------------------------------------------------------------------
         *  Copyright (c) Microsoft Corporation. All rights reserved.
         *  Licensed under the MIT License. See License.txt in the project root for license information.
         *--------------------------------------------------------------------------------------------*/
        {
          "name": "cli-${devcontainerId}",
          // "image": "mcr.microsoft.com/devcontainers/typescript-node:16-bullseye",
          "build": {
            "dockerfile": "Dockerfile",
            "args": {
              "VARIANT": "18-bookworm",
              "FOO": "bar",
            },
          },
          "workspaceMount": "source=${localWorkspaceFolder},target=${containerWorkspaceFolder},type=bind,consistency=cached",
          "workspaceFolder": "/workspace2",
          "mounts": [
            // Keep command history across instances
            "source=dev-containers-cli-bashhistory,target=/home/node/commandhistory",
          ],

          "runArgs": [
            "--cap-add=SYS_PTRACE",
            "--sig-proxy=true",
          ],

          "forwardPorts": [
            8082,
            8083,
          ],
          "appPort": [
            8084,
            "8085:8086",
          ],

          "containerEnv": {
            "VARIABLE_VALUE": "value",
          },

          "initializeCommand": "touch IAM.md",

          "onCreateCommand": "echo 'onCreateCommand' >> ON_CREATE_COMMAND.md",

          "updateContentCommand": "echo 'updateContentCommand' >> UPDATE_CONTENT_COMMAND.md",

          "postCreateCommand": {
            "yarn": "yarn install",
            "debug": "echo 'postStartCommand' >> POST_START_COMMAND.md",
          },

          "postStartCommand": "echo 'postStartCommand' >> POST_START_COMMAND.md",

          "postAttachCommand": "echo 'postAttachCommand' >> POST_ATTACH_COMMAND.md",

          "remoteUser": "node",

          "remoteEnv": {
            "PATH": "${containerEnv:PATH}:/some/other/path",
            "OTHER_ENV": "other_env_value"
          },

          "features": {
            "ghcr.io/devcontainers/features/docker-in-docker:2": {
              "moby": false,
            },
            "ghcr.io/devcontainers/features/go:1": {},
          },

          "customizations": {
            "vscode": {
              "extensions": [
                "dbaeumer.vscode-eslint",
                "GitHub.vscode-pull-request-github",
              ],
            },
            "mav": {
              "extensions": ["vue", "ruby"],
            },
            "codespaces": {
              "repositories": {
                "devcontainers/features": {
                  "permissions": {
                    "contents": "write",
                    "workflows": "write",
                  },
                },
              },
            },
          },
        }
        "#;

    let (test_dependencies, mut devcontainer_manifest) =
        init_default_devcontainer_manifest(cx, given_devcontainer_contents)
            .await
            .unwrap();

    test_dependencies
        .fs
        .atomic_write(
            PathBuf::from(TEST_PROJECT_PATH).join(".devcontainer/Dockerfile"),
            r#"
#  Copyright (c) Microsoft Corporation. All rights reserved.
#  Licensed under the MIT License. See License.txt in the project root for license information.
ARG VARIANT="16-bullseye"
FROM mcr.microsoft.com/devcontainers/typescript-node:1-${VARIANT}

RUN mkdir -p /workspaces && chown node:node /workspaces

ARG USERNAME=node
USER $USERNAME

# Save command line history
RUN echo "export HISTFILE=/home/$USERNAME/commandhistory/.bash_history" >> "/home/$USERNAME/.bashrc" \
&& echo "export PROMPT_COMMAND='history -a'" >> "/home/$USERNAME/.bashrc" \
&& mkdir -p /home/$USERNAME/commandhistory \
&& touch /home/$USERNAME/commandhistory/.bash_history \
&& chown -R $USERNAME /home/$USERNAME/commandhistory
                "#.trim().to_string(),
        )
        .await
        .unwrap();

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let devcontainer_up = devcontainer_manifest.build_and_run().await.unwrap();

    assert_eq!(
        devcontainer_up.extension_ids,
        vec!["vue".to_string(), "ruby".to_string()]
    );

    let files = test_dependencies.fs.files();
    let feature_dockerfile = files
        .iter()
        .find(|f| {
            f.file_name()
                .is_some_and(|s| s.display().to_string() == "Dockerfile.extended")
        })
        .expect("to be found");
    let feature_dockerfile = test_dependencies.fs.load(feature_dockerfile).await.unwrap();
    assert_eq!(
        &feature_dockerfile,
        r#"ARG _DEV_CONTAINERS_BASE_IMAGE=placeholder

#  Copyright (c) Microsoft Corporation. All rights reserved.
#  Licensed under the MIT License. See License.txt in the project root for license information.
ARG VARIANT="16-bullseye"
FROM mcr.microsoft.com/devcontainers/typescript-node:1-${VARIANT} AS dev_container_auto_added_stage_label

RUN mkdir -p /workspaces && chown node:node /workspaces

ARG USERNAME=node
USER $USERNAME

# Save command line history
RUN echo "export HISTFILE=/home/$USERNAME/commandhistory/.bash_history" >> "/home/$USERNAME/.bashrc" \
&& echo "export PROMPT_COMMAND='history -a'" >> "/home/$USERNAME/.bashrc" \
&& mkdir -p /home/$USERNAME/commandhistory \
&& touch /home/$USERNAME/commandhistory/.bash_history \
&& chown -R $USERNAME /home/$USERNAME/commandhistory

FROM $_DEV_CONTAINERS_BASE_IMAGE AS dev_containers_feature_content_normalize
USER root
COPY --from=dev_containers_feature_content_source ./devcontainer-features.builtin.env /tmp/build-features/
RUN chmod -R 0755 /tmp/build-features/

FROM $_DEV_CONTAINERS_BASE_IMAGE AS dev_containers_target_stage

USER root

RUN mkdir -p /tmp/dev-container-features
COPY --from=dev_containers_feature_content_normalize /tmp/build-features/ /tmp/dev-container-features

RUN \
echo "_CONTAINER_USER_HOME=$( (command -v getent >/dev/null 2>&1 && getent passwd 'root' || grep -E '^root|^[^:]*:[^:]*:root:' /etc/passwd || true) | cut -d: -f6)" >> /tmp/dev-container-features/devcontainer-features.builtin.env && \
echo "_REMOTE_USER_HOME=$( (command -v getent >/dev/null 2>&1 && getent passwd 'node' || grep -E '^node|^[^:]*:[^:]*:node:' /etc/passwd || true) | cut -d: -f6)" >> /tmp/dev-container-features/devcontainer-features.builtin.env


RUN --mount=type=bind,from=dev_containers_feature_content_source,source=./docker-in-docker_0,target=/tmp/build-features-src/docker-in-docker_0 \
cp -ar /tmp/build-features-src/docker-in-docker_0 /tmp/dev-container-features \
&& chmod -R 0755 /tmp/dev-container-features/docker-in-docker_0 \
&& cd /tmp/dev-container-features/docker-in-docker_0 \
&& chmod +x ./devcontainer-features-install.sh \
&& ./devcontainer-features-install.sh \
&& rm -rf /tmp/dev-container-features/docker-in-docker_0

RUN --mount=type=bind,from=dev_containers_feature_content_source,source=./go_1,target=/tmp/build-features-src/go_1 \
cp -ar /tmp/build-features-src/go_1 /tmp/dev-container-features \
&& chmod -R 0755 /tmp/dev-container-features/go_1 \
&& cd /tmp/dev-container-features/go_1 \
&& chmod +x ./devcontainer-features-install.sh \
&& ./devcontainer-features-install.sh \
&& rm -rf /tmp/dev-container-features/go_1


ARG _DEV_CONTAINERS_IMAGE_USER=root
USER $_DEV_CONTAINERS_IMAGE_USER
"#
    );

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

ENV DOCKER_BUILDKIT=1

ENV GOPATH=/go
ENV GOROOT=/usr/local/go
ENV PATH=/usr/local/go/bin:/go/bin:${PATH}
ENV VARIABLE_VALUE=value
"#
    );

    let golang_install_wrapper = files
        .iter()
        .find(|f| {
            f.file_name()
                .is_some_and(|s| s.display().to_string() == "devcontainer-features-install.sh")
                && f.to_str().is_some_and(|s| s.contains("/go_"))
        })
        .expect("to be found");
    let golang_install_wrapper = test_dependencies
        .fs
        .load(golang_install_wrapper)
        .await
        .unwrap();
    assert_eq!(
        &golang_install_wrapper,
        r#"#!/bin/sh
set -e

on_exit () {
[ $? -eq 0 ] && exit
echo 'ERROR: Feature "go" (ghcr.io/devcontainers/features/go:1) failed to install!'
}

trap on_exit EXIT

echo ===========================================================================
echo 'Feature       : go'
echo 'Id            : ghcr.io/devcontainers/features/go:1'
echo 'Options       :'
echo '    GOLANGCILINTVERSION=latest
VERSION=latest'
echo ===========================================================================

set -a
. ../devcontainer-features.builtin.env
. ./devcontainer-features.env
set +a

chmod +x ./install.sh
./install.sh
"#
    );

    let docker_commands = test_dependencies
        .command_runner
        .commands_by_program("docker");

    let docker_run_command = docker_commands
        .iter()
        .find(|c| c.args.get(0).is_some_and(|a| a == "run"))
        .expect("found");

    assert_eq!(
        docker_run_command.args,
        vec![
            "run".to_string(),
            "--privileged".to_string(),
            "--cap-add=SYS_PTRACE".to_string(),
            "--sig-proxy=true".to_string(),
            "-d".to_string(),
            "--mount".to_string(),
            "type=bind,source=/path/to/local/project,target=/workspace2,consistency=cached".to_string(),
            "--mount".to_string(),
            "type=volume,source=dev-containers-cli-bashhistory,target=/home/node/commandhistory,consistency=cached".to_string(),
            "--mount".to_string(),
            "type=volume,source=dind-var-lib-docker-42dad4b4ca7b8ced,target=/var/lib/docker,consistency=cached".to_string(),
            "-l".to_string(),
            "devcontainer.local_folder=/path/to/local/project".to_string(),
            "-l".to_string(),
            "devcontainer.config_file=/path/to/local/project/.devcontainer/devcontainer.json".to_string(),
            "-l".to_string(),
            "devcontainer.metadata=[{\"remoteUser\":\"node\"}]".to_string(),
            "-p".to_string(),
            "8082:8082".to_string(),
            "-p".to_string(),
            "8083:8083".to_string(),
            "-p".to_string(),
            "8084:8084".to_string(),
            "-p".to_string(),
            "8085:8086".to_string(),
            "--entrypoint".to_string(),
            "/bin/sh".to_string(),
            "sha256:610e6cfca95280188b021774f8cf69dd6f49bdb6eebc34c5ee2010f4d51cc105".to_string(),
            "-c".to_string(),
            "echo Container started\ntrap \"exit 0\" 15\n/usr/local/share/docker-init.sh\nexec \"$@\"\nwhile sleep 1 & wait $!; do :; done".to_string(),
            "-".to_string()
        ]
    );

    let docker_exec_commands = test_dependencies
        .docker
        .exec_commands_recorded
        .lock()
        .unwrap();

    assert!(docker_exec_commands.iter().all(|exec| {
        exec.env
            == HashMap::from([
                ("OTHER_ENV".to_string(), "other_env_value".to_string()),
                (
                    "PATH".to_string(),
                    "/initial/path:/some/other/path".to_string(),
                ),
            ])
    }))
}

use super::*;

#[gpui::test]
async fn test_spawns_devcontainer_with_dockerfile_and_no_update_uid(cx: &mut TestAppContext) {
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
            "target": "development",
          },
          "workspaceMount": "source=${localWorkspaceFolder},target=${containerWorkspaceFolder},type=bind,consistency=cached",
          "workspaceFolder": "/workspace2",
          "mounts": [
            // Keep command history across instances
            "source=dev-containers-cli-bashhistory,target=/home/node/commandhistory",
          ],

          "forwardPorts": [
            8082,
            8083,
          ],
          "appPort": "8084",
          "updateRemoteUserUID": false,

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
FROM mcr.microsoft.com/devcontainers/typescript-node:latest as predev
FROM mcr.microsoft.com/devcontainers/typescript-node:1-${VARIANT} as development

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
FROM mcr.microsoft.com/devcontainers/typescript-node:latest as predev
FROM mcr.microsoft.com/devcontainers/typescript-node:1-${VARIANT} as development

RUN mkdir -p /workspaces && chown node:node /workspaces

ARG USERNAME=node
USER $USERNAME

# Save command line history
RUN echo "export HISTFILE=/home/$USERNAME/commandhistory/.bash_history" >> "/home/$USERNAME/.bashrc" \
&& echo "export PROMPT_COMMAND='history -a'" >> "/home/$USERNAME/.bashrc" \
&& mkdir -p /home/$USERNAME/commandhistory \
&& touch /home/$USERNAME/commandhistory/.bash_history \
&& chown -R $USERNAME /home/$USERNAME/commandhistory
FROM development AS dev_container_auto_added_stage_label

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
                && f.to_str().is_some_and(|s| s.contains("go_"))
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
        .find(|c| c.args.get(0).is_some_and(|a| a == "run"));

    assert!(docker_run_command.is_some());

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

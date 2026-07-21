use super::*;

#[gpui::test]
async fn test_spawns_devcontainer_with_docker_compose_and_no_update_uid(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();
    let given_devcontainer_contents = r#"
    // For format details, see https://aka.ms/devcontainer.json. For config options, see the
    // README at: https://github.com/devcontainers/templates/tree/main/src/rust-postgres
    {
      "features": {
        "ghcr.io/devcontainers/features/aws-cli:1": {},
        "ghcr.io/devcontainers/features/docker-in-docker:2": {},
      },
      "name": "Rust and PostgreSQL",
      "dockerComposeFile": "docker-compose.yml",
      "service": "app",
      "workspaceFolder": "/workspaces/${localWorkspaceFolderBasename}",

      // Features to add to the dev container. More info: https://containers.dev/features.
      // "features": {},

      // Use 'forwardPorts' to make a list of ports inside the container available locally.
      "forwardPorts": [
        8083,
        "db:5432",
        "db:1234",
      ],
      "updateRemoteUserUID": false,
      "appPort": "8084",

      // Use 'postCreateCommand' to run commands after the container is created.
      // "postCreateCommand": "rustc --version",

      // Configure tool-specific properties.
      // "customizations": {},

      // Uncomment to connect as root instead. More info: https://aka.ms/dev-containers-non-root.
      // "remoteUser": "root"
    }
    "#;
    let (test_dependencies, mut devcontainer_manifest) =
        init_default_devcontainer_manifest(cx, given_devcontainer_contents)
            .await
            .unwrap();

    test_dependencies
    .fs
    .atomic_write(
        PathBuf::from(TEST_PROJECT_PATH).join(".devcontainer/docker-compose.yml"),
        r#"
version: '3.8'

volumes:
postgres-data:

services:
app:
build:
    context: .
    dockerfile: Dockerfile
env_file:
    # Ensure that the variables in .env match the same variables in devcontainer.json
    - .env

volumes:
    - ../..:/workspaces:cached

# Overrides default command so things don't shut down after the process ends.
command: sleep infinity

# Runs app on the same network as the database container, allows "forwardPorts" in devcontainer.json function.
network_mode: service:db

# Use "forwardPorts" in **devcontainer.json** to forward an app port locally.
# (Adding the "ports" property to this file will not forward from a Codespace.)

db:
image: postgres:14.1
restart: unless-stopped
volumes:
    - postgres-data:/var/lib/postgresql/data
env_file:
    # Ensure that the variables in .env match the same variables in devcontainer.json
    - .env

# Add "forwardPorts": ["5432"] to **devcontainer.json** to forward PostgreSQL locally.
# (Adding the "ports" property to this file will not forward from a Codespace.)
            "#.trim().to_string(),
    )
    .await
    .unwrap();

    test_dependencies.fs.atomic_write(
    PathBuf::from(TEST_PROJECT_PATH).join(".devcontainer/Dockerfile"),
    r#"
FROM mcr.microsoft.com/devcontainers/rust:2-1-bookworm

# Include lld linker to improve build times either by using environment variable
# RUSTFLAGS="-C link-arg=-fuse-ld=lld" or with Cargo's configuration file (i.e see .cargo/config.toml).
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
&& apt-get -y install clang lld \
&& apt-get autoremove -y && apt-get clean -y
    "#.trim().to_string()).await.unwrap();

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let _devcontainer_up = devcontainer_manifest.build_and_run().await.unwrap();

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

FROM mcr.microsoft.com/devcontainers/rust:2-1-bookworm AS dev_container_auto_added_stage_label

# Include lld linker to improve build times either by using environment variable
# RUSTFLAGS="-C link-arg=-fuse-ld=lld" or with Cargo's configuration file (i.e see .cargo/config.toml).
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
&& apt-get -y install clang lld \
&& apt-get autoremove -y && apt-get clean -y

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
echo "_REMOTE_USER_HOME=$( (command -v getent >/dev/null 2>&1 && getent passwd 'vscode' || grep -E '^vscode|^[^:]*:[^:]*:vscode:' /etc/passwd || true) | cut -d: -f6)" >> /tmp/dev-container-features/devcontainer-features.builtin.env


RUN --mount=type=bind,from=dev_containers_feature_content_source,source=./aws-cli_0,target=/tmp/build-features-src/aws-cli_0 \
cp -ar /tmp/build-features-src/aws-cli_0 /tmp/dev-container-features \
&& chmod -R 0755 /tmp/dev-container-features/aws-cli_0 \
&& cd /tmp/dev-container-features/aws-cli_0 \
&& chmod +x ./devcontainer-features-install.sh \
&& ./devcontainer-features-install.sh \
&& rm -rf /tmp/dev-container-features/aws-cli_0

RUN --mount=type=bind,from=dev_containers_feature_content_source,source=./docker-in-docker_1,target=/tmp/build-features-src/docker-in-docker_1 \
cp -ar /tmp/build-features-src/docker-in-docker_1 /tmp/dev-container-features \
&& chmod -R 0755 /tmp/dev-container-features/docker-in-docker_1 \
&& cd /tmp/dev-container-features/docker-in-docker_1 \
&& chmod +x ./devcontainer-features-install.sh \
&& ./devcontainer-features-install.sh \
&& rm -rf /tmp/dev-container-features/docker-in-docker_1


ARG _DEV_CONTAINERS_IMAGE_USER=root
USER $_DEV_CONTAINERS_IMAGE_USER

# Ensure that /etc/profile does not clobber the existing path
RUN sed -i -E 's/((^|\s)PATH=)([^\$]*)$/\1\${PATH:-\3}/g' /etc/profile || true


ENV DOCKER_BUILDKIT=1
"#
    );
}

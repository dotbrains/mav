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

fn test_project_filename() -> String {
    PathBuf::from(TEST_PROJECT_PATH)
        .file_name()
        .expect("is valid")
        .display()
        .to_string()
}

async fn init_devcontainer_config(
    fs: &Arc<FakeFs>,
    devcontainer_contents: &str,
) -> DevContainerConfig {
    fs.insert_tree(
        format!("{TEST_PROJECT_PATH}/.devcontainer"),
        serde_json::json!({"devcontainer.json": devcontainer_contents}),
    )
    .await;

    DevContainerConfig::default_config()
}

struct TestDependencies {
    fs: Arc<FakeFs>,
    _http_client: Arc<dyn HttpClient>,
    docker: Arc<FakeDocker>,
    command_runner: Arc<TestCommandRunner>,
}

async fn init_default_devcontainer_manifest(
    cx: &mut TestAppContext,
    devcontainer_contents: &str,
) -> Result<(TestDependencies, DevContainerManifest), DevContainerError> {
    let fs = FakeFs::new(cx.executor());
    let http_client = fake_http_client();
    let command_runner = Arc::new(TestCommandRunner::new());
    let docker = Arc::new(FakeDocker::new());
    let environment = HashMap::new();

    init_devcontainer_manifest(
        cx,
        fs,
        http_client,
        docker,
        command_runner,
        environment,
        devcontainer_contents,
    )
    .await
}

async fn init_devcontainer_manifest(
    cx: &mut TestAppContext,
    fs: Arc<FakeFs>,
    http_client: Arc<dyn HttpClient>,
    docker_client: Arc<FakeDocker>,
    command_runner: Arc<TestCommandRunner>,
    environment: HashMap<String, String>,
    devcontainer_contents: &str,
) -> Result<(TestDependencies, DevContainerManifest), DevContainerError> {
    let local_config = init_devcontainer_config(&fs, devcontainer_contents).await;
    let project_path = SanitizedPath::new_arc(&PathBuf::from(TEST_PROJECT_PATH));
    let worktree_store =
        cx.new(|_cx| WorktreeStore::local(false, fs.clone(), WorktreeIdCounter::default()));
    let project_environment =
        cx.new(|cx| ProjectEnvironment::new(None, worktree_store.downgrade(), None, false, cx));

    let context = DevContainerContext {
        project_directory: SanitizedPath::cast_arc(project_path),
        use_podman: false,
        use_buildkit: None,
        fs: fs.clone(),
        http_client: http_client.clone(),
        environment: project_environment.downgrade(),
    };

    let test_dependencies = TestDependencies {
        fs: fs.clone(),
        _http_client: http_client.clone(),
        docker: docker_client.clone(),
        command_runner: command_runner.clone(),
    };
    let manifest = DevContainerManifest::new(
        &context,
        environment,
        docker_client,
        command_runner,
        local_config,
        &PathBuf::from(TEST_PROJECT_PATH),
    )
    .await?;

    Ok((test_dependencies, manifest))
}

#[gpui::test]
async fn test_nonremote_variable_replacement_with_explicit_mount(cx: &mut TestAppContext) {
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
                    "LOCAL_WORKSPACE_FOLDER": "${localWorkspaceFolder}"

                },
                "workspaceMount": "source=/local/folder,target=/workspace/subfolder,type=bind,consistency=cached",
                "workspaceFolder": "/workspace/customfolder"
            }
        "#;

    let (_, mut devcontainer_manifest) =
        init_default_devcontainer_manifest(cx, given_devcontainer_contents)
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
        Some(&"customfolder".to_string())
    );

    // ${localWorkspaceFolderBasename}
    assert_eq!(
        variable_replaced_devcontainer
            .remote_env
            .as_ref()
            .and_then(|env| env.get("LOCAL_WORKSPACE_FOLDER_BASENAME")),
        Some(&"project".to_string())
    );

    // ${containerWorkspaceFolder}
    assert_eq!(
        variable_replaced_devcontainer
            .remote_env
            .as_ref()
            .and_then(|env| env.get("REMOTE_WORKSPACE_FOLDER")),
        Some(&"/workspace/customfolder".to_string())
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
}

#[gpui::test]
async fn test_spawns_devcontainer_with_docker_compose_and_podman(cx: &mut TestAppContext) {
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
      // "forwardPorts": [5432],

      // Use 'postCreateCommand' to run commands after the container is created.
      // "postCreateCommand": "rustc --version",

      // Configure tool-specific properties.
      // "customizations": {},

      // Uncomment to connect as root instead. More info: https://aka.ms/dev-containers-non-root.
      // "remoteUser": "root"
    }
    "#;
    let mut fake_docker = FakeDocker::new();
    fake_docker.set_podman(true);
    let (test_dependencies, mut devcontainer_manifest) = init_devcontainer_manifest(
        cx,
        FakeFs::new(cx.executor()),
        fake_http_client(),
        Arc::new(fake_docker),
        Arc::new(TestCommandRunner::new()),
        HashMap::new(),
        given_devcontainer_contents,
    )
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

FROM dev_container_feature_content_temp as dev_containers_feature_content_source

FROM $_DEV_CONTAINERS_BASE_IMAGE AS dev_containers_feature_content_normalize
USER root
COPY --from=dev_containers_feature_content_source /tmp/build-features/devcontainer-features.builtin.env /tmp/build-features/
RUN chmod -R 0755 /tmp/build-features/

FROM $_DEV_CONTAINERS_BASE_IMAGE AS dev_containers_target_stage

USER root

RUN mkdir -p /tmp/dev-container-features
COPY --from=dev_containers_feature_content_normalize /tmp/build-features/ /tmp/dev-container-features

RUN \
echo "_CONTAINER_USER_HOME=$( (command -v getent >/dev/null 2>&1 && getent passwd 'root' || grep -E '^root|^[^:]*:[^:]*:root:' /etc/passwd || true) | cut -d: -f6)" >> /tmp/dev-container-features/devcontainer-features.builtin.env && \
echo "_REMOTE_USER_HOME=$( (command -v getent >/dev/null 2>&1 && getent passwd 'vscode' || grep -E '^vscode|^[^:]*:[^:]*:vscode:' /etc/passwd || true) | cut -d: -f6)" >> /tmp/dev-container-features/devcontainer-features.builtin.env


COPY --chown=root:root --from=dev_containers_feature_content_source /tmp/build-features/aws-cli_0 /tmp/dev-container-features/aws-cli_0
RUN chmod -R 0755 /tmp/dev-container-features/aws-cli_0 \
&& cd /tmp/dev-container-features/aws-cli_0 \
&& chmod +x ./devcontainer-features-install.sh \
&& ./devcontainer-features-install.sh

COPY --chown=root:root --from=dev_containers_feature_content_source /tmp/build-features/docker-in-docker_1 /tmp/dev-container-features/docker-in-docker_1
RUN chmod -R 0755 /tmp/dev-container-features/docker-in-docker_1 \
&& cd /tmp/dev-container-features/docker-in-docker_1 \
&& chmod +x ./devcontainer-features-install.sh \
&& ./devcontainer-features-install.sh


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
"#
    );
}

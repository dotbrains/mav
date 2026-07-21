use super::*;

#[gpui::test]
async fn test_spawns_devcontainer_with_docker_compose(cx: &mut TestAppContext) {
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

    let docker_commands = test_dependencies
        .command_runner
        .commands_by_program("docker");
    let compose_up = docker_commands
        .iter()
        .find(|c| {
            c.args.first().map(String::as_str) == Some("compose")
                && c.args.iter().any(|a| a == "up")
        })
        .expect("docker compose up command recorded");
    let project_name_idx = compose_up
        .args
        .iter()
        .position(|a| a == "--project-name")
        .expect("compose command has --project-name flag");
    assert_eq!(
        compose_up.args[project_name_idx + 1],
        "project_devcontainer",
        "compose project name should match @devcontainers/cli derivation \
         (${{folderBasename}}_devcontainer), ignoring devcontainer.json `name`"
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

    let build_override = files
        .iter()
        .find(|f| {
            f.file_name()
                .is_some_and(|s| s.display().to_string() == "docker_compose_build.json")
        })
        .expect("to be found");
    let build_override = test_dependencies.fs.load(build_override).await.unwrap();
    let build_config: DockerComposeConfig = serde_json_lenient::from_str(&build_override).unwrap();
    let build_context = build_config
        .services
        .get("app")
        .and_then(|s| s.build.as_ref())
        .and_then(|b| b.context.clone())
        .expect("build override should have a context");
    assert_eq!(
        build_context, ".",
        "build override should preserve the original build context from docker-compose.yml"
    );

    let runtime_override = files
        .iter()
        .find(|f| {
            f.file_name()
                .is_some_and(|s| s.display().to_string() == "docker_compose_runtime.json")
        })
        .expect("to be found");
    let runtime_override = test_dependencies.fs.load(runtime_override).await.unwrap();

    let expected_runtime_override = DockerComposeConfig {
        name: None,
        services: HashMap::from([
            (
                "app".to_string(),
                DockerComposeService {
                    entrypoint: Some(vec![
                        "/bin/sh".to_string(),
                        "-c".to_string(),
                        "echo Container started\ntrap \"exit 0\" 15\n/usr/local/share/docker-init.sh\nexec \"$@\"\nwhile sleep 1 & wait $!; do :; done".to_string(),
                        "-".to_string(),
                    ]),
                    cap_add: Some(vec!["SYS_PTRACE".to_string()]),
                    security_opt: Some(vec!["seccomp=unconfined".to_string()]),
                    privileged: Some(true),
                    labels: Some(HashMap::from([
                        ("devcontainer.metadata".to_string(), "[{\"remoteUser\":\"vscode\"}]".to_string()),
                        ("devcontainer.local_folder".to_string(), "/path/to/local/project".to_string()),
                        ("devcontainer.config_file".to_string(), "/path/to/local/project/.devcontainer/devcontainer.json".to_string())
                    ])),
                    volumes: vec![
                        MountDefinition {
                            source: Some("dind-var-lib-docker-42dad4b4ca7b8ced".to_string()),
                            target: "/var/lib/docker".to_string(),
                            mount_type: Some("volume".to_string())
                        }
                    ],
                    ..Default::default()
                },
            ),
            (
                "db".to_string(),
                DockerComposeService {
                    ports: vec![
                        DockerComposeServicePort {
                            target: "8083".to_string(),
                            published: "8083".to_string(),
                            ..Default::default()
                        },
                        DockerComposeServicePort {
                            target: "5432".to_string(),
                            published: "5432".to_string(),
                            ..Default::default()
                        },
                        DockerComposeServicePort {
                            target: "1234".to_string(),
                            published: "1234".to_string(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            ),
        ]),
        volumes: HashMap::from([(
            "dind-var-lib-docker-42dad4b4ca7b8ced".to_string(),
            DockerComposeVolume {
                name: Some("dind-var-lib-docker-42dad4b4ca7b8ced".to_string()),
            },
        )]),
    };

    assert_eq!(
        serde_json_lenient::from_str::<DockerComposeConfig>(&runtime_override).unwrap(),
        expected_runtime_override
    )
}

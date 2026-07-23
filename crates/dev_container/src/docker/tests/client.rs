use std::{
    ffi::OsStr,
    process::{ExitStatus, Output},
};

use crate::{command_json::deserialize_json_output, devcontainer_api::DevContainerError};

use super::*;

#[test]
fn use_buildkit_setting_overrides_buildx_detection() {
    // `Some(_)` short-circuits the `buildx version` probe, so these run
    // without invoking docker.
    let forced_off = futures::executor::block_on(Docker::new("docker", Some(false)));
    assert!(
        !forced_off.supports_compose_buildkit(),
        "use_buildkit=false must force the classic builder"
    );

    let forced_on = futures::executor::block_on(Docker::new("docker", Some(true)));
    assert!(
        forced_on.supports_compose_buildkit(),
        "use_buildkit=true must enable BuildKit"
    );

    // podman never supports the BuildKit/buildx path, regardless of the setting.
    let podman = futures::executor::block_on(Docker::new("podman", Some(true)));
    assert!(!podman.supports_compose_buildkit());
}

#[test]
fn should_create_docker_inspect_command() {
    let docker = Docker {
        docker_cli: "docker".to_string(),
        has_buildx: false,
    };
    let given_id = "given_docker_id";

    let command = docker.create_docker_inspect(given_id);

    assert_eq!(
        command.get_args().collect::<Vec<&OsStr>>(),
        vec![
            OsStr::new("inspect"),
            OsStr::new("--format={{json . }}"),
            OsStr::new(given_id)
        ]
    )
}

#[test]
fn should_deserialize_docker_ps_with_filters() {
    // First, deserializes empty
    let empty_output = Output {
        status: ExitStatus::default(),
        stderr: vec![],
        stdout: String::from("").into_bytes(),
    };

    let result: Option<DockerPs> = deserialize_json_output(empty_output).unwrap();

    assert!(result.is_none());

    let full_output = Output {
                status: ExitStatus::default(),
                stderr: vec![],
                stdout: String::from(r#"
    {
        "Command": "\"/bin/sh -c 'echo Co…\"",
        "CreatedAt": "2026-02-04 15:44:21 -0800 PST",
        "ID": "abdb6ab59573",
        "Image": "mcr.microsoft.com/devcontainers/base:ubuntu",
        "Labels": "desktop.docker.io/mounts/0/Source=/somepath/cli,desktop.docker.io/mounts/0/SourceKind=hostFile,desktop.docker.io/mounts/0/Target=/workspaces/cli,desktop.docker.io/ports.scheme=v2,dev.containers.features=common,dev.containers.id=base-ubuntu,dev.containers.release=v0.4.24,dev.containers.source=https://github.com/devcontainers/images,dev.containers.timestamp=Fri, 30 Jan 2026 16:52:34 GMT,dev.containers.variant=noble,devcontainer.config_file=/somepath/cli/.devcontainer/dev_container_2/devcontainer.json,devcontainer.local_folder=/somepath/cli,devcontainer.metadata=[{\"id\":\"ghcr.io/devcontainers/features/common-utils:2\"},{\"id\":\"ghcr.io/devcontainers/features/git:1\",\"customizations\":{\"vscode\":{\"settings\":{\"github.copilot.chat.codeGeneration.instructions\":[{\"text\":\"This dev container includes an up-to-date version of Git, built from source as needed, pre-installed and available on the `PATH`.\"}]}}}},{\"remoteUser\":\"vscode\"}],org.opencontainers.image.ref.name=ubuntu,org.opencontainers.image.version=24.04,version=2.1.6",
        "LocalVolumes": "0",
        "Mounts": "/host_mnt/User…",
        "Names": "objective_haslett",
        "Networks": "bridge",
        "Platform": {
        "architecture": "arm64",
        "os": "linux"
        },
        "Ports": "",
        "RunningFor": "47 hours ago",
        "Size": "0B",
        "State": "running",
        "Status": "Up 47 hours"
    }
                    "#).into_bytes(),
            };

    let result: Option<DockerPs> = deserialize_json_output(full_output).unwrap();

    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result.id, "abdb6ab59573".to_string());

    // Podman variant (Id, not ID)
    let full_output = Output {
                status: ExitStatus::default(),
                stderr: vec![],
                stdout: String::from(r#"
    {
        "Command": "\"/bin/sh -c 'echo Co…\"",
        "CreatedAt": "2026-02-04 15:44:21 -0800 PST",
        "Id": "abdb6ab59573",
        "Image": "mcr.microsoft.com/devcontainers/base:ubuntu",
        "Labels": "desktop.docker.io/mounts/0/Source=/somepath/cli,desktop.docker.io/mounts/0/SourceKind=hostFile,desktop.docker.io/mounts/0/Target=/workspaces/cli,desktop.docker.io/ports.scheme=v2,dev.containers.features=common,dev.containers.id=base-ubuntu,dev.containers.release=v0.4.24,dev.containers.source=https://github.com/devcontainers/images,dev.containers.timestamp=Fri, 30 Jan 2026 16:52:34 GMT,dev.containers.variant=noble,devcontainer.config_file=/somepath/cli/.devcontainer/dev_container_2/devcontainer.json,devcontainer.local_folder=/somepath/cli,devcontainer.metadata=[{\"id\":\"ghcr.io/devcontainers/features/common-utils:2\"},{\"id\":\"ghcr.io/devcontainers/features/git:1\",\"customizations\":{\"vscode\":{\"settings\":{\"github.copilot.chat.codeGeneration.instructions\":[{\"text\":\"This dev container includes an up-to-date version of Git, built from source as needed, pre-installed and available on the `PATH`.\"}]}}}},{\"remoteUser\":\"vscode\"}],org.opencontainers.image.ref.name=ubuntu,org.opencontainers.image.version=24.04,version=2.1.6",
        "LocalVolumes": "0",
        "Mounts": "/host_mnt/User…",
        "Names": "objective_haslett",
        "Networks": "bridge",
        "Platform": {
        "architecture": "arm64",
        "os": "linux"
        },
        "Ports": "",
        "RunningFor": "47 hours ago",
        "Size": "0B",
        "State": "running",
        "Status": "Up 47 hours"
    }
                    "#).into_bytes(),
            };

    let result: Option<DockerPs> = deserialize_json_output(full_output).unwrap();

    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result.id, "abdb6ab59573".to_string());
}

#[test]
fn parse_find_process_output_none() {
    assert!(matches!(parse_find_process_output(""), Ok(None)));
    assert!(matches!(parse_find_process_output("   \n\n"), Ok(None)));
}

#[test]
fn parse_find_process_output_single() {
    let raw = r#"{"ID":"abc123"}"#;
    let result = parse_find_process_output(raw).expect("single match must parse");
    assert_eq!(result.unwrap().id, "abc123");
}

#[test]
fn parse_find_process_output_multiple_errors() {
    // `docker ps --format={{ json . }}` emits newline-delimited JSON when
    // multiple containers match the filters. The spec expects the
    // identifying labels to be unique per project, so this is an error.
    let raw = "{\"ID\":\"abc\"}\n{\"ID\":\"def\"}\n";
    match parse_find_process_output(raw) {
        Err(DevContainerError::MultipleMatchingContainers(ids)) => {
            assert_eq!(ids, vec!["abc".to_string(), "def".to_string()]);
        }
        other => panic!("expected MultipleMatchingContainers, got {other:?}"),
    }
}

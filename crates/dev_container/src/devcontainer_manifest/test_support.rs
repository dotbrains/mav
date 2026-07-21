use std::{
    collections::HashMap,
    path::PathBuf,
    process::{ExitStatus, Output},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
#[path = "test_http.rs"]
mod test_http;

pub(crate) use test_http::fake_http_client;

use serde_json_lenient::Value;
use util::command::Command;

use crate::{
    command_json::CommandRunner,
    devcontainer_api::DevContainerError,
    devcontainer_json::MountDefinition,
    docker::{
        DockerClient, DockerComposeConfig, DockerComposeService, DockerComposeServiceBuild,
        DockerComposeVolume, DockerConfigLabels, DockerInspect, DockerInspectConfig,
        DockerInspectMount, DockerPs,
    },
};

#[cfg(not(target_os = "windows"))]
pub(crate) const TEST_PROJECT_PATH: &str = "/path/to/local/project";
#[cfg(target_os = "windows")]
pub(crate) const TEST_PROJECT_PATH: &str = r#"C:\\path\to\local\project"#;

pub(crate) struct RecordedExecCommand {
    pub(crate) _container_id: String,
    pub(crate) _remote_folder: String,
    pub(crate) _user: String,
    pub(crate) env: HashMap<String, String>,
    pub(crate) _inner_command: Command,
}

pub(crate) struct FakeDocker {
    exec_commands_recorded: Mutex<Vec<RecordedExecCommand>>,
    podman: bool,
    has_buildx: bool,
    /// When `Some`, `find_process_by_filters` returns
    /// `MultipleMatchingContainers` with these IDs. Used to exercise the
    /// duplicate-container error path.
    duplicate_container_ids: Mutex<Option<Vec<String>>>,
}

impl FakeDocker {
    pub(crate) fn new() -> Self {
        Self {
            podman: false,
            has_buildx: true,
            exec_commands_recorded: Mutex::new(Vec::new()),
            duplicate_container_ids: Mutex::new(None),
        }
    }
    #[cfg(not(target_os = "windows"))]
    fn set_podman(&mut self, podman: bool) {
        self.podman = podman;
    }
    #[cfg(not(target_os = "windows"))]
    fn set_duplicate_container_ids(&self, ids: Vec<String>) {
        *self
            .duplicate_container_ids
            .lock()
            .expect("should be available") = Some(ids);
    }
}

#[async_trait]
impl DockerClient for FakeDocker {
    async fn inspect(&self, id: &String) -> Result<DockerInspect, DevContainerError> {
        if id == "mcr.microsoft.com/devcontainers/typescript-node:1-18-bookworm" {
            return Ok(DockerInspect {
                id: "sha256:610e6cfca95280188b021774f8cf69dd6f49bdb6eebc34c5ee2010f4d51cc104"
                    .to_string(),
                config: DockerInspectConfig {
                    labels: DockerConfigLabels {
                        metadata: Some(vec![HashMap::from([(
                            "remoteUser".to_string(),
                            Value::String("node".to_string()),
                        )])]),
                    },
                    env: Vec::new(),
                    image_user: Some("root".to_string()),
                },
                mounts: None,
                state: None,
            });
        }
        if id == "mcr.microsoft.com/devcontainers/rust:2-1-bookworm" {
            return Ok(DockerInspect {
                id: "sha256:39ad1c7264794d60e3bc449d9d8877a8e486d19ad8fba80f5369def6a2408392"
                    .to_string(),
                config: DockerInspectConfig {
                    labels: DockerConfigLabels {
                        metadata: Some(vec![HashMap::from([(
                            "remoteUser".to_string(),
                            Value::String("vscode".to_string()),
                        )])]),
                    },
                    image_user: Some("root".to_string()),
                    env: Vec::new(),
                },
                mounts: None,
                state: None,
            });
        }
        if id.starts_with("cli_") {
            return Ok(DockerInspect {
                id: "sha256:610e6cfca95280188b021774f8cf69dd6f49bdb6eebc34c5ee2010f4d51cc105"
                    .to_string(),
                config: DockerInspectConfig {
                    labels: DockerConfigLabels {
                        metadata: Some(vec![HashMap::from([(
                            "remoteUser".to_string(),
                            Value::String("node".to_string()),
                        )])]),
                    },
                    image_user: Some("root".to_string()),
                    env: vec!["PATH=/initial/path".to_string()],
                },
                mounts: None,
                state: None,
            });
        }
        if id == "found_docker_ps" {
            return Ok(DockerInspect {
                id: "sha256:610e6cfca95280188b021774f8cf69dd6f49bdb6eebc34c5ee2010f4d51cc105"
                    .to_string(),
                config: DockerInspectConfig {
                    labels: DockerConfigLabels {
                        metadata: Some(vec![HashMap::from([(
                            "remoteUser".to_string(),
                            Value::String("node".to_string()),
                        )])]),
                    },
                    image_user: Some("root".to_string()),
                    env: vec!["PATH=/initial/path".to_string()],
                },
                mounts: Some(vec![DockerInspectMount {
                    source: "/path/to/local/project".to_string(),
                    destination: "/workspaces/project".to_string(),
                }]),
                state: None,
            });
        }
        if id.starts_with("rust_a-") {
            return Ok(DockerInspect {
                id: "sha256:9da65c34ab809e763b13d238fd7a0f129fcabd533627d340f293308cb63620a0"
                    .to_string(),
                config: DockerInspectConfig {
                    labels: DockerConfigLabels {
                        metadata: Some(vec![HashMap::from([(
                            "remoteUser".to_string(),
                            Value::String("vscode".to_string()),
                        )])]),
                    },
                    image_user: Some("root".to_string()),
                    env: Vec::new(),
                },
                mounts: None,
                state: None,
            });
        }
        if id == "test_image:latest" {
            return Ok(DockerInspect {
                id: "sha256:610e6cfca95280188b021774f8cf69dd6f49bdb6eebc34c5ee2010f4d51cc104"
                    .to_string(),
                config: DockerInspectConfig {
                    labels: DockerConfigLabels {
                        metadata: Some(vec![HashMap::from([(
                            "remoteUser".to_string(),
                            Value::String("node".to_string()),
                        )])]),
                    },
                    env: Vec::new(),
                    image_user: Some("root".to_string()),
                },
                mounts: None,
                state: None,
            });
        }

        Err(DevContainerError::DockerNotAvailable)
    }
    async fn get_docker_compose_config(
        &self,
        config_files: &Vec<PathBuf>,
    ) -> Result<Option<DockerComposeConfig>, DevContainerError> {
        let project_path = PathBuf::from(TEST_PROJECT_PATH);
        if config_files.len() == 1
            && config_files.get(0)
                == Some(
                    &project_path
                        .join(".devcontainer")
                        .join("docker-compose.yml"),
                )
        {
            return Ok(Some(DockerComposeConfig {
                name: None,
                services: HashMap::from([
                    (
                        "devcontainer".to_string(),
                        DockerComposeService {
                            image: Some("test_image:latest".to_string()),
                            volumes: vec![MountDefinition {
                                source: Some("../..".to_string()),
                                target: "/workspaces".to_string(),
                                mount_type: Some("bind".to_string()),
                            }],
                            command: vec!["sleep".to_string(), "infinity".to_string()],
                            ..Default::default()
                        },
                    ),
                    (
                        "app".to_string(),
                        DockerComposeService {
                            build: Some(DockerComposeServiceBuild {
                                context: Some(".".to_string()),
                                dockerfile: Some("Dockerfile".to_string()),
                                args: None,
                                additional_contexts: None,
                                target: None,
                            }),
                            volumes: vec![MountDefinition {
                                source: Some("../..".to_string()),
                                target: "/workspaces".to_string(),
                                mount_type: Some("bind".to_string()),
                            }],
                            network_mode: Some("service:db".to_string()),
                            ..Default::default()
                        },
                    ),
                    (
                        "db".to_string(),
                        DockerComposeService {
                            image: Some("postgres:14.1".to_string()),
                            volumes: vec![MountDefinition {
                                source: Some("postgres-data".to_string()),
                                target: "/var/lib/postgresql/data".to_string(),
                                mount_type: Some("volume".to_string()),
                            }],
                            env_file: Some(vec![".env".to_string()]),
                            ..Default::default()
                        },
                    ),
                ]),
                volumes: HashMap::from([(
                    "postgres-data".to_string(),
                    DockerComposeVolume::default(),
                )]),
            }));
        }
        if config_files.len() == 1
            && config_files.get(0)
                == Some(
                    &project_path
                        .join(".devcontainer")
                        .join("docker-compose-context-parent.yml"),
                )
        {
            return Ok(Some(DockerComposeConfig {
                name: None,
                services: HashMap::from([(
                    "app".to_string(),
                    DockerComposeService {
                        build: Some(DockerComposeServiceBuild {
                            context: Some("..".to_string()),
                            dockerfile: Some(
                                PathBuf::from(".devcontainer")
                                    .join("Dockerfile")
                                    .display()
                                    .to_string(),
                            ),
                            args: None,
                            additional_contexts: None,
                            target: None,
                        }),
                        ..Default::default()
                    },
                )]),
                volumes: HashMap::new(),
            }));
        }
        if config_files.len() == 1
            && config_files.get(0)
                == Some(
                    &project_path
                        .join(".devcontainer")
                        .join("docker-compose-with-args.yml"),
                )
        {
            return Ok(Some(DockerComposeConfig {
                name: None,
                services: HashMap::from([(
                    "app".to_string(),
                    DockerComposeService {
                        build: Some(DockerComposeServiceBuild {
                            context: Some(".".to_string()),
                            dockerfile: Some("Dockerfile".to_string()),
                            args: Some(HashMap::from([(
                                "BASE_IMAGE".to_string(),
                                "test_image:latest".to_string(),
                            )])),
                            additional_contexts: None,
                            target: None,
                        }),
                        ..Default::default()
                    },
                )]),
                ..Default::default()
            }));
        }
        if config_files.len() == 1
            && config_files.get(0)
                == Some(
                    &project_path
                        .join(".devcontainer")
                        .join("docker-compose-plain.yml"),
                )
        {
            return Ok(Some(DockerComposeConfig {
                name: None,
                services: HashMap::from([(
                    "app".to_string(),
                    DockerComposeService {
                        image: Some("test_image:latest".to_string()),
                        command: vec!["sleep".to_string(), "infinity".to_string()],
                        ..Default::default()
                    },
                )]),
                ..Default::default()
            }));
        }
        Err(DevContainerError::DockerNotAvailable)
    }
    async fn docker_compose_build(
        &self,
        _config_files: &Vec<PathBuf>,
        _project_name: &str,
        _services: Option<&Vec<String>>,
    ) -> Result<(), DevContainerError> {
        Ok(())
    }
    async fn run_docker_exec(
        &self,
        container_id: &str,
        remote_folder: &str,
        user: &str,
        env: &HashMap<String, String>,
        inner_command: Command,
    ) -> Result<(), DevContainerError> {
        let mut record = self
            .exec_commands_recorded
            .lock()
            .expect("should be available");
        record.push(RecordedExecCommand {
            _container_id: container_id.to_string(),
            _remote_folder: remote_folder.to_string(),
            _user: user.to_string(),
            env: env.clone(),
            _inner_command: inner_command,
        });
        Ok(())
    }
    async fn start_container(&self, _id: &str) -> Result<(), DevContainerError> {
        Err(DevContainerError::DockerNotAvailable)
    }
    async fn find_process_by_filters(
        &self,
        _filters: Vec<String>,
    ) -> Result<Option<DockerPs>, DevContainerError> {
        if let Some(ids) = self
            .duplicate_container_ids
            .lock()
            .expect("should be available")
            .clone()
        {
            return Err(DevContainerError::MultipleMatchingContainers(ids));
        }
        Ok(Some(DockerPs {
            id: "found_docker_ps".to_string(),
        }))
    }
    fn supports_compose_buildkit(&self) -> bool {
        !self.podman && self.has_buildx
    }
    fn docker_cli(&self) -> String {
        if self.podman {
            "podman".to_string()
        } else {
            "docker".to_string()
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TestCommand {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
}

pub(crate) struct TestCommandRunner {
    commands_recorded: Mutex<Vec<TestCommand>>,
}

impl TestCommandRunner {
    fn new() -> Self {
        Self {
            commands_recorded: Mutex::new(Vec::new()),
        }
    }

    fn commands_by_program(&self, program: &str) -> Vec<TestCommand> {
        let record = self.commands_recorded.lock().expect("poisoned");
        record
            .iter()
            .filter(|r| r.program == program)
            .map(|r| r.clone())
            .collect()
    }
}

#[async_trait]
impl CommandRunner for TestCommandRunner {
    async fn run_command(&self, command: &mut Command) -> Result<Output, std::io::Error> {
        let mut record = self.commands_recorded.lock().expect("poisoned");

        record.push(TestCommand {
            program: command.get_program().display().to_string(),
            args: command
                .get_args()
                .map(|a| a.display().to_string())
                .collect(),
        });

        Ok(Output {
            status: ExitStatus::default(),
            stdout: vec![],
            stderr: vec![],
        })
    }
}

use std::collections::HashMap;

use super::*;

#[test]
fn should_deserialize_dockerfile_devcontainer_json() {
    let given_dockerfile_container_json = r#"
            // These are some external comments. serde_lenient should handle them
            {
                // These are some internal comments
                "name": "myDevContainer",
                "remoteUser": "root",
                "forwardPorts": [
                    "db:5432",
                    3000
                ],
                "portsAttributes": {
                    "3000": {
                        "label": "This Port",
                        "onAutoForward": "notify",
                        "elevateIfNeeded": false,
                        "requireLocalPort": true,
                        "protocol": "https"
                    },
                    "db:5432": {
                        "label": "This Port too",
                        "onAutoForward": "silent",
                        "elevateIfNeeded": true,
                        "requireLocalPort": false,
                        "protocol": "http"
                    }
                },
                "otherPortsAttributes": {
                    "label": "Other Ports",
                    "onAutoForward": "openBrowser",
                    "elevateIfNeeded": true,
                    "requireLocalPort": true,
                    "protocol": "https"
                },
                "updateRemoteUserUID": true,
                "remoteEnv": {
                    "MYVAR1": "myvarvalue",
                    "MYVAR2": "myvarothervalue"
                },
                "initializeCommand": ["echo", "initialize_command"],
                "onCreateCommand": "echo on_create_command",
                "updateContentCommand": {
                    "first": "echo update_content_command",
                    "second": ["echo", "update_content_command"]
                },
                "postCreateCommand": ["echo", "post_create_command"],
                "postStartCommand": "echo post_start_command",
                "postAttachCommand": {
                    "something": "echo post_attach_command",
                    "something1": "echo something else",
                },
                "waitFor": "postStartCommand",
                "userEnvProbe": "loginShell",
                "features": {
              		"ghcr.io/devcontainers/features/aws-cli:1": {},
              		"ghcr.io/devcontainers/features/anaconda:1": {}
               	},
                "overrideFeatureInstallOrder": [
                    "ghcr.io/devcontainers/features/anaconda:1",
                    "ghcr.io/devcontainers/features/aws-cli:1"
                ],
                "hostRequirements": {
                    "cpus": 2,
                    "memory": "8gb",
                    "storage": "32gb",
                    // Note that we're not parsing this currently
                    "gpu": true,
                },
                "appPort": 8081,
                "containerEnv": {
                    "MYVAR3": "myvar3",
                    "MYVAR4": "myvar4"
                },
                "containerUser": "myUser",
                "mounts": [
                    {
                        "source": "/localfolder/app",
                        "target": "/workspaces/app",
                        "type": "volume"
                    },
                    "source=dev-containers-cli-bashhistory,target=/home/node/commandhistory",
                ],
                "runArgs": [
                    "-c",
                    "some_command"
                ],
                "shutdownAction": "stopContainer",
                "overrideCommand": true,
                "workspaceFolder": "/workspaces",
                "workspaceMount": "source=/folder,target=/workspace,type=bind,consistency=cached",
                "build": {
                   	"dockerfile": "DockerFile",
                   	"context": "..",
                   	"args": {
                   	    "MYARG": "MYVALUE"
                   	},
                   	"options": [
                   	    "--some-option",
                   	    "--mount"
                   	],
                   	"target": "development",
                   	"cacheFrom": "some_image"
                }
            }
            "#;

    let result = deserialize_devcontainer_json(given_dockerfile_container_json);

    assert!(result.is_ok());
    let devcontainer = result.expect("ok");
    assert_eq!(
        devcontainer,
        DevContainer {
            name: Some(String::from("myDevContainer")),
            remote_user: Some(String::from("root")),
            forward_ports: Some(vec![
                ForwardPort::String("db:5432".to_string()),
                ForwardPort::Number(3000),
            ]),
            ports_attributes: Some(HashMap::from([
                (
                    "3000".to_string(),
                    PortAttributes {
                        label: Some("This Port".to_string()),
                        on_auto_forward: OnAutoForward::Notify,
                        elevate_if_needed: false,
                        require_local_port: true,
                        protocol: Some(PortAttributeProtocol::Https)
                    }
                ),
                (
                    "db:5432".to_string(),
                    PortAttributes {
                        label: Some("This Port too".to_string()),
                        on_auto_forward: OnAutoForward::Silent,
                        elevate_if_needed: true,
                        require_local_port: false,
                        protocol: Some(PortAttributeProtocol::Http)
                    }
                )
            ])),
            other_ports_attributes: Some(PortAttributes {
                label: Some("Other Ports".to_string()),
                on_auto_forward: OnAutoForward::OpenBrowser,
                elevate_if_needed: true,
                require_local_port: true,
                protocol: Some(PortAttributeProtocol::Https)
            }),
            update_remote_user_uid: Some(true),
            remote_env: Some(HashMap::from([
                ("MYVAR1".to_string(), "myvarvalue".to_string()),
                ("MYVAR2".to_string(), "myvarothervalue".to_string())
            ])),
            initialize_command: Some(LifecycleScript::from_args(vec![
                "echo".to_string(),
                "initialize_command".to_string()
            ])),
            on_create_command: Some(LifecycleScript::from_str("echo on_create_command")),
            update_content_command: Some(LifecycleScript::from_map(HashMap::from([
                (
                    "first".to_string(),
                    vec!["echo".to_string(), "update_content_command".to_string()]
                ),
                (
                    "second".to_string(),
                    vec!["echo".to_string(), "update_content_command".to_string()]
                )
            ]))),
            post_create_command: Some(LifecycleScript::from_str("echo post_create_command")),
            post_start_command: Some(LifecycleScript::from_args(vec![
                "echo".to_string(),
                "post_start_command".to_string()
            ])),
            post_attach_command: Some(LifecycleScript::from_map(HashMap::from([
                (
                    "something".to_string(),
                    vec!["echo".to_string(), "post_attach_command".to_string()]
                ),
                (
                    "something1".to_string(),
                    vec![
                        "echo".to_string(),
                        "something".to_string(),
                        "else".to_string()
                    ]
                )
            ]))),
            wait_for: Some(LifecycleCommand::PostStartCommand),
            user_env_probe: Some(UserEnvProbe::LoginShell),
            features: Some(HashMap::from([
                (
                    "ghcr.io/devcontainers/features/aws-cli:1".to_string(),
                    FeatureOptions::Options(HashMap::new())
                ),
                (
                    "ghcr.io/devcontainers/features/anaconda:1".to_string(),
                    FeatureOptions::Options(HashMap::new())
                )
            ])),
            override_feature_install_order: Some(vec![
                "ghcr.io/devcontainers/features/anaconda:1".to_string(),
                "ghcr.io/devcontainers/features/aws-cli:1".to_string()
            ]),
            host_requirements: Some(HostRequirements {
                cpus: Some(2),
                memory: Some("8gb".to_string()),
                storage: Some("32gb".to_string()),
            }),
            app_port: vec!["8081:8081".to_string()],
            container_env: Some(HashMap::from([
                ("MYVAR3".to_string(), "myvar3".to_string()),
                ("MYVAR4".to_string(), "myvar4".to_string())
            ])),
            container_user: Some("myUser".to_string()),
            mounts: Some(vec![
                MountDefinition {
                    source: Some("/localfolder/app".to_string()),
                    target: "/workspaces/app".to_string(),
                    mount_type: Some("volume".to_string()),
                },
                MountDefinition {
                    source: Some("dev-containers-cli-bashhistory".to_string()),
                    target: "/home/node/commandhistory".to_string(),
                    mount_type: None,
                }
            ]),
            run_args: Some(vec!["-c".to_string(), "some_command".to_string()]),
            shutdown_action: Some(ShutdownAction::StopContainer),
            override_command: Some(true),
            workspace_folder: Some("/workspaces".to_string()),
            workspace_mount: Some(MountDefinition {
                source: Some("/folder".to_string()),
                target: "/workspace".to_string(),
                mount_type: Some("bind".to_string())
            }),
            build: Some(ContainerBuild {
                dockerfile: "DockerFile".to_string(),
                context: Some("..".to_string()),
                args: Some(HashMap::from([(
                    "MYARG".to_string(),
                    "MYVALUE".to_string()
                )])),
                options: Some(vec!["--some-option".to_string(), "--mount".to_string()]),
                target: Some("development".to_string()),
                cache_from: Some(vec!["some_image".to_string()]),
            }),
            ..Default::default()
        }
    );

    assert_eq!(
        devcontainer.build_type(),
        DevContainerBuildType::Dockerfile(ContainerBuild {
            dockerfile: "DockerFile".to_string(),
            context: Some("..".to_string()),
            args: Some(HashMap::from([(
                "MYARG".to_string(),
                "MYVALUE".to_string()
            )])),
            options: Some(vec!["--some-option".to_string(), "--mount".to_string()]),
            target: Some("development".to_string()),
            cache_from: Some(vec!["some_image".to_string()]),
        })
    );
}

use std::collections::HashMap;

use crate::devcontainer_json::MountDefinition;

use super::*;

#[test]
fn should_parse_simple_label() {
    let json = r#"{"volumes": [], "labels": ["com.example.key=value"]}"#;
    let service: DockerComposeService = serde_json_lenient::from_str(json).unwrap();
    let labels = service.labels.unwrap();
    assert_eq!(labels.get("com.example.key").unwrap(), "value");
}

#[test]
fn should_parse_label_with_equals_in_value() {
    let json = r#"{"volumes": [], "labels": ["com.example.key=value=with=equals"]}"#;
    let service: DockerComposeService = serde_json_lenient::from_str(json).unwrap();
    let labels = service.labels.unwrap();
    assert_eq!(labels.get("com.example.key").unwrap(), "value=with=equals");
}

#[test]
fn should_deserialize_docker_compose_config() {
    let given_config = r#"
    {
        "name": "devcontainer",
        "networks": {
        "default": {
            "name": "devcontainer_default",
            "ipam": {}
        }
        },
        "services": {
            "app": {
                "command": [
                "sleep",
                "infinity"
                ],
                "depends_on": {
                "db": {
                    "condition": "service_started",
                    "restart": true,
                    "required": true
                }
                },
                "entrypoint": null,
                "environment": {
                "POSTGRES_DB": "postgres",
                "POSTGRES_HOSTNAME": "localhost",
                "POSTGRES_PASSWORD": "postgres",
                "POSTGRES_PORT": "5432",
                "POSTGRES_USER": "postgres"
                },
                "ports": [
                    {
                        "target": "5443",
                        "published": "5442"
                    },
                    {
                        "name": "custom port",
                        "protocol": "udp",
                        "host_ip": "127.0.0.1",
                        "app_protocol": "http",
                        "mode": "host",
                        "target": "8081",
                        "published": "8083"

                    }
                ],
                "image": "mcr.microsoft.com/devcontainers/rust:2-1-bookworm",
                "network_mode": "service:db",
                "volumes": [
                {
                    "type": "bind",
                    "source": "/path/to",
                    "target": "/workspaces",
                    "bind": {
                    "create_host_path": true
                    }
                }
                ]
            },
            "db": {
                "command": null,
                "entrypoint": null,
                "environment": {
                "POSTGRES_DB": "postgres",
                "POSTGRES_HOSTNAME": "localhost",
                "POSTGRES_PASSWORD": "postgres",
                "POSTGRES_PORT": "5432",
                "POSTGRES_USER": "postgres"
                },
                "image": "postgres:14.1",
                "networks": {
                "default": null
                },
                "restart": "unless-stopped",
                "volumes": [
                {
                    "type": "volume",
                    "source": "postgres-data",
                    "target": "/var/lib/postgresql/data",
                    "volume": {}
                }
                ]
            }
        },
        "volumes": {
        "postgres-data": {
            "name": "devcontainer_postgres-data"
        }
        }
    }
                "#;

    let docker_compose_config: DockerComposeConfig =
        serde_json_lenient::from_str(given_config).unwrap();

    let expected_config = DockerComposeConfig {
        name: Some("devcontainer".to_string()),
        services: HashMap::from([
            (
                "app".to_string(),
                DockerComposeService {
                    command: vec!["sleep".to_string(), "infinity".to_string()],
                    image: Some("mcr.microsoft.com/devcontainers/rust:2-1-bookworm".to_string()),
                    volumes: vec![MountDefinition {
                        mount_type: Some("bind".to_string()),
                        source: Some("/path/to".to_string()),
                        target: "/workspaces".to_string(),
                    }],
                    network_mode: Some("service:db".to_string()),

                    ports: vec![
                        DockerComposeServicePort {
                            target: "5443".to_string(),
                            published: "5442".to_string(),
                            ..Default::default()
                        },
                        DockerComposeServicePort {
                            target: "8081".to_string(),
                            published: "8083".to_string(),
                            mode: Some("host".to_string()),
                            protocol: Some("udp".to_string()),
                            host_ip: Some("127.0.0.1".to_string()),
                            app_protocol: Some("http".to_string()),
                            name: Some("custom port".to_string()),
                        },
                    ],
                    ..Default::default()
                },
            ),
            (
                "db".to_string(),
                DockerComposeService {
                    image: Some("postgres:14.1".to_string()),
                    volumes: vec![MountDefinition {
                        mount_type: Some("volume".to_string()),
                        source: Some("postgres-data".to_string()),
                        target: "/var/lib/postgresql/data".to_string(),
                    }],
                    ..Default::default()
                },
            ),
        ]),
        volumes: HashMap::from([(
            "postgres-data".to_string(),
            DockerComposeVolume {
                name: Some("devcontainer_postgres-data".to_string()),
            },
        )]),
    };

    assert_eq!(docker_compose_config, expected_config);
}

#[test]
fn should_deserialize_compose_labels_as_map() {
    let given_config = r#"
        {
            "name": "devcontainer",
            "services": {
                "app": {
                    "image": "node:22-alpine",
                    "volumes": [],
                    "labels": {
                        "com.example.test": "value",
                        "another.label": "another-value"
                    }
                }
            }
        }
        "#;

    let config: DockerComposeConfig = serde_json_lenient::from_str(given_config).unwrap();
    let service = config.services.get("app").unwrap();
    let labels = service.labels.clone().unwrap();
    assert_eq!(
        labels,
        HashMap::from([
            ("another.label".to_string(), "another-value".to_string()),
            ("com.example.test".to_string(), "value".to_string())
        ])
    );
}

#[test]
fn should_deserialize_compose_labels_as_array() {
    let given_config = r#"
        {
            "name": "devcontainer",
            "services": {
                "app": {
                    "image": "node:22-alpine",
                    "volumes": [],
                    "labels": ["com.example.test=value"]
                }
            }
        }
        "#;

    let config: DockerComposeConfig = serde_json_lenient::from_str(given_config).unwrap();
    let service = config.services.get("app").unwrap();
    assert_eq!(
        service.labels,
        Some(HashMap::from([(
            "com.example.test".to_string(),
            "value".to_string()
        )]))
    );
}

#[test]
fn should_deserialize_compose_without_volumes() {
    let given_config = r#"
        {
            "name": "devcontainer",
            "services": {
                "app": {
                    "image": "node:22-alpine",
                    "volumes": []
                }
            }
        }
        "#;

    let config: DockerComposeConfig = serde_json_lenient::from_str(given_config).unwrap();
    assert!(config.volumes.is_empty());
}

#[test]
fn should_deserialize_compose_with_missing_volumes_field() {
    let given_config = r#"
        {
            "name": "devcontainer",
            "services": {
                "sidecar": {
                    "image": "ubuntu:24.04"
                }
            }
        }
        "#;

    let config: DockerComposeConfig = serde_json_lenient::from_str(given_config).unwrap();
    let service = config.services.get("sidecar").unwrap();
    assert!(service.volumes.is_empty());
}

#[test]
fn should_deserialize_compose_volume_without_source() {
    let given_config = r#"
        {
            "name": "devcontainer",
            "services": {
                "app": {
                    "image": "ubuntu:24.04",
                    "volumes": [
                        {
                            "type": "tmpfs",
                            "target": "/tmp"
                        }
                    ]
                }
            }
        }
        "#;

    let config: DockerComposeConfig = serde_json_lenient::from_str(given_config).unwrap();
    let service = config.services.get("app").unwrap();
    assert_eq!(service.volumes.len(), 1);
    assert_eq!(service.volumes[0].source, None);
    assert_eq!(service.volumes[0].target, "/tmp");
    assert_eq!(service.volumes[0].mount_type, Some("tmpfs".to_string()));
}

#[test]
fn should_deserialize_compose_inline_volume_strings() {
    let given_yaml = indoc::indoc! {r#"
            name: devcontainer
            services:
              app:
                image: node:18
                volumes:
                  - postgres-data:/var/lib/postgresql/data
                  - /host/path:/container/path
                  - /anonymous/volume
                  - type: bind
                    source: /explicit
                    target: /mnt/explicit
            volumes:
              postgres-data:
                name: devcontainer_postgres-data
        "#};

    let config: DockerComposeConfig = serde_yaml::from_str(given_yaml).unwrap();
    let service = config.services.get("app").unwrap();
    assert_eq!(service.volumes.len(), 4);

    assert_eq!(service.volumes[0].source, Some("postgres-data".to_string()));
    assert_eq!(service.volumes[0].target, "/var/lib/postgresql/data");
    assert_eq!(service.volumes[0].mount_type, None);

    assert_eq!(service.volumes[1].source, Some("/host/path".to_string()));
    assert_eq!(service.volumes[1].target, "/container/path");

    assert_eq!(service.volumes[2].source, None);
    assert_eq!(service.volumes[2].target, "/anonymous/volume");

    assert_eq!(service.volumes[3].source, Some("/explicit".to_string()));
    assert_eq!(service.volumes[3].target, "/mnt/explicit");
    assert_eq!(service.volumes[3].mount_type, Some("bind".to_string()));
}

#[test]
fn should_deserialize_compose_top_level_volumes_with_null_value() {
    let given_yaml = indoc::indoc! {r#"
            name: devcontainer
            services:
              app:
                image: node:18
            volumes:
              postgres-data:
              named-vol:
                name: custom-name
        "#};

    let config: DockerComposeConfig = serde_yaml::from_str(given_yaml).unwrap();
    assert_eq!(config.volumes.len(), 2);

    let bare = config
        .volumes
        .get("postgres-data")
        .expect("bare volume should exist");
    assert_eq!(bare.name, None);

    let named = config
        .volumes
        .get("named-vol")
        .expect("named volume should exist");
    assert_eq!(named.name, Some("custom-name".to_string()));
}

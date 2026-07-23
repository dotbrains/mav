use crate::devcontainer_api::DevContainerError;

use super::*;

#[test]
fn should_deserialize_app_port_array() {
    let given_json = r#"
            // These are some external comments. serde_lenient should handle them
            {
                // These are some internal comments
                "name": "myDevContainer",
                "remoteUser": "root",
                "appPort": [
                    "8081:8083",
                    "9001",
                ],
                "build": {
                   	"dockerfile": "DockerFile",
                }
            }
            "#;

    let result = deserialize_devcontainer_json(given_json);

    assert!(result.is_ok());
    let devcontainer = result.expect("ok");

    assert_eq!(
        devcontainer.app_port,
        vec!["8081:8083".to_string(), "9001:9001".to_string()]
    )
}

#[test]
fn mount_definition_should_use_bind_type_for_unix_absolute_paths() {
    let mount = MountDefinition {
        source: Some("/home/user/project".to_string()),
        target: "/workspaces/project".to_string(),
        mount_type: None,
    };

    let rendered = mount.to_string();

    assert!(
        rendered.starts_with("type=bind,"),
        "Expected mount type 'bind' for Unix absolute path, but got: {rendered}"
    );
}

#[test]
fn mount_definition_should_use_bind_type_for_windows_unc_paths() {
    let mount = MountDefinition {
        source: Some("\\\\server\\share\\project".to_string()),
        target: "/workspaces/project".to_string(),
        mount_type: None,
    };

    let rendered = mount.to_string();

    assert!(
        rendered.starts_with("type=bind,"),
        "Expected mount type 'bind' for Windows UNC path, but got: {rendered}"
    );
}

#[test]
fn mount_definition_should_use_bind_type_for_windows_absolute_paths() {
    let mount = MountDefinition {
        source: Some("C:\\Users\\mrg\\cli".to_string()),
        target: "/workspaces/cli".to_string(),
        mount_type: None,
    };

    let rendered = mount.to_string();

    assert!(
        rendered.starts_with("type=bind,"),
        "Expected mount type 'bind' for Windows absolute path, but got: {rendered}"
    );
}

#[test]
fn mount_definition_should_omit_source_when_none() {
    let mount = MountDefinition {
        source: None,
        target: "/tmp".to_string(),
        mount_type: Some("tmpfs".to_string()),
    };

    let rendered = mount.to_string();

    assert_eq!(rendered, "type=tmpfs,target=/tmp,consistency=cached");
}

#[test]
fn should_deserialize_port_attributes_with_missing_optional_fields() {
    let json = r#"
        {
            "image": "nginx",
            "portsAttributes": {
                "8080": {
                    "label": "app",
                    "onAutoForward": "silent"
                }
            }
        }
        "#;

    let result = deserialize_devcontainer_json(json);
    assert!(
        result.is_ok(),
        "Expected deserialization to succeed with partial portsAttributes, got: {:?}",
        result.err()
    );

    let devcontainer = result.unwrap();
    let port_attrs = devcontainer.ports_attributes.unwrap();
    let attrs = port_attrs.get("8080").unwrap();
    assert_eq!(attrs.elevate_if_needed, false);
    assert_eq!(attrs.require_local_port, false);
}

#[test]
fn should_deserialize_port_attributes_with_all_fields_omitted() {
    let json = r#"
        {
            "image": "nginx",
            "portsAttributes": {
                "3000": {}
            }
        }
        "#;

    let result = deserialize_devcontainer_json(json);
    assert!(
        result.is_ok(),
        "Expected deserialization to succeed with empty portsAttributes, got: {:?}",
        result.err()
    );

    let devcontainer = result.unwrap();
    let port_attrs = devcontainer.ports_attributes.unwrap();
    let attrs = port_attrs.get("3000").unwrap();
    assert_eq!(attrs.on_auto_forward, OnAutoForward::Notify);
    assert_eq!(attrs.elevate_if_needed, false);
    assert_eq!(attrs.require_local_port, false);
}

#[test]
fn should_fail_validation_with_workspace_mount_only() {
    let given_image_container_json = r#"
            // These are some external comments. serde_lenient should handle them
            {
                // These are some internal comments
                "build": {
                    "dockerfile": "Dockerfile",
                },
                "name": "myDevContainer",
                "workspaceMount": "source=/app,target=/workspaces/app,type=bind,consistency=cached",
                "customizations": {
                    "vscode": {
                        // Just confirm that this can be included and ignored
                    },
                    "mav": {
                        "extensions": [
                            "html"
                        ]
                    }
                }
            }
            "#;

    let result = deserialize_devcontainer_json(given_image_container_json);

    assert!(result.is_ok());
    let devcontainer = result.expect("ok");

    assert_eq!(
        devcontainer.validate_devcontainer_contents(),
        Err(DevContainerError::DevContainerValidationFailed(
            "workspaceMount and workspaceFolder must both be defined, or neither defined"
                .to_string()
        ))
    );
}

#[test]
fn should_fail_validation_with_workspace_folder_only() {
    let given_image_container_json = r#"
            // These are some external comments. serde_lenient should handle them
            {
                // These are some internal comments
                "build": {
                    "dockerfile": "Dockerfile",
                },
                "name": "myDevContainer",
                "workspaceFolder": "/workspaces",
                "customizations": {
                    "vscode": {
                        // Just confirm that this can be included and ignored
                    },
                    "mav": {
                        "extensions": [
                            "html"
                        ]
                    }
                }
            }
            "#;

    let result = deserialize_devcontainer_json(given_image_container_json);

    assert!(result.is_ok());
    let devcontainer = result.expect("ok");

    assert_eq!(
        devcontainer.validate_devcontainer_contents(),
        Err(DevContainerError::DevContainerValidationFailed(
            "workspaceMount and workspaceFolder must both be defined, or neither defined"
                .to_string()
        ))
    );
}

#[test]
fn should_pass_validation_with_workspace_folder_for_docker_compose() {
    let given_image_container_json = r#"
            // These are some external comments. serde_lenient should handle them
            {
                // These are some internal comments
                "dockerComposeFile": "docker-compose-plain.yml",
                "service": "app",
                "name": "myDevContainer",
                "workspaceFolder": "/workspaces",
                "customizations": {
                    "vscode": {
                        // Just confirm that this can be included and ignored
                    },
                    "mav": {
                        "extensions": [
                            "html"
                        ]
                    }
                }
            }
            "#;

    let result = deserialize_devcontainer_json(given_image_container_json);

    assert!(result.is_ok());
    let devcontainer = result.expect("ok");

    assert!(devcontainer.validate_devcontainer_contents().is_ok());
}

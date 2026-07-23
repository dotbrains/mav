use super::*;

#[test]
fn should_parse_simple_env_var() {
    let config = super::DockerInspectConfig {
        labels: super::DockerConfigLabels { metadata: None },
        image_user: None,
        env: vec!["KEY=value".to_string()],
    };

    let map = config.env_as_map().unwrap();
    assert_eq!(map.get("KEY").unwrap(), "value");
}

#[test]
fn should_parse_env_var_with_equals_in_value() {
    let config = super::DockerInspectConfig {
        labels: super::DockerConfigLabels { metadata: None },
        image_user: None,
        env: vec!["COMPLEX=key=val other>=1.0".to_string()],
    };

    let map = config.env_as_map().unwrap();
    assert_eq!(map.get("COMPLEX").unwrap(), "key=val other>=1.0");
}

#[test]
fn should_parse_database_url_with_equals_in_query_string() {
    let config = super::DockerInspectConfig {
        labels: super::DockerConfigLabels { metadata: None },
        image_user: None,
        env: vec![
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
            "TEST_DATABASE_URL=postgres://postgres:postgres@db:5432/mydb?sslmode=disable"
                .to_string(),
        ],
    };

    let map = config.env_as_map().unwrap();
    assert_eq!(
        map.get("TEST_DATABASE_URL").unwrap(),
        "postgres://postgres:postgres@db:5432/mydb?sslmode=disable"
    );
}

#[test]
fn should_skip_env_var_without_equals() {
    let config = super::DockerInspectConfig {
        labels: super::DockerConfigLabels { metadata: None },
        image_user: None,
        env: vec![
            "VALID_KEY=valid_value".to_string(),
            "NO_EQUALS_VAR".to_string(),
            "ANOTHER_VALID=value".to_string(),
        ],
    };

    let map = config.env_as_map().unwrap();
    assert_eq!(map.len(), 2);
    assert_eq!(map.get("VALID_KEY").unwrap(), "valid_value");
    assert_eq!(map.get("ANOTHER_VALID").unwrap(), "value");
    assert!(!map.contains_key("NO_EQUALS_VAR"));
}

#[test]
fn should_deserialize_object_metadata_from_docker_compose_container() {
    // The devcontainer CLI writes metadata as a bare JSON object (not an array)
    // when there is only one metadata entry (e.g. docker-compose with no features).
    // See https://github.com/devcontainers/cli/issues/1054
    let given_config = r#"
    {
      "Id": "dc4e7b8ff4bf",
      "Config": {
        "Labels": {
          "devcontainer.metadata": "{\"remoteUser\":\"ubuntu\"}"
        }
      }
    }
                "#;
    let config = serde_json_lenient::from_str::<DockerInspect>(given_config).unwrap();

    assert!(config.config.labels.metadata.is_some());
    let metadata = config.config.labels.metadata.unwrap();
    assert_eq!(metadata.len(), 1);
    assert!(metadata[0].contains_key("remoteUser"));
    assert_eq!(metadata[0]["remoteUser"], "ubuntu");
}
fn should_deserialize_inspect_without_labels() {
    let given_config = r#"
        {
            "Id": "sha256:abc123",
            "Config": {
                "Env": ["PATH=/usr/bin"],
                "Cmd": ["node"],
                "WorkingDir": "/"
            }
        }
        "#;

    let inspect: DockerInspect = serde_json_lenient::from_str(given_config).unwrap();
    assert!(inspect.config.labels.metadata.is_none());
    assert!(inspect.config.image_user.is_none());
}

#[test]
fn should_deserialize_inspect_with_null_labels() {
    let given_config = r#"
        {
            "Id": "sha256:abc123",
            "Config": {
                "Labels": null,
                "Env": ["PATH=/usr/bin"]
            }
        }
        "#;

    let inspect: DockerInspect = serde_json_lenient::from_str(given_config).unwrap();
    assert!(inspect.config.labels.metadata.is_none());
}

#[test]
fn should_deserialize_inspect_with_labels_but_no_metadata() {
    let given_config = r#"
        {
            "Id": "sha256:abc123",
            "Config": {
                "Labels": {
                    "com.example.test": "value"
                },
                "Env": ["PATH=/usr/bin"]
            }
        }
        "#;

    let inspect: DockerInspect = serde_json_lenient::from_str(given_config).unwrap();
    assert!(inspect.config.labels.metadata.is_none());
}

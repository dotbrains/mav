use super::*;

#[gpui::test]
async fn test_gets_base_image_from_dockerfile(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();
    let given_devcontainer_contents = r#"
        {
          "name": "cli-${devcontainerId}",
          "build": {
            "dockerfile": "Dockerfile",
            "args": {
                "VERSION": "1.22",
            }
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
FROM dontgrabme as build_context
ARG VERSION=1.21
ARG REPOSITORY=mybuild
ARG REGISTRY=docker.io/stuff

ARG IMAGE=${REGISTRY}/${REPOSITORY}:${VERSION}

FROM ${IMAGE} AS devcontainer
                "#
            .trim()
            .to_string(),
        )
        .await
        .unwrap();

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let dockerfile_contents = devcontainer_manifest
        .expanded_dockerfile_content()
        .await
        .unwrap();
    let base_image = image_from_dockerfile(
        dockerfile_contents,
        &devcontainer_manifest
            .dev_container()
            .build
            .as_ref()
            .and_then(|b| b.target.clone()),
    )
    .unwrap();

    assert_eq!(base_image, "docker.io/stuff/mybuild:1.22".to_string());
}

#[gpui::test]
async fn test_gets_base_image_from_dockerfile_with_target_specified(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();
    let given_devcontainer_contents = r#"
        {
          "name": "cli-${devcontainerId}",
          "build": {
            "dockerfile": "Dockerfile",
            "args": {
                "VERSION": "1.22",
            },
            "target": "development"
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
FROM dontgrabme as build_context
ARG VERSION=1.21
ARG REPOSITORY=mybuild
ARG REGISTRY=docker.io/stuff

ARG IMAGE=${REGISTRY}/${REPOSITORY}:${VERSION}
ARG DEV_IMAGE=${REGISTRY}/${REPOSITORY}:latest

FROM ${DEV_IMAGE} AS development
FROM ${IMAGE} AS production
                "#
            .trim()
            .to_string(),
        )
        .await
        .unwrap();

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let dockerfile_contents = devcontainer_manifest
        .expanded_dockerfile_content()
        .await
        .unwrap();
    let base_image = image_from_dockerfile(
        dockerfile_contents,
        &devcontainer_manifest
            .dev_container()
            .build
            .as_ref()
            .and_then(|b| b.target.clone()),
    )
    .unwrap();

    assert_eq!(base_image, "docker.io/stuff/mybuild:latest".to_string());
}

#[gpui::test]
async fn test_expands_args_in_dockerfile(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();
    let given_devcontainer_contents = r#"
        {
          "name": "cli-${devcontainerId}",
          "build": {
            "dockerfile": "Dockerfile",
            "args": {
                "JSON_ARG": "some-value",
                "ELIXIR_VERSION": "1.21",
            }
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
ARG INVALID_FORWARD_REFERENCE=${OTP_VERSION}
ARG ELIXIR_VERSION=1.20.0-rc.4
ARG FOO=foo BAR=bar
ARG FOOBAR=${FOO}${BAR}
ARG OTP_VERSION=28.4.1
ARG DEBIAN_VERSION=trixie-20260316-slim
ARG IMAGE="docker.io/hexpm/elixir:${ELIXIR_VERSION}-erlang-${OTP_VERSION}-debian-${DEBIAN_VERSION}"
ARG NESTED_MAP="{"key1": "val1", "key2": "val2"}"
ARG WRAPPING_MAP={"nested_map": ${NESTED_MAP}}
ARG FROM_JSON=${JSON_ARG}

FROM ${IMAGE} AS devcontainer
                "#
            .trim()
            .to_string(),
        )
        .await
        .unwrap();

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let expanded_dockerfile = devcontainer_manifest
        .expanded_dockerfile_content()
        .await
        .unwrap();

    assert_eq!(
        &expanded_dockerfile,
        r#"
ARG INVALID_FORWARD_REFERENCE=${OTP_VERSION}
ARG ELIXIR_VERSION=1.20.0-rc.4
ARG FOO=foo BAR=bar
ARG FOOBAR=foobar
ARG OTP_VERSION=28.4.1
ARG DEBIAN_VERSION=trixie-20260316-slim
ARG IMAGE="docker.io/hexpm/elixir:1.21-erlang-28.4.1-debian-trixie-20260316-slim"
ARG NESTED_MAP="{"key1": "val1", "key2": "val2"}"
ARG WRAPPING_MAP={"nested_map": {"key1": "val1", "key2": "val2"}}
ARG FROM_JSON=some-value

FROM docker.io/hexpm/elixir:1.21-erlang-28.4.1-debian-trixie-20260316-slim AS devcontainer
        "#
        .trim()
    )
}

#[gpui::test]
async fn test_expands_compose_service_args_in_dockerfile(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();

    let given_devcontainer_contents = r#"
        {
          "dockerComposeFile": "docker-compose-with-args.yml",
          "service": "app",
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
            "FROM ${BASE_IMAGE}\nUSER root\n".to_string(),
        )
        .await
        .unwrap();

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let expanded = devcontainer_manifest
        .expanded_dockerfile_content()
        .await
        .unwrap();

    assert_eq!(expanded, "FROM test_image:latest\nUSER root");

    let base_image =
        image_from_dockerfile(expanded, &None).expect("base image resolves from compose args");
    assert_eq!(base_image, "test_image:latest");
}

#[gpui::test]
async fn test_expands_bare_dollar_args_in_dockerfile(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();
    let given_devcontainer_contents = r#"
        {
          "name": "ruby-devcontainer",
          "build": {
            "dockerfile": "Dockerfile",
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
            // Mirrors real-world Dockerfiles that use bare $VAR instead of ${VAR}.
            // $RUBY_VERSION2 must not be partially replaced when expanding $RUBY_VERSION.
            r#"
ARG RUBY_VERSION=3.4.4
ARG RUBY_VERSION2=3.3.0
FROM ghcr.io/rails/devcontainer/images/ruby:$RUBY_VERSION
RUN echo $RUBY_VERSION2
            "#
            .trim()
            .to_string(),
        )
        .await
        .unwrap();

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let expanded = devcontainer_manifest
        .expanded_dockerfile_content()
        .await
        .unwrap();

    assert_eq!(
        expanded,
        "ARG RUBY_VERSION=3.4.4\nARG RUBY_VERSION2=3.3.0\nFROM ghcr.io/rails/devcontainer/images/ruby:3.4.4\nRUN echo 3.3.0"
    );
}

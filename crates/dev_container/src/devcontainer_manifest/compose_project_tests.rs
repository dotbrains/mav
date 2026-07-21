use super::*;

#[test]
fn derive_project_name_env_wins_over_everything() {
    // CLI precedence rule 1: `COMPOSE_PROJECT_NAME` env var short-circuits
    // every later source (.env, compose name:, basename fallback).
    use crate::devcontainer_manifest::derive_project_name;

    let env = HashMap::from([("COMPOSE_PROJECT_NAME".to_string(), "from_env".to_string())]);
    let got = derive_project_name(
        &env,
        Some("COMPOSE_PROJECT_NAME=from_dotenv\n"),
        Some("from_compose_name"),
        true,
        Some(Path::new(
            "/path/to/local/project/.devcontainer/docker-compose.yml",
        )),
        Path::new("/path/to/local/project"),
        "project",
    );
    assert_eq!(got, "from_env");
}

#[test]
fn derive_project_name_dotenv_wins_over_compose_and_fallback() {
    // CLI precedence rule 2: when no env var is set, the workspace .env's
    // `COMPOSE_PROJECT_NAME=` line wins over the compose config's `name:`
    // field and the basename fallback.
    use crate::devcontainer_manifest::derive_project_name;

    let got = derive_project_name(
        &HashMap::new(),
        Some("# comment\nCOMPOSE_PROJECT_NAME=from_dotenv\n"),
        Some("from_compose_name"),
        true,
        Some(Path::new(
            "/path/to/local/project/.devcontainer/docker-compose.yml",
        )),
        Path::new("/path/to/local/project"),
        "project",
    );
    assert_eq!(got, "from_dotenv");
}

#[test]
fn derive_project_name_compose_name_wins_over_fallback() {
    // CLI precedence rule 3: when neither env nor .env provide a name,
    // the merged compose config's top-level `name:` field takes precedence
    // over the basename fallback. Also covers sanitization (spaces
    // stripped, uppercase lowercased).
    use crate::devcontainer_manifest::derive_project_name;

    let got = derive_project_name(
        &HashMap::new(),
        None,
        Some("My Compose Project"),
        true,
        Some(Path::new(
            "/path/to/local/project/.devcontainer/docker-compose.yml",
        )),
        Path::new("/path/to/local/project"),
        "project",
    );
    assert_eq!(got, "mycomposeproject");
}

#[test]
fn derive_project_name_skips_compose_name_when_not_explicitly_declared() {
    // CLI precedence rule 3 edge case: `docker compose config` injects a
    // default `name: devcontainer` into the merged output whenever no
    // compose fragment declared one. `@devcontainers/cli` ignores that
    // default by tracking per-fragment whether `name:` was declared and
    // skipping rule 3 if none was. The caller conveys that signal via
    // `compose_name_explicitly_declared`; when it's `false`, even a
    // non-empty `compose_config_name` must be skipped so rule 4 applies.
    use crate::devcontainer_manifest::derive_project_name;

    let got = derive_project_name(
        &HashMap::new(),
        None,
        Some("devcontainer"),
        false,
        Some(Path::new(
            "/path/to/myworkspace/.devcontainer/docker-compose.yml",
        )),
        Path::new("/path/to/myworkspace"),
        "myworkspace",
    );
    assert_eq!(got, "myworkspace_devcontainer");
}

#[test]
fn derive_project_name_omits_suffix_when_compose_file_outside_devcontainer_dir() {
    // CLI precedence rule 4: when falling back to the first compose file's
    // directory basename, the `_devcontainer` suffix is only appended when
    // that directory IS `<config>/.devcontainer`. A compose file at the
    // workspace root (as `"dockerComposeFile": "../docker-compose.yml"`
    // produces) must derive to the plain dir basename, not
    // `project_devcontainer` — otherwise Mav diverges from the CLI.
    use crate::devcontainer_manifest::derive_project_name;

    let got = derive_project_name(
        &HashMap::new(),
        None,
        None,
        false,
        Some(Path::new("/path/to/local/project/docker-compose.yml")),
        Path::new("/path/to/local/project"),
        "project",
    );
    assert_eq!(got, "project");
}

#[test]
fn derive_project_name_handles_resolved_paths_from_docker_compose_manifest() {
    // `docker_compose_manifest()` normalizes compose file paths upfront
    // (resolving `..` components from raw `dockerComposeFile` entries like
    // `"subdir/../docker-compose.yml"`) before populating
    // `DockerComposeResources.files`. This test pins the resulting
    // rule-4/rule-5 behavior on those normalized paths: a file
    // semantically under `<workspace>/.devcontainer` takes rule 4, and
    // one that resolves outside it takes rule 5.
    use crate::devcontainer_manifest::derive_project_name;

    // Normalized equivalent of `.devcontainer/subdir/../docker-compose.yml`:
    // rule 4 applies → `${ws}_devcontainer`.
    let got_under = derive_project_name(
        &HashMap::new(),
        None,
        None,
        false,
        Some(Path::new(
            "/path/to/local/project/.devcontainer/docker-compose.yml",
        )),
        Path::new("/path/to/local/project"),
        "project",
    );
    assert_eq!(got_under, "project_devcontainer");

    // Normalized equivalent of `.devcontainer/../docker-compose.yml`:
    // the file sits at the workspace root, so rule 5 applies — plain
    // basename of the parent dir, no suffix.
    let got_escaped = derive_project_name(
        &HashMap::new(),
        None,
        None,
        false,
        Some(Path::new("/path/to/local/project/docker-compose.yml")),
        Path::new("/path/to/local/project"),
        "project",
    );
    assert_eq!(got_escaped, "project");
}

#[test]
fn compose_fragment_declares_name_detects_top_level_name_key() {
    // Block-style top-level key — declared.
    use crate::devcontainer_manifest::compose_fragment_declares_name;

    assert!(compose_fragment_declares_name(
        "name: my-project\nservices:\n  app:\n    image: foo\n"
    ));
    // Indented `name:` belongs to a nested mapping (here a service) and
    // must NOT count as a top-level declaration.
    assert!(!compose_fragment_declares_name(
        "services:\n  app:\n    name: inner\n    image: foo\n"
    ));
    // Comment lines are ignored.
    assert!(!compose_fragment_declares_name(
        "# name: commented-out\nservices: {}\n"
    ));
    // Empty fragment — no declaration.
    assert!(!compose_fragment_declares_name(""));
    // Quoted key — still a top-level declaration. A line scanner that
    // looks for bare `name:` at column 0 would miss this.
    assert!(compose_fragment_declares_name(
        "\"name\": my-project\nservices: {}\n"
    ));
    // Flow-style root mapping — also a top-level declaration. Again a
    // line scanner keyed on block-style layout would miss it.
    assert!(compose_fragment_declares_name(
        "{name: my-project, services: {app: {image: foo}}}\n"
    ));
    // Unparsable fragment falls through to "not declared" (matches the
    // CLI's behavior on parse failure).
    assert!(!compose_fragment_declares_name(": : :\n- - -\n"));
}

#[test]
fn is_missing_file_error_only_accepts_notfound_and_isadirectory() {
    // Mirrors the CLI's narrow `ENOENT`/`EISDIR` swallow in
    // `getProjectName`'s `.env` read. Any other `io::Error` — permission
    // denied, I/O failure, `ENOTDIR`, etc. — must not be classified as
    // "missing" so callers surface the problem instead of silently
    // falling back to a non-canonical project name. Non-`io::Error`
    // anyhow errors must also not be classified as missing.
    use crate::devcontainer_manifest::is_missing_file_error;

    let notfound = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::NotFound));
    assert!(is_missing_file_error(&notfound));

    // EISDIR — `.env` exists as a directory; CLI swallows, so must we.
    let is_a_dir = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::IsADirectory));
    assert!(is_missing_file_error(&is_a_dir));

    // ENOTDIR — a path component isn't a directory; CLI does NOT
    // swallow this (its catch is narrow to ENOENT/EISDIR), so we must
    // propagate it as a real failure.
    let not_a_dir = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::NotADirectory));
    assert!(!is_missing_file_error(&not_a_dir));

    let permission_denied =
        anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
    assert!(!is_missing_file_error(&permission_denied));

    let other_io = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::Other));
    assert!(!is_missing_file_error(&other_io));

    let non_io: anyhow::Error = anyhow::anyhow!("something else");
    assert!(!is_missing_file_error(&non_io));
}

#[test]
fn sanitize_compose_project_name_matches_cli_rules() {
    use crate::devcontainer_manifest::sanitize_compose_project_name;

    // Plain lowercase alnum passes through.
    assert_eq!(
        sanitize_compose_project_name("project_devcontainer"),
        "project_devcontainer"
    );
    // Hyphens survive (unlike safe_id_lower which would replace them with _).
    assert_eq!(
        sanitize_compose_project_name("devcontainer-compose-test_devcontainer"),
        "devcontainer-compose-test_devcontainer"
    );
    // Uppercase letters are lowercased.
    assert_eq!(
        sanitize_compose_project_name("Makermint-Studio_devcontainer"),
        "makermint-studio_devcontainer"
    );
    // Characters outside [-_a-z0-9] are stripped.
    assert_eq!(
        sanitize_compose_project_name("Rust & PostgreSQL_devcontainer"),
        "rustpostgresql_devcontainer"
    );
}

#[test]
fn test_resolve_compose_dockerfile() {
    let compose = Path::new("/project/.devcontainer/docker-compose.yml");

    // Bug case (#53473): context ".." with relative dockerfile
    assert_eq!(
        resolve_compose_dockerfile(compose, Some(".."), ".devcontainer/Dockerfile"),
        Some(PathBuf::from("/project/.devcontainer/Dockerfile")),
    );

    // Compose path containing ".." (as docker_compose_manifest() produces)
    assert_eq!(
        resolve_compose_dockerfile(
            Path::new("/project/.devcontainer/../docker-compose.yml"),
            Some("."),
            "docker/Dockerfile",
        ),
        Some(PathBuf::from("/project/docker/Dockerfile")),
    );

    // Absolute dockerfile returned as-is
    assert_eq!(
        resolve_compose_dockerfile(compose, Some("."), "/absolute/Dockerfile"),
        Some(PathBuf::from("/absolute/Dockerfile")),
    );

    // Absolute context used directly
    assert_eq!(
        resolve_compose_dockerfile(compose, Some("/abs/context"), "Dockerfile"),
        Some(PathBuf::from("/abs/context/Dockerfile")),
    );

    // No context defaults to compose file's directory
    assert_eq!(
        resolve_compose_dockerfile(compose, None, "Dockerfile"),
        Some(PathBuf::from("/project/.devcontainer/Dockerfile")),
    );
}

#[gpui::test]
async fn test_dockerfile_location_with_compose_context_parent(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();

    let given_devcontainer_contents = r#"
        {
          "name": "Test",
          "dockerComposeFile": "docker-compose-context-parent.yml",
          "service": "app",
          "workspaceFolder": "/workspaces/${localWorkspaceFolderBasename}"
        }
        "#;
    let (_, mut devcontainer_manifest) =
        init_default_devcontainer_manifest(cx, given_devcontainer_contents)
            .await
            .unwrap();

    devcontainer_manifest.parse_nonremote_vars().unwrap();

    let expected = PathBuf::from(TEST_PROJECT_PATH)
        .join(".devcontainer")
        .join("Dockerfile");
    assert_eq!(
        devcontainer_manifest.dockerfile_location().await,
        Some(expected)
    );
}

use super::*;

#[gpui::test]
async fn test_spawns_only_requested_compose_services(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    env_logger::try_init().ok();
    let given_devcontainer_contents = r#"
    {
      "name": "Devcontainer and PostgreSQL",
      "dockerComposeFile": "docker-compose.yml",
      "service": "devcontainer",
      "runServices": ["devcontainer", "db"],
      "workspaceFolder": "/workspaces/${localWorkspaceFolderBasename}",
      "updateRemoteUserUID": false
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

x-base: &base
  build:
context: .
dockerfile: Dockerfile
  env_file:
- .env

volumes:
  postgres-data:

services:
  app:
<<: *base
ports:
  - "3000:3000"

  devcontainer:
<<: *base
ports:
  - "3000:3000"
volumes:
  - ../..:/workspaces:cached

  db:
image: postgres:14.1
restart: unless-stopped
volumes:
  - postgres-data:/var/lib/postgresql/data
env_file:
  - .env
    "#
            .trim()
            .to_string(),
        )
        .await
        .unwrap();

    test_dependencies
        .fs
        .atomic_write(
            PathBuf::from(TEST_PROJECT_PATH).join(".devcontainer/Dockerfile"),
            r#"
FROM mcr.microsoft.com/devcontainers/rust:2-1-bookworm

RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
&& apt-get -y install clang lld \
&& apt-get autoremove -y && apt-get clean -y
    "#
            .trim()
            .to_string(),
        )
        .await
        .unwrap();

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
    assert!(
        compose_up.args.ends_with(&[
            "up".to_string(),
            "-d".to_string(),
            "devcontainer".to_string(),
            "db".to_string(),
        ]),
        "compose up should target only the requested service, got: {:?}",
        compose_up.args
    );
}

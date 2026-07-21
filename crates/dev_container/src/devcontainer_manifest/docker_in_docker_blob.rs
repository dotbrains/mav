use http_client::AsyncBody;

#[path = "docker_in_docker_install.rs"]
mod docker_in_docker_install;

async fn build_tarball(content: Vec<(&str, &str)>) -> Vec<u8> {
    let buffer = futures::io::Cursor::new(Vec::new());
    let mut builder = async_tar::Builder::new(buffer);
    for (file_name, content) in content {
        if content.is_empty() {
            let mut header = async_tar::Header::new_gnu();
            header.set_size(0);
            header.set_mode(0o755);
            header.set_entry_type(async_tar::EntryType::Directory);
            header.set_cksum();
            builder
                .append_data(&mut header, file_name, &[] as &[u8])
                .await
                .unwrap();
        } else {
            let data = content.as_bytes();
            let mut header = async_tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_entry_type(async_tar::EntryType::Regular);
            header.set_cksum();
            builder
                .append_data(&mut header, file_name, data)
                .await
                .unwrap();
        }
    }
    let buffer = builder.into_inner().await.unwrap();
    buffer.into_inner()
}

pub(crate) async fn response() -> http::Response<AsyncBody> {
    let response = build_tarball(vec![
("./NOTES.md", r#"
    ## Limitations

    This docker-in-docker Dev Container Feature is roughly based on the [official docker-in-docker wrapper script](https://github.com/moby/moby/blob/master/hack/dind) that is part of the [Moby project](https://mobyproject.org/). With this in mind:
    * As the name implies, the Feature is expected to work when the host is running Docker (or the OSS Moby container engine it is built on). It may be possible to get running in other container engines, but it has not been tested with them.
    * The host and the container must be running on the same chip architecture. You will not be able to use it with an emulated x86 image with Docker Desktop on an Apple Silicon Mac, like in this example:
      ```
      FROM --platform=linux/amd64 mcr.microsoft.com/devcontainers/typescript-node:16
      ```
      See [Issue #219](https://github.com/devcontainers/features/issues/219) for more details.


    ## OS Support

    This Feature should work on recent versions of Debian/Ubuntu-based distributions with the `apt` package manager installed.

    Debian Trixie (13) does not include moby-cli and related system packages, so the feature cannot install with "moby": "true". To use this feature on Trixie, please set "moby": "false" or choose a different base image (for example, Ubuntu 24.04).

    `bash` is required to execute the `install.sh` script."#),
("./README.md", r#"
    # Docker (Docker-in-Docker) (docker-in-docker)

    Create child containers *inside* a container, independent from the host's docker instance. Installs Docker extension in the container along with needed CLIs.

    ## Example Usage

    ```json
    "features": {
        "ghcr.io/devcontainers/features/docker-in-docker:2": {}
    }
    ```

    ## Options

    | Options Id | Description | Type | Default Value |
    |-----|-----|-----|-----|
    | version | Select or enter a Docker/Moby Engine version. (Availability can vary by OS version.) | string | latest |
    | moby | Install OSS Moby build instead of Docker CE | boolean | true |
    | mobyBuildxVersion | Install a specific version of moby-buildx when using Moby | string | latest |
    | dockerDashComposeVersion | Default version of Docker Compose (v1, v2 or none) | string | v2 |
    | azureDnsAutoDetection | Allow automatically setting the dockerd DNS server when the installation script detects it is running in Azure | boolean | true |
    | dockerDefaultAddressPool | Define default address pools for Docker networks. e.g. base=192.168.0.0/16,size=24 | string | - |
    | installDockerBuildx | Install Docker Buildx | boolean | true |
    | installDockerComposeSwitch | Install Compose Switch (provided docker compose is available) which is a replacement to the Compose V1 docker-compose (python) executable. It translates the command line into Compose V2 docker compose then runs the latter. | boolean | false |
    | disableIp6tables | Disable ip6tables (this option is only applicable for Docker versions 27 and greater) | boolean | false |

    ## Customizations

    ### VS Code Extensions

    - `ms-azuretools.vscode-containers`

    ## Limitations

    This docker-in-docker Dev Container Feature is roughly based on the [official docker-in-docker wrapper script](https://github.com/moby/moby/blob/master/hack/dind) that is part of the [Moby project](https://mobyproject.org/). With this in mind:
    * As the name implies, the Feature is expected to work when the host is running Docker (or the OSS Moby container engine it is built on). It may be possible to get running in other container engines, but it has not been tested with them.
    * The host and the container must be running on the same chip architecture. You will not be able to use it with an emulated x86 image with Docker Desktop on an Apple Silicon Mac, like in this example:
      ```
      FROM --platform=linux/amd64 mcr.microsoft.com/devcontainers/typescript-node:16
      ```
      See [Issue #219](https://github.com/devcontainers/features/issues/219) for more details.


    ## OS Support

    This Feature should work on recent versions of Debian/Ubuntu-based distributions with the `apt` package manager installed.

    `bash` is required to execute the `install.sh` script.


    ---

    _Note: This file was auto-generated from the [devcontainer-feature.json](https://github.com/devcontainers/features/blob/main/src/docker-in-docker/devcontainer-feature.json).  Add additional notes to a `NOTES.md`._"#),
("./devcontainer-feature.json", r#"
    {
      "id": "docker-in-docker",
      "version": "2.16.1",
      "name": "Docker (Docker-in-Docker)",
      "documentationURL": "https://github.com/devcontainers/features/tree/main/src/docker-in-docker",
      "description": "Create child containers *inside* a container, independent from the host's docker instance. Installs Docker extension in the container along with needed CLIs.",
      "options": {
        "version": {
          "type": "string",
          "proposals": [
            "latest",
            "none",
            "20.10"
          ],
          "default": "latest",
          "description": "Select or enter a Docker/Moby Engine version. (Availability can vary by OS version.)"
        },
        "moby": {
          "type": "boolean",
          "default": true,
          "description": "Install OSS Moby build instead of Docker CE"
        },
        "mobyBuildxVersion": {
          "type": "string",
          "default": "latest",
          "description": "Install a specific version of moby-buildx when using Moby"
        },
        "dockerDashComposeVersion": {
          "type": "string",
          "enum": [
            "none",
            "v1",
            "v2"
          ],
          "default": "v2",
          "description": "Default version of Docker Compose (v1, v2 or none)"
        },
        "azureDnsAutoDetection": {
          "type": "boolean",
          "default": true,
          "description": "Allow automatically setting the dockerd DNS server when the installation script detects it is running in Azure"
        },
        "dockerDefaultAddressPool": {
          "type": "string",
          "default": "",
          "proposals": [],
          "description": "Define default address pools for Docker networks. e.g. base=192.168.0.0/16,size=24"
        },
        "installDockerBuildx": {
          "type": "boolean",
          "default": true,
          "description": "Install Docker Buildx"
        },
        "installDockerComposeSwitch": {
          "type": "boolean",
          "default": false,
          "description": "Install Compose Switch (provided docker compose is available) which is a replacement to the Compose V1 docker-compose (python) executable. It translates the command line into Compose V2 docker compose then runs the latter."
        },
        "disableIp6tables": {
          "type": "boolean",
          "default": false,
          "description": "Disable ip6tables (this option is only applicable for Docker versions 27 and greater)"
        }
      },
      "entrypoint": "/usr/local/share/docker-init.sh",
      "privileged": true,
      "containerEnv": {
        "DOCKER_BUILDKIT": "1"
      },
      "customizations": {
        "vscode": {
          "extensions": [
            "ms-azuretools.vscode-containers"
          ],
          "settings": {
            "github.copilot.chat.codeGeneration.instructions": [
              {
                "text": "This dev container includes the Docker CLI (`docker`) pre-installed and available on the `PATH` for running and managing containers using a dedicated Docker daemon running inside the dev container."
              }
            ]
          }
        }
      },
      "mounts": [
        {
          "source": "dind-var-lib-docker-${devcontainerId}",
          "target": "/var/lib/docker",
          "type": "volume"
        }
      ],
      "installsAfter": [
        "ghcr.io/devcontainers/features/common-utils"
      ]
    }"#),
("./install.sh", docker_in_docker_install::INSTALL_SH),
    ]).await;

    http::Response::builder()
        .status(200)
        .body(AsyncBody::from(response))
        .unwrap()
}

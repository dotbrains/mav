use std::sync::Arc;

use http_client::{AsyncBody, FakeHttpClient, HttpClient};

use crate::oci::TokenResponse;

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

pub(crate) fn fake_http_client() -> Arc<dyn HttpClient> {
    FakeHttpClient::create(|request| async move {
        let (parts, _body) = request.into_parts();
        if parts.uri.path() == "/token" {
            let token_response = TokenResponse {
                token: "token".to_string(),
            };
            return Ok(http::Response::builder()
                .status(200)
                .body(http_client::AsyncBody::from(
                    serde_json_lenient::to_string(&token_response).unwrap(),
                ))
                .unwrap());
        }

        // OCI specific things
        if parts.uri.path() == "/v2/devcontainers/features/docker-in-docker/manifests/2" {
            let response = r#"
                {
                    "schemaVersion": 2,
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "config": {
                        "mediaType": "application/vnd.devcontainers",
                        "digest": "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
                        "size": 2
                    },
                    "layers": [
                        {
                            "mediaType": "application/vnd.devcontainers.layer.v1+tar",
                            "digest": "sha256:bc7ab0d8d8339416e1491419ab9ffe931458d0130110f4b18351b0fa184e67d5",
                            "size": 59392,
                            "annotations": {
                                "org.opencontainers.image.title": "devcontainer-feature-docker-in-docker.tgz"
                            }
                        }
                    ],
                    "annotations": {
                        "dev.containers.metadata": "{\"id\":\"docker-in-docker\",\"version\":\"2.16.1\",\"name\":\"Docker (Docker-in-Docker)\",\"documentationURL\":\"https://github.com/devcontainers/features/tree/main/src/docker-in-docker\",\"description\":\"Create child containers *inside* a container, independent from the host's docker instance. Installs Docker extension in the container along with needed CLIs.\",\"options\":{\"version\":{\"type\":\"string\",\"proposals\":[\"latest\",\"none\",\"20.10\"],\"default\":\"latest\",\"description\":\"Select or enter a Docker/Moby Engine version. (Availability can vary by OS version.)\"},\"moby\":{\"type\":\"boolean\",\"default\":true,\"description\":\"Install OSS Moby build instead of Docker CE\"},\"mobyBuildxVersion\":{\"type\":\"string\",\"default\":\"latest\",\"description\":\"Install a specific version of moby-buildx when using Moby\"},\"dockerDashComposeVersion\":{\"type\":\"string\",\"enum\":[\"none\",\"v1\",\"v2\"],\"default\":\"v2\",\"description\":\"Default version of Docker Compose (v1, v2 or none)\"},\"azureDnsAutoDetection\":{\"type\":\"boolean\",\"default\":true,\"description\":\"Allow automatically setting the dockerd DNS server when the installation script detects it is running in Azure\"},\"dockerDefaultAddressPool\":{\"type\":\"string\",\"default\":\"\",\"proposals\":[],\"description\":\"Define default address pools for Docker networks. e.g. base=192.168.0.0/16,size=24\"},\"installDockerBuildx\":{\"type\":\"boolean\",\"default\":true,\"description\":\"Install Docker Buildx\"},\"installDockerComposeSwitch\":{\"type\":\"boolean\",\"default\":false,\"description\":\"Install Compose Switch (provided docker compose is available) which is a replacement to the Compose V1 docker-compose (python) executable. It translates the command line into Compose V2 docker compose then runs the latter.\"},\"disableIp6tables\":{\"type\":\"boolean\",\"default\":false,\"description\":\"Disable ip6tables (this option is only applicable for Docker versions 27 and greater)\"}},\"entrypoint\":\"/usr/local/share/docker-init.sh\",\"privileged\":true,\"containerEnv\":{\"DOCKER_BUILDKIT\":\"1\"},\"customizations\":{\"vscode\":{\"extensions\":[\"ms-azuretools.vscode-containers\"],\"settings\":{\"github.copilot.chat.codeGeneration.instructions\":[{\"text\":\"This dev container includes the Docker CLI (`docker`) pre-installed and available on the `PATH` for running and managing containers using a dedicated Docker daemon running inside the dev container.\"}]}}},\"mounts\":[{\"source\":\"dind-var-lib-docker-${devcontainerId}\",\"target\":\"/var/lib/docker\",\"type\":\"volume\"}],\"installsAfter\":[\"ghcr.io/devcontainers/features/common-utils\"]}",
                        "com.github.package.type": "devcontainer_feature"
                    }
                }
                "#;
            return Ok(http::Response::builder()
                .status(200)
                .body(http_client::AsyncBody::from(response))
                .unwrap());
        }

        if parts.uri.path()
            == "/v2/devcontainers/features/docker-in-docker/blobs/sha256:bc7ab0d8d8339416e1491419ab9ffe931458d0130110f4b18351b0fa184e67d5"
        {
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
                ("./install.sh", r#"
                #!/usr/bin/env bash
                #-------------------------------------------------------------------------------------------------------------
                # Copyright (c) Microsoft Corporation. All rights reserved.
                # Licensed under the MIT License. See https://go.microsoft.com/fwlink/?linkid=2090316 for license information.
                #-------------------------------------------------------------------------------------------------------------
                #
                # Docs: https://github.com/microsoft/vscode-dev-containers/blob/main/script-library/docs/docker-in-docker.md
                # Maintainer: The Dev Container spec maintainers


                DOCKER_VERSION="${VERSION:-"latest"}" # The Docker/Moby Engine + CLI should match in version
                USE_MOBY="${MOBY:-"true"}"
                MOBY_BUILDX_VERSION="${MOBYBUILDXVERSION:-"latest"}"
                DOCKER_DASH_COMPOSE_VERSION="${DOCKERDASHCOMPOSEVERSION:-"v2"}" #v1, v2 or none
                AZURE_DNS_AUTO_DETECTION="${AZUREDNSAUTODETECTION:-"true"}"
                DOCKER_DEFAULT_ADDRESS_POOL="${DOCKERDEFAULTADDRESSPOOL:-""}"
                USERNAME="${USERNAME:-"${_REMOTE_USER:-"automatic"}"}"
                INSTALL_DOCKER_BUILDX="${INSTALLDOCKERBUILDX:-"true"}"
                INSTALL_DOCKER_COMPOSE_SWITCH="${INSTALLDOCKERCOMPOSESWITCH:-"false"}"
                MICROSOFT_GPG_KEYS_URI="https://packages.microsoft.com/keys/microsoft.asc"
                MICROSOFT_GPG_KEYS_ROLLING_URI="https://packages.microsoft.com/keys/microsoft-rolling.asc"
                DOCKER_MOBY_ARCHIVE_VERSION_CODENAMES="trixie bookworm buster bullseye bionic focal jammy noble"
                DOCKER_LICENSED_ARCHIVE_VERSION_CODENAMES="trixie bookworm buster bullseye bionic focal hirsute impish jammy noble"
                DISABLE_IP6_TABLES="${DISABLEIP6TABLES:-false}"

                # Default: Exit on any failure.
                set -e

                # Clean up
                rm -rf /var/lib/apt/lists/*

                # Setup STDERR.
                err() {
                    echo "(!) $*" >&2
                }

                if [ "$(id -u)" -ne 0 ]; then
                    err 'Script must be run as root. Use sudo, su, or add "USER root" to your Dockerfile before running this script.'
                    exit 1
                fi

                ###################
                # Helper Functions
                # See: https://github.com/microsoft/vscode-dev-containers/blob/main/script-library/shared/utils.sh
                ###################

                # Determine the appropriate non-root user
                if [ "${USERNAME}" = "auto" ] || [ "${USERNAME}" = "automatic" ]; then
                    USERNAME=""
                    POSSIBLE_USERS=("vscode" "node" "codespace" "$(awk -v val=1000 -F ":" '$3==val{print $1}' /etc/passwd)")
                    for CURRENT_USER in "${POSSIBLE_USERS[@]}"; do
                        if id -u ${CURRENT_USER} > /dev/null 2>&1; then
                            USERNAME=${CURRENT_USER}
                            break
                        fi
                    done
                    if [ "${USERNAME}" = "" ]; then
                        USERNAME=root
                    fi
                elif [ "${USERNAME}" = "none" ] || ! id -u ${USERNAME} > /dev/null 2>&1; then
                    USERNAME=root
                fi

                # Package manager update function
                pkg_mgr_update() {
                    case ${ADJUSTED_ID} in
                        debian)
                            if [ "$(find /var/lib/apt/lists/* | wc -l)" = "0" ]; then
                                echo "Running apt-get update..."
                                apt-get update -y
                            fi
                            ;;
                        rhel)
                            if [ ${PKG_MGR_CMD} = "microdnf" ]; then
                                cache_check_dir="/var/cache/yum"
                            else
                                cache_check_dir="/var/cache/${PKG_MGR_CMD}"
                            fi
                            if [ "$(ls ${cache_check_dir}/* 2>/dev/null | wc -l)" = 0 ]; then
                                echo "Running ${PKG_MGR_CMD} makecache ..."
                                ${PKG_MGR_CMD} makecache
                            fi
                            ;;
                    esac
                }

                # Checks if packages are installed and installs them if not
                check_packages() {
                    case ${ADJUSTED_ID} in
                        debian)
                            if ! dpkg -s "$@" > /dev/null 2>&1; then
                                pkg_mgr_update
                                apt-get -y install --no-install-recommends "$@"
                            fi
                            ;;
                        rhel)
                            if ! rpm -q "$@" > /dev/null 2>&1; then
                                pkg_mgr_update
                                ${PKG_MGR_CMD} -y install "$@"
                            fi
                            ;;
                    esac
                }

                # Figure out correct version of a three part version number is not passed
                find_version_from_git_tags() {
                    local variable_name=$1
                    local requested_version=${!variable_name}
                    if [ "${requested_version}" = "none" ]; then return; fi
                    local repository=$2
                    local prefix=${3:-"tags/v"}
                    local separator=${4:-"."}
                    local last_part_optional=${5:-"false"}
                    if [ "$(echo "${requested_version}" | grep -o "." | wc -l)" != "2" ]; then
                        local escaped_separator=${separator//./\\.}
                        local last_part
                        if [ "${last_part_optional}" = "true" ]; then
                            last_part="(${escaped_separator}[0-9]+)?"
                        else
                            last_part="${escaped_separator}[0-9]+"
                        fi
                        local regex="${prefix}\\K[0-9]+${escaped_separator}[0-9]+${last_part}$"
                        local version_list="$(git ls-remote --tags ${repository} | grep -oP "${regex}" | tr -d ' ' | tr "${separator}" "." | sort -rV)"
                        if [ "${requested_version}" = "latest" ] || [ "${requested_version}" = "current" ] || [ "${requested_version}" = "lts" ]; then
                            declare -g ${variable_name}="$(echo "${version_list}" | head -n 1)"
                        else
                            set +e
                                declare -g ${variable_name}="$(echo "${version_list}" | grep -E -m 1 "^${requested_version//./\\.}([\\.\\s]|$)")"
                            set -e
                        fi
                    fi
                    if [ -z "${!variable_name}" ] || ! echo "${version_list}" | grep "^${!variable_name//./\\.}$" > /dev/null 2>&1; then
                        err "Invalid ${variable_name} value: ${requested_version}\nValid values:\n${version_list}" >&2
                        exit 1
                    fi
                    echo "${variable_name}=${!variable_name}"
                }

                # Use semver logic to decrement a version number then look for the closest match
                find_prev_version_from_git_tags() {
                    local variable_name=$1
                    local current_version=${!variable_name}
                    local repository=$2
                    # Normally a "v" is used before the version number, but support alternate cases
                    local prefix=${3:-"tags/v"}
                    # Some repositories use "_" instead of "." for version number part separation, support that
                    local separator=${4:-"."}
                    # Some tools release versions that omit the last digit (e.g. go)
                    local last_part_optional=${5:-"false"}
                    # Some repositories may have tags that include a suffix (e.g. actions/node-versions)
                    local version_suffix_regex=$6
                    # Try one break fix version number less if we get a failure. Use "set +e" since "set -e" can cause failures in valid scenarios.
                    set +e
                        major="$(echo "${current_version}" | grep -oE '^[0-9]+' || echo '')"
                        minor="$(echo "${current_version}" | grep -oP '^[0-9]+\.\K[0-9]+' || echo '')"
                        breakfix="$(echo "${current_version}" | grep -oP '^[0-9]+\.[0-9]+\.\K[0-9]+' 2>/dev/null || echo '')"

                        if [ "${minor}" = "0" ] && [ "${breakfix}" = "0" ]; then
                            ((major=major-1))
                            declare -g ${variable_name}="${major}"
                            # Look for latest version from previous major release
                            find_version_from_git_tags "${variable_name}" "${repository}" "${prefix}" "${separator}" "${last_part_optional}"
                        # Handle situations like Go's odd version pattern where "0" releases omit the last part
                        elif [ "${breakfix}" = "" ] || [ "${breakfix}" = "0" ]; then
                            ((minor=minor-1))
                            declare -g ${variable_name}="${major}.${minor}"
                            # Look for latest version from previous minor release
                            find_version_from_git_tags "${variable_name}" "${repository}" "${prefix}" "${separator}" "${last_part_optional}"
                        else
                            ((breakfix=breakfix-1))
                            if [ "${breakfix}" = "0" ] && [ "${last_part_optional}" = "true" ]; then
                                declare -g ${variable_name}="${major}.${minor}"
                            else
                                declare -g ${variable_name}="${major}.${minor}.${breakfix}"
                            fi
                        fi
                    set -e
                }

                # Function to fetch the version released prior to the latest version
                get_previous_version() {
                    local url=$1
                    local repo_url=$2
                    local variable_name=$3
                    prev_version=${!variable_name}

                    output=$(curl -s "$repo_url");
                    if echo "$output" | jq -e 'type == "object"' > /dev/null; then
                      message=$(echo "$output" | jq -r '.message')

                      if [[ $message == "API rate limit exceeded"* ]]; then
                            echo -e "\nAn attempt to find latest version using GitHub Api Failed... \nReason: ${message}"
                            echo -e "\nAttempting to find latest version using GitHub tags."
                            find_prev_version_from_git_tags prev_version "$url" "tags/v"
                            declare -g ${variable_name}="${prev_version}"
                       fi
                    elif echo "$output" | jq -e 'type == "array"' > /dev/null; then
                        echo -e "\nAttempting to find latest version using GitHub Api."
                        version=$(echo "$output" | jq -r '.[1].tag_name')
                        declare -g ${variable_name}="${version#v}"
                    fi
                    echo "${variable_name}=${!variable_name}"
                }

                get_github_api_repo_url() {
                    local url=$1
                    echo "${url/https:\/\/github.com/https:\/\/api.github.com\/repos}/releases"
                }

                ###########################################
                # Start docker-in-docker installation
                ###########################################

                # Ensure apt is in non-interactive to avoid prompts
                export DEBIAN_FRONTEND=noninteractive

                # Source /etc/os-release to get OS info
                . /etc/os-release

                # Determine adjusted ID and package manager
                if [ "${ID}" = "debian" ] || [ "${ID_LIKE}" = "debian" ]; then
                    ADJUSTED_ID="debian"
                    PKG_MGR_CMD="apt-get"
                    # Use dpkg for Debian-based systems
                    architecture="$(dpkg --print-architecture 2>/dev/null || uname -m)"
                elif [[ "${ID}" = "rhel" || "${ID}" = "fedora" || "${ID}" = "azurelinux" || "${ID}" = "mariner" || "${ID_LIKE}" = *"rhel"* || "${ID_LIKE}" = *"fedora"* || "${ID_LIKE}" = *"azurelinux"* || "${ID_LIKE}" = *"mariner"* ]]; then
                    ADJUSTED_ID="rhel"
                    # Determine the appropriate package manager for RHEL-based systems
                    for pkg_mgr in tdnf dnf microdnf yum; do
                        if command -v "$pkg_mgr" >/dev/null 2>&1; then
                            PKG_MGR_CMD="$pkg_mgr"
                            break
                        fi
                    done

                    if [ -z "${PKG_MGR_CMD}" ]; then
                        err "Unable to find a supported package manager (tdnf, dnf, microdnf, yum)"
                        exit 1
                    fi

                    architecture="$(rpm --eval '%{_arch}' 2>/dev/null || uname -m)"
                else
                    err "Linux distro ${ID} not supported."
                    exit 1
                fi

                # Azure Linux specific setup
                if [ "${ID}" = "azurelinux" ]; then
                    VERSION_CODENAME="azurelinux${VERSION_ID}"
                fi

                # Prevent attempting to install Moby on Debian trixie (packages removed)
                if [ "${USE_MOBY}" = "true" ] && [ "${ID}" = "debian" ] && [ "${VERSION_CODENAME}" = "trixie" ]; then
                    err "The 'moby' option is not supported on Debian 'trixie' because 'moby-cli' and related system packages have been removed from that distribution."
                    err "To continue, either set the feature option '\"moby\": false' or use a different base image (for example: 'debian:bookworm' or 'ubuntu-24.04')."
                    exit 1
                fi

                # Check if distro is supported
                if [ "${USE_MOBY}" = "true" ]; then
                    if [ "${ADJUSTED_ID}" = "debian" ]; then
                        if [[ "${DOCKER_MOBY_ARCHIVE_VERSION_CODENAMES}" != *"${VERSION_CODENAME}"* ]]; then
                            err "Unsupported distribution version '${VERSION_CODENAME}'. To resolve, either: (1) set feature option '\"moby\": false' , or (2) choose a compatible OS distribution"
                            err "Supported distributions include: ${DOCKER_MOBY_ARCHIVE_VERSION_CODENAMES}"
                            exit 1
                        fi
                        echo "(*) ${VERSION_CODENAME} is supported for Moby installation  - setting up Microsoft repository"
                    elif [ "${ADJUSTED_ID}" = "rhel" ]; then
                        if [ "${ID}" = "azurelinux" ] || [ "${ID}" = "mariner" ]; then
                            echo " (*) ${ID} ${VERSION_ID} detected - using Microsoft repositories for Moby packages"
                        else
                            echo "RHEL-based system (${ID}) detected - Moby packages may require additional configuration"
                        fi
                    fi
                else
                    if [ "${ADJUSTED_ID}" = "debian" ]; then
                        if [[ "${DOCKER_LICENSED_ARCHIVE_VERSION_CODENAMES}" != *"${VERSION_CODENAME}"* ]]; then
                            err "Unsupported distribution version '${VERSION_CODENAME}'. To resolve, please choose a compatible OS distribution"
                            err "Supported distributions include: ${DOCKER_LICENSED_ARCHIVE_VERSION_CODENAMES}"
                            exit 1
                        fi
                        echo "(*) ${VERSION_CODENAME} is supported for Docker CE installation (supported: ${DOCKER_LICENSED_ARCHIVE_VERSION_CODENAMES}) - setting up Docker repository"
                    elif [ "${ADJUSTED_ID}" = "rhel" ]; then

                        echo "RHEL-based system (${ID}) detected - using Docker CE packages"
                    fi
                fi

                # Install base dependencies
                base_packages="curl ca-certificates pigz iptables gnupg2 wget jq"
                case ${ADJUSTED_ID} in
                    debian)
                        check_packages apt-transport-https $base_packages dirmngr
                        ;;
                    rhel)
                        check_packages $base_packages tar gawk shadow-utils policycoreutils  procps-ng systemd-libs systemd-devel

                        ;;
                esac

                # Install git if not already present
                if ! command -v git >/dev/null 2>&1; then
                    check_packages git
                fi

                # Update CA certificates to ensure HTTPS connections work properly
                # This is especially important for Ubuntu 24.04 (Noble) and Debian Trixie
                # Only run for Debian-based systems (RHEL uses update-ca-trust instead)
                if [ "${ADJUSTED_ID}" = "debian" ] && command -v update-ca-certificates > /dev/null 2>&1; then
                    update-ca-certificates
                fi

                # Swap to legacy iptables for compatibility (Debian only)
                if [ "${ADJUSTED_ID}" = "debian" ] && type iptables-legacy > /dev/null 2>&1; then
                    update-alternatives --set iptables /usr/sbin/iptables-legacy
                    update-alternatives --set ip6tables /usr/sbin/ip6tables-legacy
                fi

                # Set up the necessary repositories
                if [ "${USE_MOBY}" = "true" ]; then
                    # Name of open source engine/cli
                    engine_package_name="moby-engine"
                    cli_package_name="moby-cli"

                    case ${ADJUSTED_ID} in
                        debian)
                            # Import key safely and import Microsoft apt repo
                            {
                                curl -sSL ${MICROSOFT_GPG_KEYS_URI}
                                curl -sSL ${MICROSOFT_GPG_KEYS_ROLLING_URI}
                            } | gpg --dearmor > /usr/share/keyrings/microsoft-archive-keyring.gpg
                            echo "deb [arch=${architecture} signed-by=/usr/share/keyrings/microsoft-archive-keyring.gpg] https://packages.microsoft.com/repos/microsoft-${ID}-${VERSION_CODENAME}-prod ${VERSION_CODENAME} main" > /etc/apt/sources.list.d/microsoft.list
                            ;;
                        rhel)
                            echo "(*) ${ID} detected - checking for Moby packages..."

                            # Check if moby packages are available in default repos
                            if ${PKG_MGR_CMD} list available moby-engine >/dev/null 2>&1; then
                                echo "(*) Using built-in ${ID} Moby packages"
                            else
                                case "${ID}" in
                                    azurelinux)
                                        echo "(*) Moby packages not found in Azure Linux repositories"
                                        echo "(*) For Azure Linux, Docker CE ('moby': false) is recommended"
                                        err "Moby packages are not available for Azure Linux ${VERSION_ID}."
                                        err "Recommendation: Use '\"moby\": false' to install Docker CE instead."
                                        exit 1
                                        ;;
                                    mariner)
                                        echo "(*) Adding Microsoft repository for CBL-Mariner..."
                                        # Add Microsoft repository if packages aren't available locally
                                        curl -sSL ${MICROSOFT_GPG_KEYS_URI} | gpg --dearmor > /etc/pki/rpm-gpg/microsoft.gpg
                                        cat > /etc/yum.repos.d/microsoft.repo << EOF
                [microsoft]
                name=Microsoft Repository
                baseurl=https://packages.microsoft.com/repos/microsoft-cbl-mariner-2.0-prod-base/
                enabled=1
                gpgcheck=1
                gpgkey=file:///etc/pki/rpm-gpg/microsoft.gpg
                EOF
                                # Verify packages are available after adding repo
                                pkg_mgr_update
                                if ! ${PKG_MGR_CMD} list available moby-engine >/dev/null 2>&1; then
                                    echo "(*) Moby packages not found in Microsoft repository either"
                                    err "Moby packages are not available for CBL-Mariner ${VERSION_ID}."
                                    err "Recommendation: Use '\"moby\": false' to install Docker CE instead."
                                    exit 1
                                fi
                                ;;
                            *)
                                err "Moby packages are not available for ${ID}. Please use 'moby': false option."
                                exit 1
                                ;;
                            esac
                        fi
                        ;;
                    esac
                else
                    # Name of licensed engine/cli
                    engine_package_name="docker-ce"
                    cli_package_name="docker-ce-cli"
                    case ${ADJUSTED_ID} in
                        debian)
                            curl -fsSL https://download.docker.com/linux/${ID}/gpg | gpg --dearmor > /usr/share/keyrings/docker-archive-keyring.gpg
                            echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/docker-archive-keyring.gpg] https://download.docker.com/linux/${ID} ${VERSION_CODENAME} stable" > /etc/apt/sources.list.d/docker.list
                            ;;
                        rhel)
                            # Docker CE repository setup for RHEL-based systems
                            setup_docker_ce_repo() {
                                curl -fsSL https://download.docker.com/linux/centos/gpg > /etc/pki/rpm-gpg/docker-ce.gpg
                                cat > /etc/yum.repos.d/docker-ce.repo << EOF
                [docker-ce-stable]
                name=Docker CE Stable
                baseurl=https://download.docker.com/linux/centos/9/\$basearch/stable
                enabled=1
                gpgcheck=1
                gpgkey=file:///etc/pki/rpm-gpg/docker-ce.gpg
                skip_if_unavailable=1
                module_hotfixes=1
                EOF
                            }
                            install_azure_linux_deps() {
                                echo "(*) Installing device-mapper libraries for Docker CE..."
                                [ "${ID}" != "mariner" ] && ${PKG_MGR_CMD} -y install device-mapper-libs 2>/dev/null || echo "(*) Device-mapper install failed, proceeding"
                                echo "(*) Installing additional Docker CE dependencies..."
                                ${PKG_MGR_CMD} -y install libseccomp libtool-ltdl systemd-libs libcgroup tar xz || {
                                    echo "(*) Some optional dependencies could not be installed, continuing..."
                                }
                            }
                            setup_selinux_context() {
                                if command -v getenforce >/dev/null 2>&1 && [ "$(getenforce 2>/dev/null)" != "Disabled" ]; then
                                    echo "(*) Creating minimal SELinux context for Docker compatibility..."
                                    mkdir -p /etc/selinux/targeted/contexts/files/ 2>/dev/null || true
                                    echo "/var/lib/docker(/.*)? system_u:object_r:container_file_t:s0" >> /etc/selinux/targeted/contexts/files/file_contexts.local 2>/dev/null || true
                                fi
                            }

                            # Special handling for RHEL Docker CE installation
                            case "${ID}" in
                                azurelinux|mariner)
                                    echo "(*) ${ID} detected"
                                    echo "(*) Note: Moby packages work better on Azure Linux. Consider using 'moby': true"
                                    echo "(*) Setting up Docker CE repository..."

                                    setup_docker_ce_repo
                                    install_azure_linux_deps

                                    if [ "${USE_MOBY}" != "true" ]; then
                                        echo "(*) Docker CE installation for Azure Linux - skipping container-selinux"
                                        echo "(*) Note: SELinux policies will be minimal but Docker will function normally"
                                        setup_selinux_context
                                    else
                                        echo "(*) Using Moby - container-selinux not required"
                                    fi
                                    ;;
                                *)
                                    # Standard RHEL/CentOS/Fedora approach
                                    if command -v dnf >/dev/null 2>&1; then
                                        dnf config-manager --add-repo https://download.docker.com/linux/centos/docker-ce.repo
                                    elif command -v yum-config-manager >/dev/null 2>&1; then
                                        yum-config-manager --add-repo https://download.docker.com/linux/centos/docker-ce.repo
                                    else
                                        # Manual fallback
                                        setup_docker_ce_repo
                            fi
                            ;;
                        esac
                        ;;
                    esac
                fi

                # Refresh package database
                case ${ADJUSTED_ID} in
                    debian)
                        apt-get update
                        ;;
                    rhel)
                        pkg_mgr_update
                        ;;
                esac

                # Soft version matching
                if [ "${DOCKER_VERSION}" = "latest" ] || [ "${DOCKER_VERSION}" = "lts" ] || [ "${DOCKER_VERSION}" = "stable" ]; then
                    # Empty, meaning grab whatever "latest" is in apt repo
                    engine_version_suffix=""
                    cli_version_suffix=""
                else
                    case ${ADJUSTED_ID} in
                        debian)
                    # Fetch a valid version from the apt-cache (eg: the Microsoft repo appends +azure, breakfix, etc...)
                    docker_version_dot_escaped="${DOCKER_VERSION//./\\.}"
                    docker_version_dot_plus_escaped="${docker_version_dot_escaped//+/\\+}"
                    # Regex needs to handle debian package version number format: https://www.systutorials.com/docs/linux/man/5-deb-version/
                    docker_version_regex="^(.+:)?${docker_version_dot_plus_escaped}([\\.\\+ ~:-]|$)"
                    set +e # Don't exit if finding version fails - will handle gracefully
                        cli_version_suffix="=$(apt-cache madison ${cli_package_name} | awk -F"|" '{print $2}' | sed -e 's/^[ \t]*//' | grep -E -m 1 "${docker_version_regex}")"
                        engine_version_suffix="=$(apt-cache madison ${engine_package_name} | awk -F"|" '{print $2}' | sed -e 's/^[ \t]*//' | grep -E -m 1 "${docker_version_regex}")"
                    set -e
                    if [ -z "${engine_version_suffix}" ] || [ "${engine_version_suffix}" = "=" ] || [ -z "${cli_version_suffix}" ] || [ "${cli_version_suffix}" = "=" ] ; then
                        err "No full or partial Docker / Moby version match found for \"${DOCKER_VERSION}\" on OS ${ID} ${VERSION_CODENAME} (${architecture}). Available versions:"
                        apt-cache madison ${cli_package_name} | awk -F"|" '{print $2}' | grep -oP '^(.+:)?\K.+'
                        exit 1
                    fi
                    ;;
                rhel)
                     # For RHEL-based systems, use dnf/yum to find versions
                            docker_version_escaped="${DOCKER_VERSION//./\\.}"
                            set +e # Don't exit if finding version fails - will handle gracefully
                                if [ "${USE_MOBY}" = "true" ]; then
                                    available_versions=$(${PKG_MGR_CMD} list --available moby-engine 2>/dev/null | grep -v "Available Packages" | awk '{print $2}' | grep -E "^${docker_version_escaped}" | head -1)
                                else
                                    available_versions=$(${PKG_MGR_CMD} list --available docker-ce 2>/dev/null | grep -v "Available Packages" | awk '{print $2}' | grep -E "^${docker_version_escaped}" | head -1)
                                fi
                            set -e
                            if [ -n "${available_versions}" ]; then
                                engine_version_suffix="-${available_versions}"
                                cli_version_suffix="-${available_versions}"
                            else
                                echo "(*) Exact version ${DOCKER_VERSION} not found, using latest available"
                                engine_version_suffix=""
                                cli_version_suffix=""
                            fi
                            ;;
                    esac
                fi

                # Version matching for moby-buildx
                if [ "${USE_MOBY}" = "true" ]; then
                    if [ "${MOBY_BUILDX_VERSION}" = "latest" ]; then
                        # Empty, meaning grab whatever "latest" is in apt repo
                        buildx_version_suffix=""
                    else
                        case ${ADJUSTED_ID} in
                            debian)
                        buildx_version_dot_escaped="${MOBY_BUILDX_VERSION//./\\.}"
                        buildx_version_dot_plus_escaped="${buildx_version_dot_escaped//+/\\+}"
                        buildx_version_regex="^(.+:)?${buildx_version_dot_plus_escaped}([\\.\\+ ~:-]|$)"
                        set +e
                            buildx_version_suffix="=$(apt-cache madison moby-buildx | awk -F"|" '{print $2}' | sed -e 's/^[ \t]*//' | grep -E -m 1 "${buildx_version_regex}")"
                        set -e
                        if [ -z "${buildx_version_suffix}" ] || [ "${buildx_version_suffix}" = "=" ]; then
                            err "No full or partial moby-buildx version match found for \"${MOBY_BUILDX_VERSION}\" on OS ${ID} ${VERSION_CODENAME} (${architecture}). Available versions:"
                            apt-cache madison moby-buildx | awk -F"|" '{print $2}' | grep -oP '^(.+:)?\K.+'
                            exit 1
                        fi
                        ;;
                            rhel)
                                # For RHEL-based systems, try to find buildx version or use latest
                                buildx_version_escaped="${MOBY_BUILDX_VERSION//./\\.}"
                                set +e
                                available_buildx=$(${PKG_MGR_CMD} list --available moby-buildx 2>/dev/null | grep -v "Available Packages" | awk '{print $2}' | grep -E "^${buildx_version_escaped}" | head -1)
                                set -e
                                if [ -n "${available_buildx}" ]; then
                                    buildx_version_suffix="-${available_buildx}"
                                else
                                    echo "(*) Exact buildx version ${MOBY_BUILDX_VERSION} not found, using latest available"
                                    buildx_version_suffix=""
                                fi
                                ;;
                        esac
                        echo "buildx_version_suffix ${buildx_version_suffix}"
                    fi
                fi

                # Install Docker / Moby CLI if not already installed
                if type docker > /dev/null 2>&1 && type dockerd > /dev/null 2>&1; then
                    echo "Docker / Moby CLI and Engine already installed."
                else
                        case ${ADJUSTED_ID} in
                        debian)
                            if [ "${USE_MOBY}" = "true" ]; then
                                # Install engine
                                set +e # Handle error gracefully
                                    apt-get -y install --no-install-recommends moby-cli${cli_version_suffix} moby-buildx${buildx_version_suffix} moby-engine${engine_version_suffix}
                                    exit_code=$?
                                set -e

                                if [ ${exit_code} -ne 0 ]; then
                                    err "Packages for moby not available in OS ${ID} ${VERSION_CODENAME} (${architecture}). To resolve, either: (1) set feature option '\"moby\": false' , or (2) choose a compatible OS version (eg: 'ubuntu-24.04')."
                                    exit 1
                                fi

                                # Install compose
                                apt-get -y install --no-install-recommends moby-compose || err "Package moby-compose (Docker Compose v2) not available for OS ${ID} ${VERSION_CODENAME} (${architecture}). Skipping."
                            else
                                apt-get -y install --no-install-recommends docker-ce-cli${cli_version_suffix} docker-ce${engine_version_suffix}
                                # Install compose
                                apt-mark hold docker-ce docker-ce-cli
                                apt-get -y install --no-install-recommends docker-compose-plugin || echo "(*) Package docker-compose-plugin (Docker Compose v2) not available for OS ${ID} ${VERSION_CODENAME} (${architecture}). Skipping."
                            fi
                            ;;
                        rhel)
                            if [ "${USE_MOBY}" = "true" ]; then
                                set +e # Handle error gracefully
                                    ${PKG_MGR_CMD} -y install moby-cli${cli_version_suffix} moby-engine${engine_version_suffix}
                                    exit_code=$?
                                set -e

                                if [ ${exit_code} -ne 0 ]; then
                                    err "Packages for moby not available in OS ${ID} ${VERSION_CODENAME} (${architecture}). To resolve, either: (1) set feature option '\"moby\": false' , or (2) choose a compatible OS version."
                                    exit 1
                                fi

                                # Install compose
                                if [ "${DOCKER_DASH_COMPOSE_VERSION}" != "none" ]; then
                                    ${PKG_MGR_CMD} -y install moby-compose || echo "(*) Package moby-compose not available for ${ID} ${VERSION_CODENAME} (${architecture}). Skipping."
                                fi
                            else
                                               # Special handling for Azure Linux Docker CE installation
                                if [ "${ID}" = "azurelinux" ] || [ "${ID}" = "mariner" ]; then
                                    echo "(*) Installing Docker CE on Azure Linux (bypassing container-selinux dependency)..."

                                    # Use rpm with --force and --nodeps for Azure Linux
                                    set +e  # Don't exit on error for this section
                                    ${PKG_MGR_CMD} -y install docker-ce${cli_version_suffix} docker-ce-cli${engine_version_suffix} containerd.io
                                    install_result=$?
                                    set -e

                                    if [ $install_result -ne 0 ]; then
                                        echo "(*) Standard installation failed, trying manual installation..."

                                        echo "(*) Standard installation failed, trying manual installation..."

                                        # Create directory for downloading packages
                                        mkdir -p /tmp/docker-ce-install

                                        # Download packages manually using curl since tdnf doesn't support download
                                        echo "(*) Downloading Docker CE packages manually..."

                                        # Get the repository baseurl
                                        repo_baseurl="https://download.docker.com/linux/centos/9/x86_64/stable"

                                        # Download packages directly
                                        cd /tmp/docker-ce-install

                                        # Get package names with versions
                                        if [ -n "${cli_version_suffix}" ]; then
                                            docker_ce_version="${cli_version_suffix#-}"
                                            docker_cli_version="${engine_version_suffix#-}"
                                        else
                                            # Get latest version from repository
                                            docker_ce_version="latest"
                                        fi

                                        echo "(*) Attempting to download Docker CE packages from repository..."

                                        # Try to download latest packages if specific version fails
                                        if ! curl -fsSL "${repo_baseurl}/Packages/docker-ce-${docker_ce_version}.el9.x86_64.rpm" -o docker-ce.rpm 2>/dev/null; then
                                            # Fallback: try to get latest available version
                                            echo "(*) Specific version not found, trying latest..."
                                            latest_docker=$(curl -s "${repo_baseurl}/Packages/" | grep -o 'docker-ce-[0-9][^"]*\.el9\.x86_64\.rpm' | head -1)
                                            latest_cli=$(curl -s "${repo_baseurl}/Packages/" | grep -o 'docker-ce-cli-[0-9][^"]*\.el9\.x86_64\.rpm' | head -1)
                                            latest_containerd=$(curl -s "${repo_baseurl}/Packages/" | grep -o 'containerd\.io-[0-9][^"]*\.el9\.x86_64\.rpm' | head -1)

                                            if [ -n "${latest_docker}" ]; then
                                                curl -fsSL "${repo_baseurl}/Packages/${latest_docker}" -o docker-ce.rpm
                                                curl -fsSL "${repo_baseurl}/Packages/${latest_cli}" -o docker-ce-cli.rpm
                                                curl -fsSL "${repo_baseurl}/Packages/${latest_containerd}" -o containerd.io.rpm
                                            else
                                                echo "(*) ERROR: Could not find Docker CE packages in repository"
                                                echo "(*) Please check repository configuration or use 'moby': true"
                                                exit 1
                                            fi
                                        fi
                                        # Install systemd libraries required by Docker CE
                                        echo "(*) Installing systemd libraries required by Docker CE..."
                                        ${PKG_MGR_CMD} -y install systemd-libs || ${PKG_MGR_CMD} -y install systemd-devel || {
                                            echo "(*) WARNING: Could not install systemd libraries"
                                            echo "(*) Docker may fail to start without these"
                                        }

                                        # Install with rpm --force --nodeps
                                        echo "(*) Installing Docker CE packages with dependency override..."
                                        rpm -Uvh --force --nodeps *.rpm

                                        # Cleanup
                                        cd /
                                        rm -rf /tmp/docker-ce-install

                                        echo "(*) Docker CE installation completed with dependency bypass"
                                        echo "(*) Note: Some SELinux functionality may be limited without container-selinux"
                                    fi
                                else
                                    # Standard installation for other RHEL-based systems
                                    ${PKG_MGR_CMD} -y install docker-ce${cli_version_suffix} docker-ce-cli${engine_version_suffix} containerd.io
                                fi
                                # Install compose
                                if [ "${DOCKER_DASH_COMPOSE_VERSION}" != "none" ]; then
                                    ${PKG_MGR_CMD} -y install docker-compose-plugin || echo "(*) Package docker-compose-plugin not available for ${ID} ${VERSION_CODENAME} (${architecture}). Skipping."
                                fi
                            fi
                            ;;
                    esac
                fi

                echo "Finished installing docker / moby!"

                docker_home="/usr/libexec/docker"
                cli_plugins_dir="${docker_home}/cli-plugins"

                # fallback for docker-compose
                fallback_compose(){
                    local url=$1
                    local repo_url=$(get_github_api_repo_url "$url")
                    echo -e "\n(!) Failed to fetch the latest artifacts for docker-compose v${compose_version}..."
                    get_previous_version "${url}" "${repo_url}" compose_version
                    echo -e "\nAttempting to install v${compose_version}"
                    curl -fsSL "https://github.com/docker/compose/releases/download/v${compose_version}/docker-compose-linux-${target_compose_arch}" -o ${docker_compose_path}
                }

                # If 'docker-compose' command is to be included
                if [ "${DOCKER_DASH_COMPOSE_VERSION}" != "none" ]; then
                    case "${architecture}" in
                    amd64|x86_64) target_compose_arch=x86_64 ;;
                    arm64|aarch64) target_compose_arch=aarch64 ;;
                    *)
                        echo "(!) Docker in docker does not support machine architecture '$architecture'. Please use an x86-64 or ARM64 machine."
                        exit 1
                    esac

                    docker_compose_path="/usr/local/bin/docker-compose"
                    if [ "${DOCKER_DASH_COMPOSE_VERSION}" = "v1" ]; then
                        err "The final Compose V1 release, version 1.29.2, was May 10, 2021. These packages haven't received any security updates since then. Use at your own risk."
                        INSTALL_DOCKER_COMPOSE_SWITCH="false"

                        if [ "${target_compose_arch}" = "x86_64" ]; then
                            echo "(*) Installing docker compose v1..."
                            curl -fsSL "https://github.com/docker/compose/releases/download/1.29.2/docker-compose-Linux-x86_64" -o ${docker_compose_path}
                            chmod +x ${docker_compose_path}

                            # Download the SHA256 checksum
                            DOCKER_COMPOSE_SHA256="$(curl -sSL "https://github.com/docker/compose/releases/download/1.29.2/docker-compose-Linux-x86_64.sha256" | awk '{print $1}')"
                            echo "${DOCKER_COMPOSE_SHA256}  ${docker_compose_path}" > docker-compose.sha256sum
                            sha256sum -c docker-compose.sha256sum --ignore-missing
                        elif [ "${VERSION_CODENAME}" = "bookworm" ]; then
                            err "Docker compose v1 is unavailable for 'bookworm' on Arm64. Kindly switch to use v2"
                            exit 1
                        else
                            # Use pip to get a version that runs on this architecture
                            check_packages python3-minimal python3-pip libffi-dev python3-venv
                            echo "(*) Installing docker compose v1 via pip..."
                            export PYTHONUSERBASE=/usr/local
                            pip3 install --disable-pip-version-check --no-cache-dir --user "Cython<3.0" pyyaml wheel docker-compose --no-build-isolation
                        fi
                    else
                        compose_version=${DOCKER_DASH_COMPOSE_VERSION#v}
                        docker_compose_url="https://github.com/docker/compose"
                        find_version_from_git_tags compose_version "$docker_compose_url" "tags/v"
                        echo "(*) Installing docker-compose ${compose_version}..."
                        curl -fsSL "https://github.com/docker/compose/releases/download/v${compose_version}/docker-compose-linux-${target_compose_arch}" -o ${docker_compose_path} || {
                                 echo -e "\n(!) Failed to fetch the latest artifacts for docker-compose v${compose_version}..."
                                 fallback_compose "$docker_compose_url"
                        }

                        chmod +x ${docker_compose_path}

                        # Download the SHA256 checksum
                        DOCKER_COMPOSE_SHA256="$(curl -sSL "https://github.com/docker/compose/releases/download/v${compose_version}/docker-compose-linux-${target_compose_arch}.sha256" | awk '{print $1}')"
                        echo "${DOCKER_COMPOSE_SHA256}  ${docker_compose_path}" > docker-compose.sha256sum
                        sha256sum -c docker-compose.sha256sum --ignore-missing

                        mkdir -p ${cli_plugins_dir}
                        cp ${docker_compose_path} ${cli_plugins_dir}
                    fi
                fi

                # fallback method for compose-switch
                fallback_compose-switch() {
                    local url=$1
                    local repo_url=$(get_github_api_repo_url "$url")
                    echo -e "\n(!) Failed to fetch the latest artifacts for compose-switch v${compose_switch_version}..."
                    get_previous_version "$url" "$repo_url" compose_switch_version
                    echo -e "\nAttempting to install v${compose_switch_version}"
                    curl -fsSL "https://github.com/docker/compose-switch/releases/download/v${compose_switch_version}/docker-compose-linux-${target_switch_arch}" -o /usr/local/bin/compose-switch
                }
                # Install docker-compose switch if not already installed - https://github.com/docker/compose-switch#manual-installation
                if [ "${INSTALL_DOCKER_COMPOSE_SWITCH}" = "true" ] && ! type compose-switch > /dev/null 2>&1; then
                    if type docker-compose > /dev/null 2>&1; then
                        echo "(*) Installing compose-switch..."
                        current_compose_path="$(command -v docker-compose)"
                        target_compose_path="$(dirname "${current_compose_path}")/docker-compose-v1"
                        compose_switch_version="latest"
                        compose_switch_url="https://github.com/docker/compose-switch"
                        # Try to get latest version, fallback to known stable version if GitHub API fails
                        set +e
                        find_version_from_git_tags compose_switch_version "$compose_switch_url"
                        if [ $? -ne 0 ] || [ -z "${compose_switch_version}" ] || [ "${compose_switch_version}" = "latest" ]; then
                            echo "(*) GitHub API rate limited or failed, using fallback method"
                            fallback_compose-switch "$compose_switch_url"
                        fi
                        set -e

                        # Map architecture for compose-switch downloads
                        case "${architecture}" in
                            amd64|x86_64) target_switch_arch=amd64 ;;
                            arm64|aarch64) target_switch_arch=arm64 ;;
                            *) target_switch_arch=${architecture} ;;
                        esac
                        curl -fsSL "https://github.com/docker/compose-switch/releases/download/v${compose_switch_version}/docker-compose-linux-${target_switch_arch}" -o /usr/local/bin/compose-switch || fallback_compose-switch "$compose_switch_url"
                        chmod +x /usr/local/bin/compose-switch
                        # TODO: Verify checksum once available: https://github.com/docker/compose-switch/issues/11
                        # Setup v1 CLI as alternative in addition to compose-switch (which maps to v2)
                        mv "${current_compose_path}" "${target_compose_path}"
                        update-alternatives --install ${docker_compose_path} docker-compose /usr/local/bin/compose-switch 99
                        update-alternatives --install ${docker_compose_path} docker-compose "${target_compose_path}" 1
                    else
                        err "Skipping installation of compose-switch as docker compose is unavailable..."
                    fi
                fi

                # If init file already exists, exit
                if [ -f "/usr/local/share/docker-init.sh" ]; then
                    echo "/usr/local/share/docker-init.sh already exists, so exiting."
                    # Clean up
                    rm -rf /var/lib/apt/lists/*
                    exit 0
                fi
                echo "docker-init doesn't exist, adding..."

                if ! cat /etc/group | grep -e "^docker:" > /dev/null 2>&1; then
                        groupadd -r docker
                fi

                usermod -aG docker ${USERNAME}

                # fallback for docker/buildx
                fallback_buildx() {
                    local url=$1
                    local repo_url=$(get_github_api_repo_url "$url")
                    echo -e "\n(!) Failed to fetch the latest artifacts for docker buildx v${buildx_version}..."
                    get_previous_version "$url" "$repo_url" buildx_version
                    buildx_file_name="buildx-v${buildx_version}.linux-${target_buildx_arch}"
                    echo -e "\nAttempting to install v${buildx_version}"
                    wget https://github.com/docker/buildx/releases/download/v${buildx_version}/${buildx_file_name}
                }

                if [ "${INSTALL_DOCKER_BUILDX}" = "true" ]; then
                    buildx_version="latest"
                    docker_buildx_url="https://github.com/docker/buildx"
                    find_version_from_git_tags buildx_version "$docker_buildx_url" "refs/tags/v"
                    echo "(*) Installing buildx ${buildx_version}..."

                      # Map architecture for buildx downloads
                    case "${architecture}" in
                        amd64|x86_64) target_buildx_arch=amd64 ;;
                        arm64|aarch64) target_buildx_arch=arm64 ;;
                        *) target_buildx_arch=${architecture} ;;
                    esac

                    buildx_file_name="buildx-v${buildx_version}.linux-${target_buildx_arch}"

                    cd /tmp
                    wget https://github.com/docker/buildx/releases/download/v${buildx_version}/${buildx_file_name} || fallback_buildx "$docker_buildx_url"

                    docker_home="/usr/libexec/docker"
                    cli_plugins_dir="${docker_home}/cli-plugins"

                    mkdir -p ${cli_plugins_dir}
                    mv ${buildx_file_name} ${cli_plugins_dir}/docker-buildx
                    chmod +x ${cli_plugins_dir}/docker-buildx

                    chown -R "${USERNAME}:docker" "${docker_home}"
                    chmod -R g+r+w "${docker_home}"
                    find "${docker_home}" -type d -print0 | xargs -n 1 -0 chmod g+s
                fi

                DOCKER_DEFAULT_IP6_TABLES=""
                if [ "$DISABLE_IP6_TABLES" == true ]; then
                    requested_version=""
                    # checking whether the version requested either is in semver format or just a number denoting the major version
                    # and, extracting the major version number out of the two scenarios
                    semver_regex="^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-([0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*))?(\+([0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*))?$"
                    if echo "$DOCKER_VERSION" | grep -Eq $semver_regex; then
                        requested_version=$(echo $DOCKER_VERSION | cut -d. -f1)
                    elif echo "$DOCKER_VERSION" | grep -Eq "^[1-9][0-9]*$"; then
                        requested_version=$DOCKER_VERSION
                    fi
                    if [ "$DOCKER_VERSION" = "latest" ] || [[ -n "$requested_version" && "$requested_version" -ge 27 ]] ; then
                        DOCKER_DEFAULT_IP6_TABLES="--ip6tables=false"
                        echo "(!) As requested, passing '${DOCKER_DEFAULT_IP6_TABLES}'"
                    fi
                fi

                if [ ! -d /usr/local/share ]; then
                    mkdir -p /usr/local/share
                fi

                tee /usr/local/share/docker-init.sh > /dev/null \
                << EOF
                #!/bin/sh
                #-------------------------------------------------------------------------------------------------------------
                # Copyright (c) Microsoft Corporation. All rights reserved.
                # Licensed under the MIT License. See https://go.microsoft.com/fwlink/?linkid=2090316 for license information.
                #-------------------------------------------------------------------------------------------------------------

                set -e

                AZURE_DNS_AUTO_DETECTION=${AZURE_DNS_AUTO_DETECTION}
                DOCKER_DEFAULT_ADDRESS_POOL=${DOCKER_DEFAULT_ADDRESS_POOL}
                DOCKER_DEFAULT_IP6_TABLES=${DOCKER_DEFAULT_IP6_TABLES}
                EOF

                tee -a /usr/local/share/docker-init.sh > /dev/null \
                << 'EOF'
                dockerd_start="AZURE_DNS_AUTO_DETECTION=${AZURE_DNS_AUTO_DETECTION} DOCKER_DEFAULT_ADDRESS_POOL=${DOCKER_DEFAULT_ADDRESS_POOL} DOCKER_DEFAULT_IP6_TABLES=${DOCKER_DEFAULT_IP6_TABLES} $(cat << 'INNEREOF'
                    # explicitly remove dockerd and containerd PID file to ensure that it can start properly if it was stopped uncleanly
                    find /run /var/run -iname 'docker*.pid' -delete || :
                    find /run /var/run -iname 'container*.pid' -delete || :

                    # -- Start: dind wrapper script --
                    # Maintained: https://github.com/moby/moby/blob/master/hack/dind

                    export container=docker

                    if [ -d /sys/kernel/security ] && ! mountpoint -q /sys/kernel/security; then
                        mount -t securityfs none /sys/kernel/security || {
                            echo >&2 'Could not mount /sys/kernel/security.'
                            echo >&2 'AppArmor detection and --privileged mode might break.'
                        }
                    fi

                    # Mount /tmp (conditionally)
                    if ! mountpoint -q /tmp; then
                        mount -t tmpfs none /tmp
                    fi

                    set_cgroup_nesting()
                    {
                        # cgroup v2: enable nesting
                        if [ -f /sys/fs/cgroup/cgroup.controllers ]; then
                            # move the processes from the root group to the /init group,
                            # otherwise writing subtree_control fails with EBUSY.
                            # An error during moving non-existent process (i.e., "cat") is ignored.
                            mkdir -p /sys/fs/cgroup/init
                            xargs -rn1 < /sys/fs/cgroup/cgroup.procs > /sys/fs/cgroup/init/cgroup.procs || :
                            # enable controllers
                            sed -e 's/ / +/g' -e 's/^/+/' < /sys/fs/cgroup/cgroup.controllers \
                                > /sys/fs/cgroup/cgroup.subtree_control
                        fi
                    }

                    # Set cgroup nesting, retrying if necessary
                    retry_cgroup_nesting=0

                    until [ "${retry_cgroup_nesting}" -eq "5" ];
                    do
                        set +e
                            set_cgroup_nesting

                            if [ $? -ne 0 ]; then
                                echo "(*) cgroup v2: Failed to enable nesting, retrying..."
                            else
                                break
                            fi

                            retry_cgroup_nesting=`expr $retry_cgroup_nesting + 1`
                        set -e
                    done

                    # -- End: dind wrapper script --

                    # Handle DNS
                    set +e
                        cat /etc/resolv.conf | grep -i 'internal.cloudapp.net' > /dev/null 2>&1
                        if [ $? -eq 0 ] && [ "${AZURE_DNS_AUTO_DETECTION}" = "true" ]
                        then
                            echo "Setting dockerd Azure DNS."
                            CUSTOMDNS="--dns 168.63.129.16"
                        else
                            echo "Not setting dockerd DNS manually."
                            CUSTOMDNS=""
                        fi
                    set -e

                    if [ -z "$DOCKER_DEFAULT_ADDRESS_POOL" ]
                    then
                        DEFAULT_ADDRESS_POOL=""
                    else
                        DEFAULT_ADDRESS_POOL="--default-address-pool $DOCKER_DEFAULT_ADDRESS_POOL"
                    fi

                    # Start docker/moby engine
                    ( dockerd $CUSTOMDNS $DEFAULT_ADDRESS_POOL $DOCKER_DEFAULT_IP6_TABLES > /tmp/dockerd.log 2>&1 ) &
                INNEREOF
                )"

                sudo_if() {
                    COMMAND="$*"

                    if [ "$(id -u)" -ne 0 ]; then
                        sudo $COMMAND
                    else
                        $COMMAND
                    fi
                }

                retry_docker_start_count=0
                docker_ok="false"

                until [ "${docker_ok}" = "true"  ] || [ "${retry_docker_start_count}" -eq "5" ];
                do
                    # Start using sudo if not invoked as root
                    if [ "$(id -u)" -ne 0 ]; then
                        sudo /bin/sh -c "${dockerd_start}"
                    else
                        eval "${dockerd_start}"
                    fi

                    retry_count=0
                    until [ "${docker_ok}" = "true"  ] || [ "${retry_count}" -eq "5" ];
                    do
                        sleep 1s
                        set +e
                            docker info > /dev/null 2>&1 && docker_ok="true"
                        set -e

                        retry_count=`expr $retry_count + 1`
                    done

                    if [ "${docker_ok}" != "true" ] && [ "${retry_docker_start_count}" != "4" ]; then
                        echo "(*) Failed to start docker, retrying..."
                        set +e
                            sudo_if pkill dockerd
                            sudo_if pkill containerd
                        set -e
                    fi

                    retry_docker_start_count=`expr $retry_docker_start_count + 1`
                done

                # Execute whatever commands were passed in (if any). This allows us
                # to set this script to ENTRYPOINT while still executing the default CMD.
                exec "$@"
                EOF

                chmod +x /usr/local/share/docker-init.sh
                chown ${USERNAME}:root /usr/local/share/docker-init.sh

                # Clean up
                rm -rf /var/lib/apt/lists/*

                echo 'docker-in-docker-debian script has completed!'"#),
            ]).await;

            return Ok(http::Response::builder()
                .status(200)
                .body(AsyncBody::from(response))
                .unwrap());
        }
        if parts.uri.path() == "/v2/devcontainers/features/go/manifests/1" {
            let response = r#"
                {
                    "schemaVersion": 2,
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "config": {
                        "mediaType": "application/vnd.devcontainers",
                        "digest": "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
                        "size": 2
                    },
                    "layers": [
                        {
                            "mediaType": "application/vnd.devcontainers.layer.v1+tar",
                            "digest": "sha256:eadd8a4757ee8ea6c1bc0aae22da49b7e5f2f1e32a87a5eac3cadeb7d2ccdad1",
                            "size": 20992,
                            "annotations": {
                                "org.opencontainers.image.title": "devcontainer-feature-go.tgz"
                            }
                        }
                    ],
                    "annotations": {
                        "dev.containers.metadata": "{\"id\":\"go\",\"version\":\"1.3.3\",\"name\":\"Go\",\"documentationURL\":\"https://github.com/devcontainers/features/tree/main/src/go\",\"description\":\"Installs Go and common Go utilities. Auto-detects latest version and installs needed dependencies.\",\"options\":{\"version\":{\"type\":\"string\",\"proposals\":[\"latest\",\"none\",\"1.24\",\"1.23\"],\"default\":\"latest\",\"description\":\"Select or enter a Go version to install\"},\"golangciLintVersion\":{\"type\":\"string\",\"default\":\"latest\",\"description\":\"Version of golangci-lint to install\"}},\"init\":true,\"customizations\":{\"vscode\":{\"extensions\":[\"golang.Go\"],\"settings\":{\"github.copilot.chat.codeGeneration.instructions\":[{\"text\":\"This dev container includes Go and common Go utilities pre-installed and available on the `PATH`, along with the Go language extension for Go development.\"}]}}},\"containerEnv\":{\"GOROOT\":\"/usr/local/go\",\"GOPATH\":\"/go\",\"PATH\":\"/usr/local/go/bin:/go/bin:${PATH}\"},\"capAdd\":[\"SYS_PTRACE\"],\"securityOpt\":[\"seccomp=unconfined\"],\"installsAfter\":[\"ghcr.io/devcontainers/features/common-utils\"]}",
                        "com.github.package.type": "devcontainer_feature"
                    }
                }
                "#;

            return Ok(http::Response::builder()
                .status(200)
                .body(http_client::AsyncBody::from(response))
                .unwrap());
        }
        if parts.uri.path()
            == "/v2/devcontainers/features/go/blobs/sha256:eadd8a4757ee8ea6c1bc0aae22da49b7e5f2f1e32a87a5eac3cadeb7d2ccdad1"
        {
            let response = build_tarball(vec![
                ("./devcontainer-feature.json", r#"
                    {
                        "id": "go",
                        "version": "1.3.3",
                        "name": "Go",
                        "documentationURL": "https://github.com/devcontainers/features/tree/main/src/go",
                        "description": "Installs Go and common Go utilities. Auto-detects latest version and installs needed dependencies.",
                        "options": {
                            "version": {
                                "type": "string",
                                "proposals": [
                                    "latest",
                                    "none",
                                    "1.24",
                                    "1.23"
                                ],
                                "default": "latest",
                                "description": "Select or enter a Go version to install"
                            },
                            "golangciLintVersion": {
                                "type": "string",
                                "default": "latest",
                                "description": "Version of golangci-lint to install"
                            }
                        },
                        "init": true,
                        "customizations": {
                            "vscode": {
                                "extensions": [
                                    "golang.Go"
                                ],
                                "settings": {
                                    "github.copilot.chat.codeGeneration.instructions": [
                                        {
                                            "text": "This dev container includes Go and common Go utilities pre-installed and available on the `PATH`, along with the Go language extension for Go development."
                                        }
                                    ]
                                }
                            }
                        },
                        "containerEnv": {
                            "GOROOT": "/usr/local/go",
                            "GOPATH": "/go",
                            "PATH": "/usr/local/go/bin:/go/bin:${PATH}"
                        },
                        "capAdd": [
                            "SYS_PTRACE"
                        ],
                        "securityOpt": [
                            "seccomp=unconfined"
                        ],
                        "installsAfter": [
                            "ghcr.io/devcontainers/features/common-utils"
                        ]
                    }
                    "#),
                ("./install.sh", r#"
                #!/usr/bin/env bash
                #-------------------------------------------------------------------------------------------------------------
                # Copyright (c) Microsoft Corporation. All rights reserved.
                # Licensed under the MIT License. See https://go.microsoft.com/fwlink/?linkid=2090316 for license information
                #-------------------------------------------------------------------------------------------------------------
                #
                # Docs: https://github.com/microsoft/vscode-dev-containers/blob/main/script-library/docs/go.md
                # Maintainer: The VS Code and Codespaces Teams

                TARGET_GO_VERSION="${VERSION:-"latest"}"
                GOLANGCILINT_VERSION="${GOLANGCILINTVERSION:-"latest"}"

                TARGET_GOROOT="${TARGET_GOROOT:-"/usr/local/go"}"
                TARGET_GOPATH="${TARGET_GOPATH:-"/go"}"
                USERNAME="${USERNAME:-"${_REMOTE_USER:-"automatic"}"}"
                INSTALL_GO_TOOLS="${INSTALL_GO_TOOLS:-"true"}"

                # https://www.google.com/linuxrepositories/
                GO_GPG_KEY_URI="https://dl.google.com/linux/linux_signing_key.pub"

                set -e

                if [ "$(id -u)" -ne 0 ]; then
                    echo -e 'Script must be run as root. Use sudo, su, or add "USER root" to your Dockerfile before running this script.'
                    exit 1
                fi

                # Bring in ID, ID_LIKE, VERSION_ID, VERSION_CODENAME
                . /etc/os-release
                # Get an adjusted ID independent of distro variants
                MAJOR_VERSION_ID=$(echo ${VERSION_ID} | cut -d . -f 1)
                if [ "${ID}" = "debian" ] || [ "${ID_LIKE}" = "debian" ]; then
                    ADJUSTED_ID="debian"
                elif [[ "${ID}" = "rhel" || "${ID}" = "fedora" || "${ID}" = "mariner" || "${ID_LIKE}" = *"rhel"* || "${ID_LIKE}" = *"fedora"* || "${ID_LIKE}" = *"mariner"* ]]; then
                    ADJUSTED_ID="rhel"
                    if [[ "${ID}" = "rhel" ]] || [[ "${ID}" = *"alma"* ]] || [[ "${ID}" = *"rocky"* ]]; then
                        VERSION_CODENAME="rhel${MAJOR_VERSION_ID}"
                    else
                        VERSION_CODENAME="${ID}${MAJOR_VERSION_ID}"
                    fi
                else
                    echo "Linux distro ${ID} not supported."
                    exit 1
                fi

                if [ "${ADJUSTED_ID}" = "rhel" ] && [ "${VERSION_CODENAME-}" = "centos7" ]; then
                    # As of 1 July 2024, mirrorlist.centos.org no longer exists.
                    # Update the repo files to reference vault.centos.org.
                    sed -i s/mirror.centos.org/vault.centos.org/g /etc/yum.repos.d/*.repo
                    sed -i s/^#.*baseurl=http/baseurl=http/g /etc/yum.repos.d/*.repo
                    sed -i s/^mirrorlist=http/#mirrorlist=http/g /etc/yum.repos.d/*.repo
                fi

                # Setup INSTALL_CMD & PKG_MGR_CMD
                if type apt-get > /dev/null 2>&1; then
                    PKG_MGR_CMD=apt-get
                    INSTALL_CMD="${PKG_MGR_CMD} -y install --no-install-recommends"
                elif type microdnf > /dev/null 2>&1; then
                    PKG_MGR_CMD=microdnf
                    INSTALL_CMD="${PKG_MGR_CMD} ${INSTALL_CMD_ADDL_REPOS} -y install --refresh --best --nodocs --noplugins --setopt=install_weak_deps=0"
                elif type dnf > /dev/null 2>&1; then
                    PKG_MGR_CMD=dnf
                    INSTALL_CMD="${PKG_MGR_CMD} ${INSTALL_CMD_ADDL_REPOS} -y install --refresh --best --nodocs --noplugins --setopt=install_weak_deps=0"
                else
                    PKG_MGR_CMD=yum
                    INSTALL_CMD="${PKG_MGR_CMD} ${INSTALL_CMD_ADDL_REPOS} -y install --noplugins --setopt=install_weak_deps=0"
                fi

                # Clean up
                clean_up() {
                    case ${ADJUSTED_ID} in
                        debian)
                            rm -rf /var/lib/apt/lists/*
                            ;;
                        rhel)
                            rm -rf /var/cache/dnf/* /var/cache/yum/*
                            rm -rf /tmp/yum.log
                            rm -rf ${GPG_INSTALL_PATH}
                            ;;
                    esac
                }
                clean_up


                # Figure out correct version of a three part version number is not passed
                find_version_from_git_tags() {
                    local variable_name=$1
                    local requested_version=${!variable_name}
                    if [ "${requested_version}" = "none" ]; then return; fi
                    local repository=$2
                    local prefix=${3:-"tags/v"}
                    local separator=${4:-"."}
                    local last_part_optional=${5:-"false"}
                    if [ "$(echo "${requested_version}" | grep -o "." | wc -l)" != "2" ]; then
                        local escaped_separator=${separator//./\\.}
                        local last_part
                        if [ "${last_part_optional}" = "true" ]; then
                            last_part="(${escaped_separator}[0-9]+)?"
                        else
                            last_part="${escaped_separator}[0-9]+"
                        fi
                        local regex="${prefix}\\K[0-9]+${escaped_separator}[0-9]+${last_part}$"
                        local version_list="$(git ls-remote --tags ${repository} | grep -oP "${regex}" | tr -d ' ' | tr "${separator}" "." | sort -rV)"
                        if [ "${requested_version}" = "latest" ] || [ "${requested_version}" = "current" ] || [ "${requested_version}" = "lts" ]; then
                            declare -g ${variable_name}="$(echo "${version_list}" | head -n 1)"
                        else
                            set +e
                            declare -g ${variable_name}="$(echo "${version_list}" | grep -E -m 1 "^${requested_version//./\\.}([\\.\\s]|$)")"
                            set -e
                        fi
                    fi
                    if [ -z "${!variable_name}" ] || ! echo "${version_list}" | grep "^${!variable_name//./\\.}$" > /dev/null 2>&1; then
                        echo -e "Invalid ${variable_name} value: ${requested_version}\nValid values:\n${version_list}" >&2
                        exit 1
                    fi
                    echo "${variable_name}=${!variable_name}"
                }

                pkg_mgr_update() {
                    case $ADJUSTED_ID in
                        debian)
                            if [ "$(find /var/lib/apt/lists/* | wc -l)" = "0" ]; then
                                echo "Running apt-get update..."
                                ${PKG_MGR_CMD} update -y
                            fi
                            ;;
                        rhel)
                            if [ ${PKG_MGR_CMD} = "microdnf" ]; then
                                if [ "$(ls /var/cache/yum/* 2>/dev/null | wc -l)" = 0 ]; then
                                    echo "Running ${PKG_MGR_CMD} makecache ..."
                                    ${PKG_MGR_CMD} makecache
                                fi
                            else
                                if [ "$(ls /var/cache/${PKG_MGR_CMD}/* 2>/dev/null | wc -l)" = 0 ]; then
                                    echo "Running ${PKG_MGR_CMD} check-update ..."
                                    set +e
                                    ${PKG_MGR_CMD} check-update
                                    rc=$?
                                    if [ $rc != 0 ] && [ $rc != 100 ]; then
                                        exit 1
                                    fi
                                    set -e
                                fi
                            fi
                            ;;
                    esac
                }

                # Checks if packages are installed and installs them if not
                check_packages() {
                    case ${ADJUSTED_ID} in
                        debian)
                            if ! dpkg -s "$@" > /dev/null 2>&1; then
                                pkg_mgr_update
                                ${INSTALL_CMD} "$@"
                            fi
                            ;;
                        rhel)
                            if ! rpm -q "$@" > /dev/null 2>&1; then
                                pkg_mgr_update
                                ${INSTALL_CMD} "$@"
                            fi
                            ;;
                    esac
                }

                # Ensure that login shells get the correct path if the user updated the PATH using ENV.
                rm -f /etc/profile.d/00-restore-env.sh
                echo "export PATH=${PATH//$(sh -lc 'echo $PATH')/\$PATH}" > /etc/profile.d/00-restore-env.sh
                chmod +x /etc/profile.d/00-restore-env.sh

                # Some distributions do not install awk by default (e.g. Mariner)
                if ! type awk >/dev/null 2>&1; then
                    check_packages awk
                fi

                # Determine the appropriate non-root user
                if [ "${USERNAME}" = "auto" ] || [ "${USERNAME}" = "automatic" ]; then
                    USERNAME=""
                    POSSIBLE_USERS=("vscode" "node" "codespace" "$(awk -v val=1000 -F ":" '$3==val{print $1}' /etc/passwd)")
                    for CURRENT_USER in "${POSSIBLE_USERS[@]}"; do
                        if id -u ${CURRENT_USER} > /dev/null 2>&1; then
                            USERNAME=${CURRENT_USER}
                            break
                        fi
                    done
                    if [ "${USERNAME}" = "" ]; then
                        USERNAME=root
                    fi
                elif [ "${USERNAME}" = "none" ] || ! id -u ${USERNAME} > /dev/null 2>&1; then
                    USERNAME=root
                fi

                export DEBIAN_FRONTEND=noninteractive

                check_packages ca-certificates gnupg2 tar gcc make pkg-config

                if [ $ADJUSTED_ID = "debian" ]; then
                    check_packages g++ libc6-dev
                else
                    check_packages gcc-c++ glibc-devel
                fi
                # Install curl, git, other dependencies if missing
                if ! type curl > /dev/null 2>&1; then
                    check_packages curl
                fi
                if ! type git > /dev/null 2>&1; then
                    check_packages git
                fi
                # Some systems, e.g. Mariner, still a few more packages
                if ! type as > /dev/null 2>&1; then
                    check_packages binutils
                fi
                if ! [ -f /usr/include/linux/errno.h ]; then
                    check_packages kernel-headers
                fi
                # Minimal RHEL install may need findutils installed
                if ! [ -f /usr/bin/find ]; then
                    check_packages findutils
                fi

                # Get closest match for version number specified
                find_version_from_git_tags TARGET_GO_VERSION "https://go.googlesource.com/go" "tags/go" "." "true"

                architecture="$(uname -m)"
                case $architecture in
                    x86_64) architecture="amd64";;
                    aarch64 | armv8*) architecture="arm64";;
                    aarch32 | armv7* | armvhf*) architecture="armv6l";;
                    i?86) architecture="386";;
                    *) echo "(!) Architecture $architecture unsupported"; exit 1 ;;
                esac

                # Install Go
                umask 0002
                if ! cat /etc/group | grep -e "^golang:" > /dev/null 2>&1; then
                    groupadd -r golang
                fi
                usermod -a -G golang "${USERNAME}"
                mkdir -p "${TARGET_GOROOT}" "${TARGET_GOPATH}"

                if [[ "${TARGET_GO_VERSION}" != "none" ]] && [[ "$(go version 2>/dev/null)" != *"${TARGET_GO_VERSION}"* ]]; then
                    # Use a temporary location for gpg keys to avoid polluting image
                    export GNUPGHOME="/tmp/tmp-gnupg"
                    mkdir -p ${GNUPGHOME}
                    chmod 700 ${GNUPGHOME}
                    curl -sSL -o /tmp/tmp-gnupg/golang_key "${GO_GPG_KEY_URI}"
                    gpg -q --import /tmp/tmp-gnupg/golang_key
                    echo "Downloading Go ${TARGET_GO_VERSION}..."
                    set +e
                    curl -fsSL -o /tmp/go.tar.gz "https://golang.org/dl/go${TARGET_GO_VERSION}.linux-${architecture}.tar.gz"
                    exit_code=$?
                    set -e
                    if [ "$exit_code" != "0" ]; then
                        echo "(!) Download failed."
                        # Try one break fix version number less if we get a failure. Use "set +e" since "set -e" can cause failures in valid scenarios.
                        set +e
                        major="$(echo "${TARGET_GO_VERSION}" | grep -oE '^[0-9]+' || echo '')"
                        minor="$(echo "${TARGET_GO_VERSION}" | grep -oP '^[0-9]+\.\K[0-9]+' || echo '')"
                        breakfix="$(echo "${TARGET_GO_VERSION}" | grep -oP '^[0-9]+\.[0-9]+\.\K[0-9]+' 2>/dev/null || echo '')"
                        # Handle Go's odd version pattern where "0" releases omit the last part
                        if [ "${breakfix}" = "" ] || [ "${breakfix}" = "0" ]; then
                            ((minor=minor-1))
                            TARGET_GO_VERSION="${major}.${minor}"
                            # Look for latest version from previous minor release
                            find_version_from_git_tags TARGET_GO_VERSION "https://go.googlesource.com/go" "tags/go" "." "true"
                        else
                            ((breakfix=breakfix-1))
                            if [ "${breakfix}" = "0" ]; then
                                TARGET_GO_VERSION="${major}.${minor}"
                            else
                                TARGET_GO_VERSION="${major}.${minor}.${breakfix}"
                            fi
                        fi
                        set -e
                        echo "Trying ${TARGET_GO_VERSION}..."
                        curl -fsSL -o /tmp/go.tar.gz "https://golang.org/dl/go${TARGET_GO_VERSION}.linux-${architecture}.tar.gz"
                    fi
                    curl -fsSL -o /tmp/go.tar.gz.asc "https://golang.org/dl/go${TARGET_GO_VERSION}.linux-${architecture}.tar.gz.asc"
                    gpg --verify /tmp/go.tar.gz.asc /tmp/go.tar.gz
                    echo "Extracting Go ${TARGET_GO_VERSION}..."
                    tar -xzf /tmp/go.tar.gz -C "${TARGET_GOROOT}" --strip-components=1
                    rm -rf /tmp/go.tar.gz /tmp/go.tar.gz.asc /tmp/tmp-gnupg
                else
                    echo "(!) Go is already installed with version ${TARGET_GO_VERSION}. Skipping."
                fi

                # Install Go tools that are isImportant && !replacedByGopls based on
                # https://github.com/golang/vscode-go/blob/v0.38.0/src/goToolsInformation.ts
                GO_TOOLS="\
                    golang.org/x/tools/gopls@latest \
                    honnef.co/go/tools/cmd/staticcheck@latest \
                    golang.org/x/lint/golint@latest \
                    github.com/mgechev/revive@latest \
                    github.com/go-delve/delve/cmd/dlv@latest \
                    github.com/fatih/gomodifytags@latest \
                    github.com/haya14busa/goplay/cmd/goplay@latest \
                    github.com/cweill/gotests/gotests@latest \
                    github.com/josharian/impl@latest"

                if [ "${INSTALL_GO_TOOLS}" = "true" ]; then
                    echo "Installing common Go tools..."
                    export PATH=${TARGET_GOROOT}/bin:${PATH}
                    export GOPATH=/tmp/gotools
                    export GOCACHE="${GOPATH}/cache"

                    mkdir -p "${GOPATH}" /usr/local/etc/vscode-dev-containers "${TARGET_GOPATH}/bin"
                    cd "${GOPATH}"

                    # Use go get for versions of go under 1.16
                    go_install_command=install
                    if [[ "1.16" > "$(go version | grep -oP 'go\K[0-9]+\.[0-9]+(\.[0-9]+)?')" ]]; then
                        export GO111MODULE=on
                        go_install_command=get
                        echo "Go version < 1.16, using go get."
                    fi

                    (echo "${GO_TOOLS}" | xargs -n 1 go ${go_install_command} -v )2>&1 | tee -a /usr/local/etc/vscode-dev-containers/go.log

                    # Move Go tools into path
                    if [ -d "${GOPATH}/bin" ]; then
                        mv "${GOPATH}/bin"/* "${TARGET_GOPATH}/bin/"
                    fi

                    # Install golangci-lint from precompiled binaries
                    if [ "$GOLANGCILINT_VERSION" = "latest" ] || [ "$GOLANGCILINT_VERSION" = "" ]; then
                        echo "Installing golangci-lint latest..."
                        curl -fsSL https://raw.githubusercontent.com/golangci/golangci-lint/master/install.sh | \
                            sh -s -- -b "${TARGET_GOPATH}/bin"
                    else
                        echo "Installing golangci-lint ${GOLANGCILINT_VERSION}..."
                        curl -fsSL https://raw.githubusercontent.com/golangci/golangci-lint/master/install.sh | \
                            sh -s -- -b "${TARGET_GOPATH}/bin" "v${GOLANGCILINT_VERSION}"
                    fi

                    # Remove Go tools temp directory
                    rm -rf "${GOPATH}"
                fi


                chown -R "${USERNAME}:golang" "${TARGET_GOROOT}" "${TARGET_GOPATH}"
                chmod -R g+r+w "${TARGET_GOROOT}" "${TARGET_GOPATH}"
                find "${TARGET_GOROOT}" -type d -print0 | xargs -n 1 -0 chmod g+s
                find "${TARGET_GOPATH}" -type d -print0 | xargs -n 1 -0 chmod g+s

                # Clean up
                clean_up

                echo "Done!"
                    "#),
            ])
            .await;
            return Ok(http::Response::builder()
                .status(200)
                .body(AsyncBody::from(response))
                .unwrap());
        }
        if parts.uri.path() == "/v2/devcontainers/features/aws-cli/manifests/1" {
            let response = r#"
                {
                    "schemaVersion": 2,
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "config": {
                        "mediaType": "application/vnd.devcontainers",
                        "digest": "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
                        "size": 2
                    },
                    "layers": [
                        {
                            "mediaType": "application/vnd.devcontainers.layer.v1+tar",
                            "digest": "sha256:4e9b04b394fb63e297b3d5f58185406ea45bddb639c2ba83b5a8394643cd5b13",
                            "size": 19968,
                            "annotations": {
                                "org.opencontainers.image.title": "devcontainer-feature-aws-cli.tgz"
                            }
                        }
                    ],
                    "annotations": {
                        "dev.containers.metadata": "{\"id\":\"aws-cli\",\"version\":\"1.1.3\",\"name\":\"AWS CLI\",\"documentationURL\":\"https://github.com/devcontainers/features/tree/main/src/aws-cli\",\"description\":\"Installs the AWS CLI along with needed dependencies. Useful for base Dockerfiles that often are missing required install dependencies like gpg.\",\"options\":{\"version\":{\"type\":\"string\",\"proposals\":[\"latest\"],\"default\":\"latest\",\"description\":\"Select or enter an AWS CLI version.\"},\"verbose\":{\"type\":\"boolean\",\"default\":true,\"description\":\"Suppress verbose output.\"}},\"customizations\":{\"vscode\":{\"extensions\":[\"AmazonWebServices.aws-toolkit-vscode\"],\"settings\":{\"github.copilot.chat.codeGeneration.instructions\":[{\"text\":\"This dev container includes the AWS CLI along with needed dependencies pre-installed and available on the `PATH`, along with the AWS Toolkit extensions for AWS development.\"}]}}},\"installsAfter\":[\"ghcr.io/devcontainers/features/common-utils\"]}",
                        "com.github.package.type": "devcontainer_feature"
                    }
                }"#;
            return Ok(http::Response::builder()
                .status(200)
                .body(AsyncBody::from(response))
                .unwrap());
        }
        if parts.uri.path()
            == "/v2/devcontainers/features/aws-cli/blobs/sha256:4e9b04b394fb63e297b3d5f58185406ea45bddb639c2ba83b5a8394643cd5b13"
        {
            let response = build_tarball(vec![
                (
                    "./devcontainer-feature.json",
                    r#"
{
"id": "aws-cli",
"version": "1.1.3",
"name": "AWS CLI",
"documentationURL": "https://github.com/devcontainers/features/tree/main/src/aws-cli",
"description": "Installs the AWS CLI along with needed dependencies. Useful for base Dockerfiles that often are missing required install dependencies like gpg.",
"options": {
    "version": {
        "type": "string",
        "proposals": [
            "latest"
        ],
        "default": "latest",
        "description": "Select or enter an AWS CLI version."
    },
    "verbose": {
        "type": "boolean",
        "default": true,
        "description": "Suppress verbose output."
    }
},
"customizations": {
    "vscode": {
        "extensions": [
            "AmazonWebServices.aws-toolkit-vscode"
        ],
        "settings": {
            "github.copilot.chat.codeGeneration.instructions": [
                {
                    "text": "This dev container includes the AWS CLI along with needed dependencies pre-installed and available on the `PATH`, along with the AWS Toolkit extensions for AWS development."
                }
            ]
        }
    }
},
"installsAfter": [
    "ghcr.io/devcontainers/features/common-utils"
]
}
                "#,
                ),
                (
                    "./install.sh",
                    r#"#!/usr/bin/env bash
                #-------------------------------------------------------------------------------------------------------------
                # Copyright (c) Microsoft Corporation. All rights reserved.
                # Licensed under the MIT License. See https://go.microsoft.com/fwlink/?linkid=2090316 for license information.
                #-------------------------------------------------------------------------------------------------------------
                #
                # Docs: https://github.com/microsoft/vscode-dev-containers/blob/main/script-library/docs/awscli.md
                # Maintainer: The VS Code and Codespaces Teams

                set -e

                # Clean up
                rm -rf /var/lib/apt/lists/*

                VERSION=${VERSION:-"latest"}
                VERBOSE=${VERBOSE:-"true"}

                AWSCLI_GPG_KEY=FB5DB77FD5C118B80511ADA8A6310ACC4672475C
                AWSCLI_GPG_KEY_MATERIAL="-----BEGIN PGP PUBLIC KEY BLOCK-----

                mQINBF2Cr7UBEADJZHcgusOJl7ENSyumXh85z0TRV0xJorM2B/JL0kHOyigQluUG
                ZMLhENaG0bYatdrKP+3H91lvK050pXwnO/R7fB/FSTouki4ciIx5OuLlnJZIxSzx
                PqGl0mkxImLNbGWoi6Lto0LYxqHN2iQtzlwTVmq9733zd3XfcXrZ3+LblHAgEt5G
                TfNxEKJ8soPLyWmwDH6HWCnjZ/aIQRBTIQ05uVeEoYxSh6wOai7ss/KveoSNBbYz
                gbdzoqI2Y8cgH2nbfgp3DSasaLZEdCSsIsK1u05CinE7k2qZ7KgKAUIcT/cR/grk
                C6VwsnDU0OUCideXcQ8WeHutqvgZH1JgKDbznoIzeQHJD238GEu+eKhRHcz8/jeG
                94zkcgJOz3KbZGYMiTh277Fvj9zzvZsbMBCedV1BTg3TqgvdX4bdkhf5cH+7NtWO
                lrFj6UwAsGukBTAOxC0l/dnSmZhJ7Z1KmEWilro/gOrjtOxqRQutlIqG22TaqoPG
                fYVN+en3Zwbt97kcgZDwqbuykNt64oZWc4XKCa3mprEGC3IbJTBFqglXmZ7l9ywG
                EEUJYOlb2XrSuPWml39beWdKM8kzr1OjnlOm6+lpTRCBfo0wa9F8YZRhHPAkwKkX
                XDeOGpWRj4ohOx0d2GWkyV5xyN14p2tQOCdOODmz80yUTgRpPVQUtOEhXQARAQAB
                tCFBV1MgQ0xJIFRlYW0gPGF3cy1jbGlAYW1hem9uLmNvbT6JAlQEEwEIAD4WIQT7
                Xbd/1cEYuAURraimMQrMRnJHXAUCXYKvtQIbAwUJB4TOAAULCQgHAgYVCgkICwIE
                FgIDAQIeAQIXgAAKCRCmMQrMRnJHXJIXEAChLUIkg80uPUkGjE3jejvQSA1aWuAM
                yzy6fdpdlRUz6M6nmsUhOExjVIvibEJpzK5mhuSZ4lb0vJ2ZUPgCv4zs2nBd7BGJ
                MxKiWgBReGvTdqZ0SzyYH4PYCJSE732x/Fw9hfnh1dMTXNcrQXzwOmmFNNegG0Ox
                au+VnpcR5Kz3smiTrIwZbRudo1ijhCYPQ7t5CMp9kjC6bObvy1hSIg2xNbMAN/Do
                ikebAl36uA6Y/Uczjj3GxZW4ZWeFirMidKbtqvUz2y0UFszobjiBSqZZHCreC34B
                hw9bFNpuWC/0SrXgohdsc6vK50pDGdV5kM2qo9tMQ/izsAwTh/d/GzZv8H4lV9eO
                tEis+EpR497PaxKKh9tJf0N6Q1YLRHof5xePZtOIlS3gfvsH5hXA3HJ9yIxb8T0H
                QYmVr3aIUse20i6meI3fuV36VFupwfrTKaL7VXnsrK2fq5cRvyJLNzXucg0WAjPF
                RrAGLzY7nP1xeg1a0aeP+pdsqjqlPJom8OCWc1+6DWbg0jsC74WoesAqgBItODMB
                rsal1y/q+bPzpsnWjzHV8+1/EtZmSc8ZUGSJOPkfC7hObnfkl18h+1QtKTjZme4d
                H17gsBJr+opwJw/Zio2LMjQBOqlm3K1A4zFTh7wBC7He6KPQea1p2XAMgtvATtNe
                YLZATHZKTJyiqA==
                =vYOk
                -----END PGP PUBLIC KEY BLOCK-----"

                if [ "$(id -u)" -ne 0 ]; then
                    echo -e 'Script must be run as root. Use sudo, su, or add "USER root" to your Dockerfile before running this script.'
                    exit 1
                fi

                apt_get_update()
                {
                    if [ "$(find /var/lib/apt/lists/* | wc -l)" = "0" ]; then
                        echo "Running apt-get update..."
                        apt-get update -y
                    fi
                }

                # Checks if packages are installed and installs them if not
                check_packages() {
                    if ! dpkg -s "$@" > /dev/null 2>&1; then
                        apt_get_update
                        apt-get -y install --no-install-recommends "$@"
                    fi
                }

                export DEBIAN_FRONTEND=noninteractive

                check_packages curl ca-certificates gpg dirmngr unzip bash-completion less

                verify_aws_cli_gpg_signature() {
                    local filePath=$1
                    local sigFilePath=$2
                    local awsGpgKeyring=aws-cli-public-key.gpg

                    echo "${AWSCLI_GPG_KEY_MATERIAL}" | gpg --dearmor > "./${awsGpgKeyring}"
                    gpg --batch --quiet --no-default-keyring --keyring "./${awsGpgKeyring}" --verify "${sigFilePath}" "${filePath}"
                    local status=$?

                    rm "./${awsGpgKeyring}"

                    return ${status}
                }

                install() {
                    local scriptZipFile=awscli.zip
                    local scriptSigFile=awscli.sig

                    # See Linux install docs at https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html
                    if [ "${VERSION}" != "latest" ]; then
                        local versionStr=-${VERSION}
                    fi
                    architecture=$(dpkg --print-architecture)
                    case "${architecture}" in
                        amd64) architectureStr=x86_64 ;;
                        arm64) architectureStr=aarch64 ;;
                        *)
                            echo "AWS CLI does not support machine architecture '$architecture'. Please use an x86-64 or ARM64 machine."
                            exit 1
                    esac
                    local scriptUrl=https://awscli.amazonaws.com/awscli-exe-linux-${architectureStr}${versionStr}.zip
                    curl "${scriptUrl}" -o "${scriptZipFile}"
                    curl "${scriptUrl}.sig" -o "${scriptSigFile}"

                    verify_aws_cli_gpg_signature "$scriptZipFile" "$scriptSigFile"
                    if (( $? > 0 )); then
                        echo "Could not verify GPG signature of AWS CLI install script. Make sure you provided a valid version."
                        exit 1
                    fi

                    if [ "${VERBOSE}" = "false" ]; then
                        unzip -q "${scriptZipFile}"
                    else
                        unzip "${scriptZipFile}"
                    fi

                    ./aws/install

                    # kubectl bash completion
                    mkdir -p /etc/bash_completion.d
                    cp ./scripts/vendor/aws_bash_completer /etc/bash_completion.d/aws

                    # kubectl zsh completion
                    if [ -e "${USERHOME}/.oh-my-zsh" ]; then
                        mkdir -p "${USERHOME}/.oh-my-zsh/completions"
                        cp ./scripts/vendor/aws_zsh_completer.sh "${USERHOME}/.oh-my-zsh/completions/_aws"
                        chown -R "${USERNAME}" "${USERHOME}/.oh-my-zsh"
                    fi

                    rm -rf ./aws
                }

                echo "(*) Installing AWS CLI..."

                install

                # Clean up
                rm -rf /var/lib/apt/lists/*

                echo "Done!""#,
                ),
                ("./scripts/", r#""#),
                (
                    "./scripts/fetch-latest-completer-scripts.sh",
                    r#"
                    #!/bin/bash
                    #-------------------------------------------------------------------------------------------------------------
                    # Copyright (c) Microsoft Corporation. All rights reserved.
                    # Licensed under the MIT License. See https://go.microsoft.com/fwlink/?linkid=2090316 for license information.
                    #-------------------------------------------------------------------------------------------------------------
                    #
                    # Docs: https://github.com/devcontainers/features/tree/main/src/aws-cli
                    # Maintainer: The Dev Container spec maintainers
                    #
                    # Run this script to replace aws_bash_completer and aws_zsh_completer.sh with the latest and greatest available version
                    #
                    COMPLETER_SCRIPTS=$(dirname "${BASH_SOURCE[0]}")
                    BASH_COMPLETER_SCRIPT="$COMPLETER_SCRIPTS/vendor/aws_bash_completer"
                    ZSH_COMPLETER_SCRIPT="$COMPLETER_SCRIPTS/vendor/aws_zsh_completer.sh"

                    wget https://raw.githubusercontent.com/aws/aws-cli/v2/bin/aws_bash_completer -O "$BASH_COMPLETER_SCRIPT"
                    chmod +x "$BASH_COMPLETER_SCRIPT"

                    wget https://raw.githubusercontent.com/aws/aws-cli/v2/bin/aws_zsh_completer.sh -O "$ZSH_COMPLETER_SCRIPT"
                    chmod +x "$ZSH_COMPLETER_SCRIPT"
                    "#,
                ),
                ("./scripts/vendor/", r#""#),
                (
                    "./scripts/vendor/aws_bash_completer",
                    r#"
                    # Typically that would be added under one of the following paths:
                    # - /etc/bash_completion.d
                    # - /usr/local/etc/bash_completion.d
                    # - /usr/share/bash-completion/completions

                    complete -C aws_completer aws
                    "#,
                ),
                (
                    "./scripts/vendor/aws_zsh_completer.sh",
                    r#"
                    # Source this file to activate auto completion for zsh using the bash
                    # compatibility helper.  Make sure to run `compinit` before, which should be
                    # given usually.
                    #
                    # % source /path/to/zsh_complete.sh
                    #
                    # Typically that would be called somewhere in your .zshrc.
                    #
                    # Note, the overwrite of _bash_complete() is to export COMP_LINE and COMP_POINT
                    # That is only required for zsh <= edab1d3dbe61da7efe5f1ac0e40444b2ec9b9570
                    #
                    # https://github.com/zsh-users/zsh/commit/edab1d3dbe61da7efe5f1ac0e40444b2ec9b9570
                    #
                    # zsh releases prior to that version do not export the required env variables!

                    autoload -Uz bashcompinit
                    bashcompinit -i

                    _bash_complete() {
                      local ret=1
                      local -a suf matches
                      local -x COMP_POINT COMP_CWORD
                      local -a COMP_WORDS COMPREPLY BASH_VERSINFO
                      local -x COMP_LINE="$words"
                      local -A savejobstates savejobtexts

                      (( COMP_POINT = 1 + ${#${(j. .)words[1,CURRENT]}} + $#QIPREFIX + $#IPREFIX + $#PREFIX ))
                      (( COMP_CWORD = CURRENT - 1))
                      COMP_WORDS=( $words )
                      BASH_VERSINFO=( 2 05b 0 1 release )

                      savejobstates=( ${(kv)jobstates} )
                      savejobtexts=( ${(kv)jobtexts} )

                      [[ ${argv[${argv[(I)nospace]:-0}-1]} = -o ]] && suf=( -S '' )

                      matches=( ${(f)"$(compgen $@ -- ${words[CURRENT]})"} )

                      if [[ -n $matches ]]; then
                        if [[ ${argv[${argv[(I)filenames]:-0}-1]} = -o ]]; then
                          compset -P '*/' && matches=( ${matches##*/} )
                          compset -S '/*' && matches=( ${matches%%/*} )
                          compadd -Q -f "${suf[@]}" -a matches && ret=0
                        else
                          compadd -Q "${suf[@]}" -a matches && ret=0
                        fi
                      fi

                      if (( ret )); then
                        if [[ ${argv[${argv[(I)default]:-0}-1]} = -o ]]; then
                          _default "${suf[@]}" && ret=0
                        elif [[ ${argv[${argv[(I)dirnames]:-0}-1]} = -o ]]; then
                          _directories "${suf[@]}" && ret=0
                        fi
                      fi

                      return ret
                    }

                    complete -C aws_completer aws
                    "#,
                ),
            ]).await;

            return Ok(http::Response::builder()
                .status(200)
                .body(AsyncBody::from(response))
                .unwrap());
        }

        Ok(http::Response::builder()
            .status(404)
            .body(http_client::AsyncBody::default())
            .unwrap())
    })
}

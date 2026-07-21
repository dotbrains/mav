use std::sync::Arc;

#[path = "docker_in_docker_http.rs"]
mod docker_in_docker_http;

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
        if let Some(response) = docker_in_docker_http::response(parts.uri.path()).await {
            return Ok(response);
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

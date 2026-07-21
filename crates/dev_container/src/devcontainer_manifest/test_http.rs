use std::sync::Arc;

#[path = "docker_in_docker_http.rs"]
mod docker_in_docker_http;
#[path = "go_http.rs"]
mod go_http;

use http_client::{FakeHttpClient, HttpClient};

use crate::oci::TokenResponse;

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

        if let Some(response) = go_http::response(parts.uri.path()).await {
            return Ok(response);
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

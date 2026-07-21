pub(crate) const PART: &str = r#"            check_packages python3-minimal python3-pip libffi-dev python3-venv
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

echo 'docker-in-docker-debian script has completed!'"#;

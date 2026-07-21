pub(crate) const PART: &str = r#"EOF
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
"#;

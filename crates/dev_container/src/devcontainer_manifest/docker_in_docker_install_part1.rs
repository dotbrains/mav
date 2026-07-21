pub(crate) const PART: &str = r#"
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
"#;

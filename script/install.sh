#!/usr/bin/env sh
set -eu

# Downloads a tarball from https://mav.dev/releases and unpacks it
# into ~/.local/. If you'd prefer to do this manually, instructions are at
# https://mav.dev/docs/linux.

main() {
    platform="$(uname -s)"
    arch="$(uname -m)"
    channel="${MAV_CHANNEL:-stable}"
    MAV_VERSION="${MAV_VERSION:-latest}"
    # Use TMPDIR if available (for environments with non-standard temp directories)
    if [ -n "${TMPDIR:-}" ] && [ -d "${TMPDIR}" ]; then
        temp="$(mktemp -d "$TMPDIR/mav-XXXXXX")"
    else
        temp="$(mktemp -d "/tmp/mav-XXXXXX")"
    fi

    if [ "$platform" = "Darwin" ]; then
        platform="macos"
    elif [ "$platform" = "Linux" ]; then
        platform="linux"
    else
        echo "Unsupported platform $platform"
        exit 1
    fi

    case "$platform-$arch" in
        macos-arm64* | linux-arm64* | linux-armhf | linux-aarch64)
            arch="aarch64"
            ;;
        macos-x86* | linux-x86* | linux-i686*)
            arch="x86_64"
            ;;
        *)
            echo "Unsupported platform or architecture"
            exit 1
            ;;
    esac

    if command -v curl >/dev/null 2>&1; then
        curl () {
            command curl -fL "$@"
        }
    elif command -v wget >/dev/null 2>&1; then
        curl () {
            wget -O- "$@"
        }
    else
        echo "Could not find 'curl' or 'wget' in your path"
        exit 1
    fi

    "$platform" "$@"

    if [ "$(command -v mav)" = "$HOME/.local/bin/mav" ]; then
        echo "Mav has been installed. Run with 'mav'"
    else
        echo "To run Mav from your terminal, you must add ~/.local/bin to your PATH"
        echo "Run:"

        case "$SHELL" in
            *zsh)
                echo "   echo 'export PATH=\$HOME/.local/bin:\$PATH' >> ~/.zshrc"
                echo "   source ~/.zshrc"
                ;;
            *fish)
                echo "   fish_add_path -U $HOME/.local/bin"
                ;;
            *)
                echo "   echo 'export PATH=\$HOME/.local/bin:\$PATH' >> ~/.bashrc"
                echo "   source ~/.bashrc"
                ;;
        esac

        echo "To run Mav now, '~/.local/bin/mav'"
    fi
}

linux() {
    if [ -n "${MAV_BUNDLE_PATH:-}" ]; then
        cp "$MAV_BUNDLE_PATH" "$temp/mav-linux-$arch.tar.gz"
    else
        echo "Downloading Mav version: $MAV_VERSION"
        curl "https://cloud.mav.dev/releases/$channel/$MAV_VERSION/download?asset=mav&arch=$arch&os=linux&source=install.sh" > "$temp/mav-linux-$arch.tar.gz"
    fi

    suffix=""
    if [ "$channel" != "stable" ]; then
        suffix="-$channel"
    fi

    appid=""
    case "$channel" in
      stable)
        appid="dev.mav.Mav"
        ;;
      nightly)
        appid="dev.mav.Mav-Nightly"
        ;;
      preview)
        appid="dev.mav.Mav-Preview"
        ;;
      dev)
        appid="dev.mav.Mav-Dev"
        ;;
      *)
        echo "Unknown release channel: ${channel}. Using stable app ID."
        appid="dev.mav.Mav"
        ;;
    esac

    # Unpack
    rm -rf "$HOME/.local/mav$suffix.app"
    mkdir -p "$HOME/.local/mav$suffix.app"
    tar -xzf "$temp/mav-linux-$arch.tar.gz" -C "$HOME/.local/"

    # Setup ~/.local directories
    mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"

    # Link the binary
    if [ -f "$HOME/.local/mav$suffix.app/bin/mav" ]; then
        ln -sf "$HOME/.local/mav$suffix.app/bin/mav" "$HOME/.local/bin/mav"
    else
        # support for versions before 0.139.x.
        ln -sf "$HOME/.local/mav$suffix.app/bin/cli" "$HOME/.local/bin/mav"
    fi

    # Copy .desktop file
    desktop_file_path="$HOME/.local/share/applications/${appid}.desktop"
    src_dir="$HOME/.local/mav$suffix.app/share/applications"
    if [ -f "$src_dir/${appid}.desktop" ]; then
        cp "$src_dir/${appid}.desktop" "${desktop_file_path}"
    else
        # Fallback for older tarballs
        cp "$src_dir/mav$suffix.desktop" "${desktop_file_path}"
    fi
    sed -i "s|Icon=mav|Icon=$HOME/.local/mav$suffix.app/share/icons/hicolor/512x512/apps/mav.png|g" "${desktop_file_path}"
    sed -i "s|Exec=mav|Exec=$HOME/.local/mav$suffix.app/bin/mav|g" "${desktop_file_path}"
}

macos() {
    echo "Downloading Mav version: $MAV_VERSION"
    curl "https://cloud.mav.dev/releases/$channel/$MAV_VERSION/download?asset=mav&os=macos&arch=$arch&source=install.sh" > "$temp/Mav-$arch.dmg"
    hdiutil attach -quiet "$temp/Mav-$arch.dmg" -mountpoint "$temp/mount"
    app="$(cd "$temp/mount/"; echo *.app)"
    echo "Installing $app"
    if [ -d "/Applications/$app" ]; then
        echo "Removing existing $app"
        rm -rf "/Applications/$app"
    fi
    ditto "$temp/mount/$app" "/Applications/$app"
    hdiutil detach -quiet "$temp/mount"

    mkdir -p "$HOME/.local/bin"
    # Link the binary
    ln -sf "/Applications/$app/Contents/MacOS/cli" "$HOME/.local/bin/mav"
}

main "$@"

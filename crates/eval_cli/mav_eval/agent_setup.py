"""Best-effort environment setup helpers for the Harbor Mav agent."""

import shlex
from typing import Any

from harbor.environments.base import BaseEnvironment


async def install_node(agent: Any, environment: BaseEnvironment) -> None:
    """Install Node.js from official binary tarballs.

    Uses the musl build on Alpine and the glibc build elsewhere.
    Skips if node is already on PATH.
    """
    try:
        await agent.exec_as_root(
            environment,
            command=(
                "if command -v node >/dev/null 2>&1; then "
                '  echo "Node.js already available: $(node --version)"; '
                "else "
                "  NODE_VER=v22.14.0; "
                "  ARCH=$(uname -m); "
                '  case "$ARCH" in '
                "    x86_64)  NODE_ARCH=x64  ;; "
                "    aarch64) NODE_ARCH=arm64 ;; "
                '    *)       echo "WARNING: unsupported arch $ARCH for Node.js" >&2; exit 0 ;; '
                "  esac; "
                "  if ldd /bin/sh 2>&1 | grep -qi musl; then "
                '    NODE_URL="https://unofficial-builds.nodejs.org/download/release/${NODE_VER}/node-${NODE_VER}-linux-${NODE_ARCH}-musl.tar.gz"; '
                "  else "
                '    NODE_URL="https://nodejs.org/dist/${NODE_VER}/node-${NODE_VER}-linux-${NODE_ARCH}.tar.gz"; '
                "  fi; "
                '  echo "Downloading Node.js from $NODE_URL"; '
                '  curl -fsSL "$NODE_URL" | tar -xz -C /usr/local --strip-components=1; '
                '  echo "Installed Node.js $(node --version)"; '
                "fi"
            ),
        )
    except Exception as exc:
        agent.logger.warning("Node.js installation failed (non-fatal): %s", exc)


async def install_lsps(agent: Any, environment: BaseEnvironment) -> None:
    """Pre-install language servers so Mav doesn't download them at runtime."""
    # npm-based LSPs - skip all if npm is not available.
    try:
        await agent.exec_as_agent(
            environment,
            command="command -v npm >/dev/null 2>&1",
        )
    except Exception:
        agent.logger.warning("npm not available - skipping npm-based LSP installs")
        return

    lsp_installs = [
        (
            "basedpyright",
            'DIR="$MAV_DATA_DIR/languages/basedpyright"; '
            'mkdir -p "$DIR" && npm install --prefix "$DIR" --save-exact basedpyright',
        ),
        (
            "typescript-language-server",
            'DIR="$MAV_DATA_DIR/languages/typescript-language-server"; '
            'mkdir -p "$DIR" && npm install --prefix "$DIR" --save-exact typescript typescript-language-server',
        ),
        (
            "vtsls",
            'DIR="$MAV_DATA_DIR/languages/vtsls"; '
            'mkdir -p "$DIR" && npm install --prefix "$DIR" --save-exact @vtsls/language-server typescript',
        ),
        (
            "tailwindcss-language-server",
            'DIR="$MAV_DATA_DIR/languages/tailwindcss-language-server"; '
            'mkdir -p "$DIR" && npm install --prefix "$DIR" --save-exact @tailwindcss/language-server',
        ),
    ]

    for name, cmd in lsp_installs:
        try:
            await agent.exec_as_agent(
                environment,
                command=(
                    'MAV_DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/mav"; '
                    + cmd
                ),
            )
        except Exception as exc:
            agent.logger.warning("LSP install '%s' failed (non-fatal): %s", name, exc)

    await install_eslint(agent, environment)
    await install_gopls(agent, environment)


async def install_eslint(agent: Any, environment: BaseEnvironment) -> None:
    try:
        await agent.exec_as_agent(
            environment,
            command=(
                "set -euo pipefail; "
                'MAV_DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/mav"; '
                'ESLINT_DIR="$MAV_DATA_DIR/languages/eslint/vscode-eslint-2.4.4"; '
                'mkdir -p "$ESLINT_DIR"; '
                'curl -fsSL "https://github.com/mav-industries/vscode-eslint/archive/refs/tags/release/2.4.4.tar.gz" '
                '| tar -xz -C "$ESLINT_DIR"; '
                'mv "$ESLINT_DIR"/vscode-eslint-release-2.4.4 "$ESLINT_DIR/vscode-eslint"; '
                'cd "$ESLINT_DIR/vscode-eslint" && npm install && npm run compile'
            ),
        )
    except Exception as exc:
        agent.logger.warning("eslint LSP install failed (non-fatal): %s", exc)


async def install_gopls(agent: Any, environment: BaseEnvironment) -> None:
    # Only when Go is present. Guarded by a 120s timeout so slow compilation can
    # never eat the full setup budget.
    gopls_script = (
        "if command -v go >/dev/null 2>&1; then "
        "if go install golang.org/x/tools/gopls@latest 2>/dev/null; then "
        "echo 'Installed gopls@latest'; "
        "else "
        '  MY_GO=$(go env GOVERSION | sed "s/^go//"); '
        "  for v in $(curl -fsSL "
        "https://proxy.golang.org/golang.org/x/tools/gopls/@v/list 2>/dev/null"
        " | grep -E '^v[0-9]+\\.[0-9]+\\.[0-9]+$' | sort -rV | head -5); do "
        "    NEED=$(curl -fsSL "
        '"https://proxy.golang.org/golang.org/x/tools/gopls/@v/${v}.mod"'
        " 2>/dev/null | awk '/^go /{print $2; exit}'); "
        '    if [ -n "$NEED" ] '
        '    && [ "$(printf \'%s\\n%s\\n\' "$NEED" "$MY_GO" '
        '         | sort -V | head -1)" = "$NEED" ]; then '
        '      echo "Installing gopls $v (compatible with Go $MY_GO)"; '
        '      go install "golang.org/x/tools/gopls@$v" && break; '
        "    fi; "
        "  done; "
        "fi; "
        "fi"
    )
    try:
        await agent.exec_as_agent(
            environment,
            command=(
                "timeout 120 bash -c "
                + shlex.quote(gopls_script)
                + " || echo 'WARNING: gopls installation timed out or failed -- skipping'"
            ),
        )
    except Exception as exc:
        agent.logger.warning("gopls install failed (non-fatal): %s", exc)


async def install_uv_and_ruff(agent: Any, environment: BaseEnvironment) -> None:
    """Install uv and ruff for Python tooling."""
    try:
        await agent.exec_as_agent(
            environment,
            command=(
                "curl -LsSf https://astral.sh/uv/install.sh | sh && "
                '. "$HOME/.local/bin/env"'
            ),
        )

        agent_home_result = await agent.exec_as_agent(
            environment,
            command='printf %s "$HOME"',
        )
        agent_home = agent_home_result.stdout.strip()
        if not agent_home:
            agent.logger.warning(
                "Could not determine agent home directory - skipping uv symlinks"
            )
            return

        await agent.exec_as_root(
            environment,
            command=(
                f"ln -sf {shlex.quote(agent_home + '/.local/bin/uv')} /usr/local/bin/uv && "
                f"ln -sf {shlex.quote(agent_home + '/.local/bin/uvx')} /usr/local/bin/uvx"
            ),
        )

        await agent.exec_as_agent(
            environment,
            command='export PATH="$HOME/.local/bin:$PATH" && uv tool install ruff',
        )
    except Exception as exc:
        agent.logger.warning("uv/ruff installation failed (non-fatal): %s", exc)

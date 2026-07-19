# mav

[![Private](https://img.shields.io/badge/repo-private-black)](https://github.com/dotbrains/mav)
[![Rust](https://img.shields.io/badge/rust-stable-f74c00)](https://www.rust-lang.org/)
[![Nix](https://img.shields.io/badge/nix-flake-5277c3)](./flake.nix)
[![Flox](https://img.shields.io/badge/flox-supported-6b46c1)](./.flox/env/manifest.toml)
[![CI](https://github.com/dotbrains/mav/actions/workflows/ci.yml/badge.svg)](https://github.com/dotbrains/mav/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-PolyForm%20Shield-blue)](./LICENSE)

Private dotbrains workspace for the Zed editor core, bundled assets, and local extension development.

## Quick Start

```bash
flox activate
cargo run -p zed --bin mav
```

## What Is Kept

| Path | Purpose |
| --- | --- |
| `crates/` | Rust workspace for the editor, UI, language, extension, agent, terminal, and platform crates. |
| `assets/` | Runtime assets, default settings, keymaps, themes, icons, prompts, sounds, and images. |
| `extensions/` | First-party extension fixtures and examples used by the extension host and extension CLI. |
| `nix/`, `flake.nix`, `shell.nix`, `default.nix` | Nix package and development shell support. |
| `.flox/` | Flox environment for repeatable local development. |
| `tooling/`, `script/` | Build, workflow, license, formatting, and maintenance commands still needed by the workspace. |

## Common Tasks

```bash
flox activate
cargo fmt --all -- --check
cargo check -p zed --bin mav
cargo test -p extension_api
nix flake check
pre-commit run --all-files
```

## Documentation

- [Getting started](./docs/getting-started.md)
- [Architecture](./docs/architecture.md)
- [Development](./docs/development.md)
- [Extensions](./docs/extensions.md)
- [Nix and Flox](./docs/nix-and-flox.md)
- [CI](./docs/ci.md)
- [Testing](./docs/testing.md)
- [Releasing](./docs/releasing.md)

## License

This repository uses the [PolyForm Shield License 1.0.0](./LICENSE).

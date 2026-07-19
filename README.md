# mav

[![CI](https://img.shields.io/badge/ci-private%20actions-2088ff.svg)](https://github.com/dotbrains/mav/actions/workflows/ci.yml)
[![Private](https://img.shields.io/badge/repo-private-111827.svg)](https://github.com/dotbrains/mav)
[![License: PolyForm Shield](https://img.shields.io/badge/license-PolyForm%20Shield-blue.svg)](LICENSE)
[![Platform: macOS + Linux](https://img.shields.io/badge/platform-macOS%20%2B%20Linux-lightgrey.svg)](docs/getting-started.md)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](CONTRIBUTING.md)
[![pre-commit](https://img.shields.io/badge/pre--commit-enabled-brightgreen?logo=pre-commit&logoColor=white)](.pre-commit-config.yaml)
[![Flox](https://img.shields.io/badge/dev%20env-flox-7c3aed.svg)](https://flox.dev)
[![Nix](https://img.shields.io/badge/nix-flake-5277c3.svg)](flake.nix)

Private **dotbrains editor workspace for macOS and Linux**. `mav` keeps the
application runtime, extension host, bundled assets, reproducible development
shells, and CI checks in one private repository.

```console
$ git clone https://github.com/dotbrains/mav.git
$ cd mav
$ flox activate
$ cargo run -p mav --bin mav
```

See [docs/architecture.md](docs/architecture.md) for the workspace layout and
[docs/getting-started.md](docs/getting-started.md) for first-run setup.

## Install

This repository is private. Authenticate before cloning or fetching release
artifacts:

```sh
gh auth login
git clone https://github.com/dotbrains/mav.git
cd mav
```

Use [Flox](https://flox.dev) for the fastest setup:

```sh
flox activate
cargo check -p mav --bin mav --locked
```

Nix users can enter the same pinned toolchain with:

```sh
nix develop
cargo check -p mav --bin mav --locked
```

## Development

```sh
flox activate
pre-commit run --all-files
cargo check -p mav --bin mav --locked
```

See **[CONTRIBUTING.md](CONTRIBUTING.md)** for the contributor workflow,
**[docs/testing.md](docs/testing.md)** for test guidance, **[docs/ci.md](docs/ci.md)**
for CI coverage, and **[docs/nix-and-flox.md](docs/nix-and-flox.md)** for the
reproducible development environments.

[PolyForm Shield License 1.0.0](LICENSE).

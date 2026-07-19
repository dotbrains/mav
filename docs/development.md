# Development

Use Flox for the normal local shell:

```bash
flox activate
```

Use Nix when validating package behavior:

```bash
nix develop
nix build .#default
```

## Formatting

```bash
cargo fmt --all -- --check
```

## Build Checks

```bash
cargo check -p zed --bin mav --locked
```

For narrower work, check the crate you touched first, then check the binary:

```bash
cargo check -p extension_host
cargo check -p zed --bin mav --locked
```

## Pre-Commit

```bash
pre-commit install
pre-commit run --all-files
```

The pre-commit hooks enforce formatting, the main binary check, GitHub Actions syntax, typo checks, and Blacksmith runner policy.

## Dependency Changes

When changing Rust dependencies:

```bash
cargo update -p <package>
cargo check -p zed --bin mav --locked
```

Commit `Cargo.toml` and `Cargo.lock` together. Do not hand-edit lockfile entries.

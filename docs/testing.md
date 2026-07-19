# Testing

The full workspace test surface is large. Prefer targeted tests while developing, then run broader checks before merging risky changes.

## Fast Checks

```bash
cargo check -p mav --bin mav --locked
cargo test -p extension_api
cargo test -p language
```

## Nextest

```bash
cargo nextest run -p extension_api
```

## Nix

```bash
nix flake check
```

## Pre-Commit

```bash
pre-commit run --all-files
```

## Risk-Based Expansion

Editing UI composition, workspace persistence, language services, extension host behavior, or platform backends should trigger tests for the touched crate plus `cargo check -p mav --bin mav --locked`.

Editing assets or settings should include a binary check because assets are embedded and validated through compile-time/runtime paths.

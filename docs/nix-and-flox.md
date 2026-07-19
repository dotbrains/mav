# Nix And Flox

`mav` supports both Flox and Nix.

Flox is the default contributor environment. It installs common tools and keeps the local workflow short:

```bash
flox activate
cargo check -p zed --bin mav --locked
```

Nix is the package and reproducibility layer:

```bash
nix develop
nix flake check
nix build .#default -L --accept-flake-config
```

## When To Use Each

Use Flox for day-to-day edits, formatting, tests, and CI hygiene.

Use Nix for packaging validation, closure debugging, and platform-specific dependency issues.

## Updating Tooling

Update `.flox/env/manifest.toml` when adding a tool to the standard developer loop.

Update `flake.nix` and files under `nix/` when changing package inputs, overlays, build dependencies, or supported systems.

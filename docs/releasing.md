# Releasing

`mav` does not currently include public release automation. The private port keeps build and packaging support, but public release workflows, hosted-service deployment, and community automation were removed.

## Current Release Posture

- Source repository: private GitHub repository under `dotbrains`.
- License: PolyForm Shield License 1.0.0.
- Build validation: Cargo, Nix, Flox, and pre-commit.
- Distribution: manual until a dotbrains release channel is defined.

## Manual Build

```bash
flox activate
cargo build -p zed --bin mav --release
```

For Nix:

```bash
nix build .#default -L --accept-flake-config
```

## Before Adding Automation

Define the target platforms, signing requirements, update channel, artifact naming, crash/telemetry posture, and secret storage model before adding release workflows.

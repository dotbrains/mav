# CI

CI is intentionally small for the public port. It verifies formatting, the main editor binary, the Nix flake, and documentation links.

```mermaid
flowchart TD
  pr["push or pull request"] --> hygiene["hygiene: Flox + cargo fmt/check"]
  pr --> nix["nix: flake check"]
  pr --> docs["docs: offline markdown links"]
```

## Runner Policy

All Linux jobs use GitHub-hosted runners:

```yaml
runs-on: ubuntu-latest
```

Do not introduce self-hosted or third-party runner labels unless a job explicitly needs dedicated infrastructure.

## Adding Jobs

Prefer small jobs with clear ownership:

- `hygiene` for formatting, linting, and quick compile checks.
- `test` for targeted Rust test suites.
- `nix` for package and flake validation.
- `docs` for documentation quality gates.

Keep secrets out of CI unless a private deployment or release workflow explicitly needs them.

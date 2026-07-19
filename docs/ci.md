# CI

CI is intentionally small for the private port. It verifies formatting, the main editor binary, the Nix flake, and documentation links.

```mermaid
flowchart TD
  pr["push or pull request"] --> hygiene["hygiene: Flox + cargo fmt/check"]
  pr --> nix["nix: flake check"]
  pr --> docs["docs: offline markdown links"]
```

## Runner Policy

All Linux jobs use Blacksmith runners:

```yaml
runs-on: blacksmith-4vcpu-ubuntu-2404
```

Do not introduce `ubuntu-latest` or `ubuntu-*` runners. The pre-commit hook checks this policy.

## Adding Jobs

Prefer small jobs with clear ownership:

- `hygiene` for formatting, linting, and quick compile checks.
- `test` for targeted Rust test suites.
- `nix` for package and flake validation.
- `docs` for documentation quality gates.

Keep secrets out of CI unless a private deployment or release workflow explicitly needs them.

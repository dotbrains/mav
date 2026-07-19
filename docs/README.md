# mav docs

This directory is the user-facing documentation set for `mav`. Start here when
you need setup instructions, contribution workflows, environment details, or
release guidance beyond the slim root README.

## Start here

| Document | Use it for |
|---|---|
| [getting-started.md](getting-started.md) | Cloning the private repository, authenticating with GitHub, entering a development shell, and running the app locally |
| [development.md](development.md) | Daily development workflow, repository hygiene, local commands, and expected contributor habits |
| [nix-and-flox.md](nix-and-flox.md) | Reproducible development shells with Flox and Nix, including when to use each environment |
| [testing.md](testing.md) | Local test and check commands that mirror the CI expectations |

## Reference

| Document | Use it for |
|---|---|
| [architecture.md](architecture.md) | Workspace structure, crate responsibilities, runtime boundaries, and how the major pieces fit together |
| [extensions.md](extensions.md) | Extension API work, extension checks, and bundled extension development notes |
| [ci.md](ci.md) | GitHub Actions jobs, required checks, and troubleshooting failed runs |
| [releasing.md](releasing.md) | Private release handling, versioning expectations, and artifact verification |

## Common paths

Use Flox for the default local workflow:

```sh
flox activate
cargo check -p mav --bin mav --locked
pre-commit run --all-files
```

Use Nix when you want to verify the checked-in flake:

```sh
nix develop
cargo check -p mav --bin mav --locked
```

Check documentation links before pushing README or docs changes:

```sh
lychee --offline README.md docs/**/*.md
```

# mav Extensions

This directory contains extensions and fixtures used by the private mav editor workspace.

Registry publishing is intentionally outside the current private repository scope.

## Structure

The editor includes support for a number of languages without requiring an installed extension. Those languages live under `crates/languages/src`.

Support for all other languages is done via extensions. This directory contains the extension fixtures and examples kept with the editor source for local development.

## Dev Extensions

See [docs/extensions.md](../docs/extensions.md) for local extension development commands.

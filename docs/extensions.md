# Extensions

The `extensions/` directory contains extension fixtures and examples used by the editor extension host. The Rust extension API lives in `crates/extension_api`; the host/runtime implementation lives across `crates/extension`, `crates/extension_host`, `crates/extensions_ui`, and language-specific crates.

## Local Extension Check

```bash
flox activate
rustup target add wasm32-wasip2
cargo check -p extension_api
```

For a specific extension crate:

```bash
cargo check --manifest-path extensions/glsl/Cargo.toml --target wasm32-wasip2
```

## Extension Layout

Each extension should keep:

- `extension.toml` for extension metadata.
- `Cargo.toml` when it ships Rust/Wasm code.
- Language queries, grammars, or runtime assets required by that extension.
- A short README when the extension is not self-explanatory.

Do not add registry publishing automation unless this repository starts publishing extensions externally.

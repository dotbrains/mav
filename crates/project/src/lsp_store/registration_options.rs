use anyhow::Result;
use lsp::OneOf;

// Registration with registerOptions as null should fall back to true.
// See vscode-languageserver-node client dynamic registration handling.
pub(super) fn parse_register_capabilities<T: serde::de::DeserializeOwned>(
    reg: lsp::Registration,
) -> Result<OneOf<bool, T>> {
    Ok(match reg.register_options {
        Some(options) => OneOf::Right(serde_json::from_value::<T>(options)?),
        None => OneOf::Left(true),
    })
}

pub(super) fn server_capabilities_support_range_formatting(
    capabilities: &lsp::ServerCapabilities,
) -> bool {
    matches!(
        capabilities.document_range_formatting_provider.as_ref(),
        Some(provider) if *provider != OneOf::Left(false)
    )
}

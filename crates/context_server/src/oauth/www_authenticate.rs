use super::*;

/// Fields extracted from a `WWW-Authenticate: Bearer` header.
///
/// Per RFC 9728 Section 5.1, MCP servers include `resource_metadata` to point
/// at the Protected Resource Metadata document. The optional `scope` parameter
/// (RFC 6750 Section 3) indicates scopes required for the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WwwAuthenticate {
    pub resource_metadata: Option<Url>,
    pub scope: Option<Vec<String>>,
    /// The parsed `error` parameter per RFC 6750 Section 3.1.
    pub error: Option<BearerError>,
    pub error_description: Option<String>,
}
/// Parse a `WWW-Authenticate` header value.
///
/// Expects the `Bearer` scheme followed by comma-separated `key="value"` pairs.
/// Per RFC 6750 and RFC 9728, the relevant parameters are:
/// - `resource_metadata` — URL of the Protected Resource Metadata document
/// - `scope` — space-separated list of required scopes
/// - `error` — error code (e.g. "insufficient_scope")
/// - `error_description` — human-readable error description
pub fn parse_www_authenticate(header: &str) -> Result<WwwAuthenticate> {
    let header = header.trim();

    let params_str = if header.len() >= 6 && header[..6].eq_ignore_ascii_case("bearer") {
        header[6..].trim()
    } else {
        bail!("WWW-Authenticate header does not use Bearer scheme");
    };

    if params_str.is_empty() {
        return Ok(WwwAuthenticate {
            resource_metadata: None,
            scope: None,
            error: None,
            error_description: None,
        });
    }

    let params = parse_auth_params(params_str);

    let resource_metadata = params
        .get("resource_metadata")
        .map(|v| Url::parse(v))
        .transpose()
        .map_err(|e| anyhow!("invalid resource_metadata URL: {}", e))?;

    let scope = params
        .get("scope")
        .map(|v| v.split_whitespace().map(String::from).collect());

    let error = params.get("error").map(|v| BearerError::parse(v));
    let error_description = params.get("error_description").cloned();

    Ok(WwwAuthenticate {
        resource_metadata,
        scope,
        error,
        error_description,
    })
}

/// Parse comma-separated `key="value"` or `key=token` parameters from an
/// auth-param list (RFC 7235 Section 2.1).
fn parse_auth_params(input: &str) -> collections::HashMap<String, String> {
    let mut params = collections::HashMap::default();
    let mut remaining = input.trim();

    while !remaining.is_empty() {
        // Skip leading whitespace and commas.
        remaining = remaining.trim_start_matches(|c: char| c == ',' || c.is_whitespace());
        if remaining.is_empty() {
            break;
        }

        // Find the key (everything before '=').
        let eq_pos = match remaining.find('=') {
            Some(pos) => pos,
            None => break,
        };

        let key = remaining[..eq_pos].trim().to_lowercase();
        remaining = &remaining[eq_pos + 1..];
        remaining = remaining.trim_start();

        // Parse the value: either quoted or unquoted (token).
        let value;
        if remaining.starts_with('"') {
            // Quoted string: find the closing quote, handling escaped chars.
            remaining = &remaining[1..]; // skip opening quote
            let mut val = String::new();
            let mut chars = remaining.char_indices();
            loop {
                match chars.next() {
                    Some((_, '\\')) => {
                        // Escaped character — take the next char literally.
                        if let Some((_, c)) = chars.next() {
                            val.push(c);
                        }
                    }
                    Some((i, '"')) => {
                        remaining = &remaining[i + 1..];
                        break;
                    }
                    Some((_, c)) => val.push(c),
                    None => {
                        remaining = "";
                        break;
                    }
                }
            }
            value = val;
        } else {
            // Unquoted token: read until comma or whitespace.
            let end = remaining
                .find(|c: char| c == ',' || c.is_whitespace())
                .unwrap_or(remaining.len());
            value = remaining[..end].to_string();
            remaining = &remaining[end..];
        }

        if !key.is_empty() {
            params.insert(key, value);
        }
    }

    params
}

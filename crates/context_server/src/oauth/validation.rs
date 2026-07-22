use super::*;

pub const CIMD_URL: &str = "https://mav.dev/oauth/client-metadata.json";
/// Validate that a URL is safe to use as an OAuth endpoint.
///
/// OAuth endpoints carry sensitive material (authorization codes, PKCE
/// verifiers, tokens) and must use TLS. Plain HTTP is only permitted for
/// loopback addresses, per RFC 8252 Section 8.3.
pub(super) fn require_https_or_loopback(url: &Url) -> Result<()> {
    if url.scheme() == "https" {
        return Ok(());
    }
    if url.scheme() == "http" {
        if let Some(host) = url.host() {
            match host {
                url::Host::Ipv4(ip) if ip.is_loopback() => return Ok(()),
                url::Host::Ipv6(ip) if ip.is_loopback() => return Ok(()),
                url::Host::Domain(d) if d.eq_ignore_ascii_case("localhost") => return Ok(()),
                _ => {}
            }
        }
    }
    bail!(
        "OAuth endpoint must use HTTPS (got {}://{})",
        url.scheme(),
        url.host_str().unwrap_or("?")
    )
}

/// Validate that a URL is safe to use as an OAuth endpoint, including SSRF
/// protections against private/reserved IP ranges.
///
/// This wraps [`require_https_or_loopback`] and adds IP-range checks to prevent
/// an attacker-controlled MCP server from directing Mav to fetch internal
/// network resources via metadata URLs.
///
/// **Known limitation:** Domain-name URLs that resolve to private IPs are *not*
/// blocked here — full mitigation requires resolver-level validation (e.g. a
/// custom `Resolve` implementation). This function only blocks IP-literal URLs.
pub(super) fn validate_oauth_url(url: &Url) -> Result<()> {
    require_https_or_loopback(url)?;

    if let Some(host) = url.host() {
        match host {
            url::Host::Ipv4(ip) => {
                // Loopback is already allowed by require_https_or_loopback.
                if ip.is_private() || ip.is_link_local() || ip.is_broadcast() || ip.is_unspecified()
                {
                    bail!(
                        "OAuth endpoint must not point to private/reserved IP: {}",
                        ip
                    );
                }
            }
            url::Host::Ipv6(ip) => {
                // Check for IPv4-mapped IPv6 addresses (::ffff:a.b.c.d) which
                // could bypass the IPv4 checks above.
                if let Some(mapped_v4) = ip.to_ipv4_mapped() {
                    if mapped_v4.is_private()
                        || mapped_v4.is_link_local()
                        || mapped_v4.is_broadcast()
                        || mapped_v4.is_unspecified()
                    {
                        bail!(
                            "OAuth endpoint must not point to private/reserved IP: ::ffff:{}",
                            mapped_v4
                        );
                    }
                }

                if ip.is_unspecified() || ip.is_multicast() {
                    bail!(
                        "OAuth endpoint must not point to reserved IPv6 address: {}",
                        ip
                    );
                }
                // IPv6 Unique Local Addresses (fc00::/7). is_unique_local() is
                // nightly-only, so check the prefix manually.
                if (ip.segments()[0] & 0xfe00) == 0xfc00 {
                    bail!(
                        "OAuth endpoint must not point to IPv6 unique-local address: {}",
                        ip
                    );
                }
            }
            url::Host::Domain(_) => {
                // Domain-based SSRF prevention requires resolver-level checks.
                // See known limitation in the doc comment above.
            }
        }
    }

    Ok(())
}

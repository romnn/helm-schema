use std::net::{IpAddr, Ipv6Addr};

/// Explicit policy for schema/document retrieval during input assembly.
///
/// This governs chart-authored and override-authored external references that
/// helm-schema may load while preparing a self-contained schema document.
/// Knowledge-provider fetching remains controlled separately by
/// [`crate::provider_builder::ProviderOptions`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchPolicy {
    allow_file: bool,
    allow_network: bool,
}

impl FetchPolicy {
    /// Creates an explicit local-file and network retrieval policy.
    #[must_use]
    pub const fn new(allow_file: bool, allow_network: bool) -> Self {
        Self {
            allow_file,
            allow_network,
        }
    }

    /// Policy for chart-local and override-local input assembly. Local files
    /// remain readable; network refs depend on the caller's offline policy.
    #[must_use]
    pub const fn input_assembly(allow_network: bool) -> Self {
        Self::new(true, allow_network)
    }

    pub(crate) fn validate_file_host(self, host: &str) -> Result<(), String> {
        if !self.allow_file {
            return Err("local file access is disabled by fetch policy".to_string());
        }
        if host.is_empty() {
            return Ok(());
        }
        Err(format!(
            "file:// authority host is not allowed by fetch policy: {host}"
        ))
    }

    pub(crate) fn validate_network_host(self, host: Option<&str>) -> Result<(), String> {
        if !self.allow_network {
            return Err("network access is disabled by fetch policy".to_string());
        }

        let Some(host) = host else {
            return Err("network URI is missing an authority host".to_string());
        };
        let normalized = host.trim_end_matches('.').to_ascii_lowercase();
        if normalized == "localhost" || normalized.ends_with(".localhost") {
            return Err(format!(
                "network host is denied by fetch policy because it is loopback-local: {host}"
            ));
        }

        let ip = parse_ip_literal(host);
        if let Some(ip) = ip
            && is_denied_ip(ip)
        {
            return Err(format!(
                "network host is denied by fetch policy because it is loopback/link-local: {host}"
            ));
        }

        Ok(())
    }
}

fn parse_ip_literal(host: &str) -> Option<IpAddr> {
    if let Some(inner) = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        return inner.parse::<Ipv6Addr>().ok().map(IpAddr::V6);
    }
    host.parse::<IpAddr>().ok()
}

fn is_denied_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(addr) => addr.is_loopback() || addr.is_link_local() || addr.is_unspecified(),
        IpAddr::V6(addr) => {
            addr.is_loopback() || addr.is_unicast_link_local() || addr == Ipv6Addr::UNSPECIFIED
        }
    }
}

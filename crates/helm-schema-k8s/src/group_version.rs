//! Kubernetes apiVersion syntax: `version` for the core group, else
//! `group/version`.

/// Longest DNS label Kubernetes accepts.
const MAX_LABEL_LEN: usize = 63;

/// Longest DNS subdomain Kubernetes accepts.
const MAX_SUBDOMAIN_LEN: usize = 253;

/// A syntactically valid apiVersion, split into its group and version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GroupVersion<'a> {
    /// API group; empty for the core group.
    pub(crate) group: &'a str,
    /// API version within the group, such as `v1` or `v2beta1`.
    pub(crate) version: &'a str,
}

impl<'a> GroupVersion<'a> {
    /// Parses `api_version` under Kubernetes naming: the group is a DNS-1123
    /// subdomain and the version a DNS-1035 label (`v1`, `v1beta1`,
    /// `v2alpha3`). Anything else, such as the lone `"` a quoted template
    /// once leaked or `autoscaling/` with an unresolved version, is `None`:
    /// it names no schema, so callers treat the apiVersion as unknown.
    pub(crate) fn parse(api_version: &'a str) -> Option<Self> {
        let (group, version) = match api_version.split_once('/') {
            Some((group, version)) => (is_dns_subdomain(group).then_some(group)?, version),
            None => ("", api_version),
        };
        is_dns_label(version, |first| first.is_ascii_lowercase()).then_some(Self { group, version })
    }
}

/// A DNS-1123 subdomain: dot-separated DNS labels, 253 bytes at most.
fn is_dns_subdomain(text: &str) -> bool {
    text.len() <= MAX_SUBDOMAIN_LEN
        && text
            .split('.')
            .all(|label| is_dns_label(label, |first| first.is_ascii_alphanumeric()))
}

/// A DNS label: 1 to 63 lowercase alphanumerics or `-`, ending alphanumeric
/// and starting with a byte `starts` accepts.
fn is_dns_label(text: &str, starts: impl Fn(u8) -> bool) -> bool {
    let bytes = text.as_bytes();
    let (Some(&first), Some(&last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    bytes.len() <= MAX_LABEL_LEN
        && starts(first)
        && (last.is_ascii_lowercase() || last.is_ascii_digit())
        && bytes
            .iter()
            .all(|&byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
#[path = "tests/group_version.rs"]
mod tests;

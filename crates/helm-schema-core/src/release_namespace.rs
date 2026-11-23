//! The namespace language a release installs into (ADR 0072): one Kubernetes RFC 1123 label.
//!
//! Helm does not check the namespace; the cluster does, and a release that cannot be installed is
//! outside what the schema speaks for. The interpreter's initial `.Release.Namespace`, the emitted
//! schema's recorded assumption and the acceptance oracle all read this one definition.

/// Full-match pattern of the language, without anchors.
pub const RELEASE_NAMESPACE_PATTERN: &str = "[a-z0-9]([-a-z0-9]*[a-z0-9])?";

/// Fewest bytes of a namespace.
pub const RELEASE_NAMESPACE_MIN_BYTES: usize = 1;

/// Most bytes of a namespace.
pub const RELEASE_NAMESPACE_MAX_BYTES: usize = 63;

/// Whether `text` is a namespace a release can be installed into.
#[must_use]
pub fn is_release_namespace(text: &str) -> bool {
    (RELEASE_NAMESPACE_MIN_BYTES..=RELEASE_NAMESPACE_MAX_BYTES).contains(&text.len())
        && regex::Regex::new(&format!("^(?:{RELEASE_NAMESPACE_PATTERN})$"))
            .is_ok_and(|language| language.is_match(text))
}

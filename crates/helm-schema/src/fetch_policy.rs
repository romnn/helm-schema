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

    /// Policy for local-file-only preparation.
    #[must_use]
    pub const fn local_files_only() -> Self {
        Self::new(true, false)
    }

    /// Policy that rejects all external retrieval.
    #[must_use]
    pub const fn deny_all() -> Self {
        Self::new(false, false)
    }

    #[must_use]
    pub const fn allows_file(self) -> bool {
        self.allow_file
    }

    #[must_use]
    pub const fn allows_network(self) -> bool {
        self.allow_network
    }
}

use std::fmt;

/// Error kinds returned by an [`HttpFetcher`].
#[derive(Debug)]
pub enum FetchError {
    /// Network was disabled by configuration (`allow_download = false`).
    NetworkDisabled,
    /// Transport-level failure: TCP, TLS, DNS, body read, etc.
    Transport(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::NetworkDisabled => f.write_str("network disabled"),
            FetchError::Transport(s) => write!(f, "transport error: {s}"),
        }
    }
}

impl std::error::Error for FetchError {}

/// Trait abstraction over HTTP fetches so providers can be tested without
/// touching the network. Production code wires `UreqFetcher`.
pub trait HttpFetcher: Send + Sync + fmt::Debug {
    /// Fetch the URL.
    ///
    /// Returns:
    /// - `Ok(Some(bytes))` on HTTP 200.
    /// - `Ok(None)` on HTTP 404 (so providers can distinguish a definitive
    ///   "not present" from a transport failure).
    /// - `Err(FetchError::NetworkDisabled)` when the fetcher was configured
    ///   to refuse network access.
    /// - `Err(FetchError::Transport(_))` for any other transport-level
    ///   failure (TCP/TLS/DNS, non-200/404 status, body read failure).
    ///
    /// # Errors
    ///
    /// See variants above.
    fn fetch(&self, url: &str) -> Result<Option<Vec<u8>>, FetchError>;
}

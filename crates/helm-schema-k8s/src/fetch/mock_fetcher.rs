use std::collections::HashMap;
use std::sync::Mutex;

use super::http_fetcher::{FetchError, HttpFetcher};

/// Response a [`MockFetcher`] returns for a given URL.
#[derive(Debug, Clone)]
pub enum MockResponse {
    /// HTTP 200 with these bytes.
    Body(Vec<u8>),
    /// HTTP 404 — definitively not present.
    NotFound,
    /// Transport error.
    Transport(String),
    /// Network was disabled.
    NetworkDisabled,
}

/// In-memory [`HttpFetcher`] for unit + integration tests.
///
/// URLs without an explicit response default to [`MockResponse::NotFound`]
/// so tests can pre-seed only the URLs they care about and let everything
/// else 404 naturally.
#[derive(Debug)]
pub struct MockFetcher {
    responses: Mutex<HashMap<String, MockResponse>>,
    call_counts: Mutex<HashMap<String, usize>>,
    default: Mutex<MockResponse>,
}

impl Default for MockFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl MockFetcher {
    #[must_use]
    pub fn new() -> Self {
        Self {
            responses: Mutex::new(HashMap::new()),
            call_counts: Mutex::new(HashMap::new()),
            default: Mutex::new(MockResponse::NotFound),
        }
    }

    /// Wire a canned response for an exact URL.
    #[must_use]
    pub fn with(self, url: impl Into<String>, response: MockResponse) -> Self {
        if let Ok(mut guard) = self.responses.lock() {
            guard.insert(url.into(), response);
        }
        self
    }

    /// Wire a canned body for an exact URL (convenience for HTTP 200).
    #[must_use]
    pub fn with_body(self, url: impl Into<String>, body: impl Into<Vec<u8>>) -> Self {
        self.with(url, MockResponse::Body(body.into()))
    }

    /// Change the default response for any URL not explicitly wired
    /// (default: 404).
    #[must_use]
    pub fn with_default(self, response: MockResponse) -> Self {
        if let Ok(mut guard) = self.default.lock() {
            *guard = response;
        }
        self
    }

    /// Count of `fetch` invocations across all URLs.
    #[must_use]
    pub fn total_calls(&self) -> usize {
        self.call_counts
            .lock()
            .map(|g| g.values().sum())
            .unwrap_or(0)
    }

    /// Count of `fetch` invocations for a specific URL.
    #[must_use]
    pub fn calls_for(&self, url: &str) -> usize {
        self.call_counts
            .lock()
            .map(|g| g.get(url).copied().unwrap_or(0))
            .unwrap_or(0)
    }
}

impl HttpFetcher for MockFetcher {
    fn fetch(&self, url: &str) -> Result<Option<Vec<u8>>, FetchError> {
        if let Ok(mut counts) = self.call_counts.lock() {
            *counts.entry(url.to_string()).or_default() += 1;
        }
        let response = self
            .responses
            .lock()
            .ok()
            .and_then(|g| g.get(url).cloned())
            .or_else(|| self.default.lock().ok().map(|g| g.clone()))
            .unwrap_or(MockResponse::NotFound);
        match response {
            MockResponse::Body(b) => Ok(Some(b)),
            MockResponse::NotFound => Ok(None),
            MockResponse::Transport(s) => Err(FetchError::Transport(s)),
            MockResponse::NetworkDisabled => Err(FetchError::NetworkDisabled),
        }
    }
}

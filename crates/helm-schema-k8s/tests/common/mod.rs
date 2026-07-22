use std::collections::HashMap;
use std::sync::Mutex;

use helm_schema_k8s::{FetchError, HttpFetcher};

/// Scripted result returned by the mock HTTP transport.
#[derive(Debug, Clone)]
pub enum MockResponse {
    /// Successful response with raw body bytes.
    Body(Vec<u8>),
    /// Authoritative HTTP not-found response.
    NotFound,
    /// Transport-layer failure with a diagnostic message.
    Transport(String),
    /// Fetch rejected because network access is disabled.
    NetworkDisabled,
}

/// Deterministic URL-keyed HTTP transport used by provider integration tests.
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
    /// Creates a mock that returns not-found for unconfigured URLs.
    #[must_use]
    pub fn new() -> Self {
        Self {
            responses: Mutex::new(HashMap::new()),
            call_counts: Mutex::new(HashMap::new()),
            default: Mutex::new(MockResponse::NotFound),
        }
    }

    /// Configures one URL response.
    #[must_use]
    pub fn with(self, url: impl Into<String>, response: MockResponse) -> Self {
        if let Ok(mut guard) = self.responses.lock() {
            guard.insert(url.into(), response);
        }
        self
    }

    /// Configures one successful URL response body.
    #[must_use]
    pub fn with_body(self, url: impl Into<String>, body: impl Into<Vec<u8>>) -> Self {
        self.with(url, MockResponse::Body(body.into()))
    }

    /// Changes the response used for unconfigured URLs.
    #[must_use]
    pub fn with_default(self, response: MockResponse) -> Self {
        if let Ok(mut guard) = self.default.lock() {
            *guard = response;
        }
        self
    }

    /// Returns the number of fetches across all URLs.
    #[must_use]
    pub fn total_calls(&self) -> usize {
        self.call_counts
            .lock()
            .map(|g| g.values().sum())
            .unwrap_or(0)
    }

    /// Returns the number of fetches for one URL.
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

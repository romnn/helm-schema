use std::io::Read;

use super::http_fetcher::{FetchError, HttpFetcher};

/// Production [`HttpFetcher`] backed by [`ureq`].
#[derive(Debug, Default)]
pub struct UreqFetcher;

impl UreqFetcher {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl HttpFetcher for UreqFetcher {
    fn fetch(&self, url: &str) -> Result<Option<Vec<u8>>, FetchError> {
        match ureq::get(url).call() {
            Ok(resp) => {
                let mut reader = resp.into_body().into_reader();
                let mut buf = Vec::new();
                reader
                    .read_to_end(&mut buf)
                    .map_err(|err| FetchError::Transport(err.to_string()))?;
                Ok(Some(buf))
            }
            Err(ureq::Error::StatusCode(404)) => Ok(None),
            Err(err) => Err(FetchError::Transport(err.to_string())),
        }
    }
}

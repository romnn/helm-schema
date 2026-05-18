mod http_fetcher;
mod mock_fetcher;
mod ureq_fetcher;

pub use http_fetcher::{FetchError, HttpFetcher};
pub use mock_fetcher::{MockFetcher, MockResponse};
pub use ureq_fetcher::UreqFetcher;

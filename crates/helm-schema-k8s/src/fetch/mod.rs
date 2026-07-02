mod http_fetcher;
mod ureq_fetcher;

pub use http_fetcher::{FetchError, HttpFetcher};
pub(crate) use ureq_fetcher::UreqFetcher;

//! Perfetto output for chart generation and verification harnesses.

use std::fs::File;
use std::sync::Mutex;

use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt as _;

/// Creates a dispatcher writing helm-schema spans and fields to `file`.
///
/// The caller chooses a scoped or process-wide subscriber lifetime.
/// Worker threads require a process-wide subscriber or explicit propagation.
#[must_use]
pub fn perfetto_dispatch(file: File) -> tracing::Dispatch {
    let layer = tracing_perfetto::PerfettoLayer::new(Mutex::new(file))
        .with_debug_annotations(true)
        .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
            metadata.target().starts_with("helm_schema")
        }));
    tracing::Dispatch::new(tracing_subscriber::registry().with(layer))
}

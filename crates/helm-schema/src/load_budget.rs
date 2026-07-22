use std::io::Read;

use crate::error::{CliError, EngineResult};

/// Byte budgets for input assembly and local preparation work.
///
/// These limits are intentionally generous defaults. They exist to make
/// archive extraction and external schema ingestion bounded by policy instead
/// of accidentally unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_field_names,
    reason = "the shared max prefix makes each public budget field's bound explicit"
)]
pub struct LoadBudget {
    /// Maximum compressed chart archive size.
    pub max_chart_archive_bytes: usize,
    /// Maximum size of one loaded JSON Schema document.
    pub max_schema_document_bytes: usize,
    /// Maximum number of members accepted in a chart archive.
    pub max_chart_archive_entries: usize,
    /// Maximum aggregate uncompressed size of chart archive members.
    pub max_chart_archive_unpacked_bytes: usize,
}

impl LoadBudget {
    /// Creates a budget with explicit document limits and conservative archive defaults.
    #[must_use]
    pub const fn new(max_chart_archive_bytes: usize, max_schema_document_bytes: usize) -> Self {
        Self {
            max_chart_archive_bytes,
            max_schema_document_bytes,
            max_chart_archive_entries: 4096,
            max_chart_archive_unpacked_bytes: 256 * 1024 * 1024,
        }
    }
}

impl Default for LoadBudget {
    fn default() -> Self {
        Self::new(64 * 1024 * 1024, 16 * 1024 * 1024)
    }
}

pub(crate) fn read_to_end_capped(
    reader: &mut impl Read,
    limit_bytes: usize,
    subject: impl Into<String>,
) -> EngineResult<Vec<u8>> {
    let subject = subject.into();
    let limit_plus_one = u64::try_from(limit_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut limited = reader.take(limit_plus_one);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes)?;
    if bytes.len() > limit_bytes {
        return Err(CliError::LoadBudgetExceeded {
            subject,
            limit_bytes,
        });
    }
    Ok(bytes)
}

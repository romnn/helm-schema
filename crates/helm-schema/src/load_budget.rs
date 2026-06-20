use std::io::Read;

use crate::error::{CliError, CliResult};

/// Byte budgets for input assembly and local preparation work.
///
/// These limits are intentionally generous defaults. They exist to make
/// archive extraction and external schema ingestion bounded by policy instead
/// of accidentally unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadBudget {
    pub max_chart_archive_bytes: usize,
    pub max_schema_document_bytes: usize,
    pub max_chart_archive_entries: usize,
    pub max_chart_archive_unpacked_bytes: usize,
}

impl LoadBudget {
    #[must_use]
    pub const fn new(max_chart_archive_bytes: usize, max_schema_document_bytes: usize) -> Self {
        Self {
            max_chart_archive_bytes,
            max_schema_document_bytes,
            max_chart_archive_entries: 4096,
            max_chart_archive_unpacked_bytes: 256 * 1024 * 1024,
        }
    }

    #[must_use]
    pub const fn with_chart_archive_limits(
        mut self,
        max_chart_archive_entries: usize,
        max_chart_archive_unpacked_bytes: usize,
    ) -> Self {
        self.max_chart_archive_entries = max_chart_archive_entries;
        self.max_chart_archive_unpacked_bytes = max_chart_archive_unpacked_bytes;
        self
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
) -> CliResult<Vec<u8>> {
    let subject = subject.into();
    let limit_plus_one = limit_bytes.saturating_add(1).min(u64::MAX as usize) as u64;
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

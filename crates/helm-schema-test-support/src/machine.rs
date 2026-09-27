//! Default concurrency for the heavy test tooling, sized to the host.
//!
//! Each default derives from the core count and total memory.
//! The formulas reproduce the fixed values the tooling used on an 11-core,
//! 18 GiB laptop and scale them with the machine; explicit settings such as
//! `SCHEMA_HELM_WORKERS` or `corpus_generation --jobs` still override them.
//! The values change only speed, never results.

use std::num::NonZero;

const GIB: u64 = 1024 * 1024 * 1024;

/// The cores and memory available to this process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Machine {
    /// Logical cores this process may use, respecting CPU quotas.
    pub cores: usize,
    /// Physical memory in bytes, capped by a cgroup memory limit when one applies.
    pub memory_bytes: u64,
}

impl Machine {
    /// Measures the host.
    #[must_use]
    pub fn detect() -> Self {
        let cores = std::thread::available_parallelism().map_or(1, NonZero::get);
        let mut system = sysinfo::System::new();
        system.refresh_memory_specifics(sysinfo::MemoryRefreshKind::nothing().with_ram());
        let mut memory_bytes = system.total_memory();
        if let Some(limits) = system.cgroup_limits() {
            memory_bytes = memory_bytes.min(limits.total_memory);
        }
        Self {
            cores,
            memory_bytes,
        }
    }

    /// Concurrent chart generations for the corpus producer.
    ///
    /// The largest charts set a peak of about 5 GiB, and each further job adds
    /// about 0.7 GiB (measured: 7.4 GiB at 4 jobs, 15.5 GiB at 16).
    /// One job per 4.5 GiB of memory keeps that peak under half of memory.
    #[must_use]
    pub fn corpus_jobs(&self) -> usize {
        let by_memory = self.memory_bytes / (GIB * 9 / 2);
        usize::try_from(by_memory)
            .unwrap_or(usize::MAX)
            .clamp(1, self.spare_cores())
    }

    /// The memory budget for concurrently admitted Helm renders: a third of
    /// memory, leaving the rest to the test process and the host.
    #[must_use]
    pub fn helm_memory_bytes(&self) -> u64 {
        self.memory_bytes / 3
    }

    /// Helm render workers: one per GiB of [`Self::helm_memory_bytes`].
    #[must_use]
    pub fn helm_workers(&self) -> usize {
        let by_memory = self.helm_memory_bytes() / GIB;
        usize::try_from(by_memory)
            .unwrap_or(usize::MAX)
            .clamp(1, self.spare_cores())
    }

    /// The cores left after two stay free for the host and the coordinating
    /// process.
    fn spare_cores(&self) -> usize {
        self.cores.saturating_sub(2).max(1)
    }
}

//! Runs ordered jobs on a fixed set of worker threads under a worker and a
//! memory bound.
//!
//! Every job carries an ordinal assigned before dispatch. Among the queued
//! jobs the least ordinal is admitted first, so no queued job starves; a
//! job may submit further jobs while it runs. Results are returned in
//! ordinal order, so completion order never shows in them. A panicking job
//! releases its slot and the panic propagates once the others finish.

use std::collections::BTreeMap;
use std::sync::{Condvar, Mutex, PoisonError};

use color_eyre::eyre::{self, WrapErr as _};

/// Overrides the host-sized Helm worker count.
const WORKERS_VAR: &str = "SCHEMA_HELM_WORKERS";
/// Overrides the host-sized Helm memory budget, in MiB.
const MEMORY_MIB_VAR: &str = "SCHEMA_HELM_MEMORY_MIB";

/// A job's place in the result order: its chart, then its probe.
pub(crate) type Ordinal = (usize, usize);

/// How much work may run at once.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PoolLimits {
    pub(crate) workers: usize,
    /// The sum of admitted reservations, in bytes. A job reserving more than
    /// this runs alone.
    pub(crate) memory_bytes: u64,
}

impl PoolLimits {
    /// `SCHEMA_HELM_WORKERS` workers, at most two fewer than the available
    /// cores and at least one, sharing `SCHEMA_HELM_MEMORY_MIB` mebibytes of
    /// child memory.
    /// Both default to the host's size (see
    /// [`helm_schema_test_support::machine::Machine`]).
    ///
    /// # Errors
    ///
    /// Returns an error when either variable is not a number.
    pub(crate) fn from_env() -> eyre::Result<Self> {
        let machine = helm_schema_test_support::machine::Machine::detect();
        let workers: usize = match std::env::var(WORKERS_VAR) {
            Ok(workers) => workers.parse().wrap_err(WORKERS_VAR)?,
            Err(_) => machine.helm_workers(),
        };
        let memory_bytes: u64 = match std::env::var(MEMORY_MIB_VAR) {
            Ok(memory) => memory.parse::<u64>().wrap_err(MEMORY_MIB_VAR)? * 1024 * 1024,
            Err(_) => machine.helm_memory_bytes(),
        };
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        Ok(Self {
            workers: workers.min(cores.saturating_sub(2)).max(1),
            memory_bytes,
        })
    }
}

/// The most the pool ever ran at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub(crate) struct PoolPeaks {
    pub(crate) running: usize,
    pub(crate) reserved_bytes: u64,
}

struct State<J> {
    queued: BTreeMap<Ordinal, J>,
    running: usize,
    reserved: u64,
    peaks: PoolPeaks,
}

/// Lets a running job submit further jobs.
pub(crate) struct Submitter<'a, J> {
    state: &'a Mutex<State<J>>,
    changed: &'a Condvar,
}

impl<J> Submitter<'_, J> {
    pub(crate) fn submit(&self, ordinal: Ordinal, job: J) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .queued
            .insert(ordinal, job);
        self.changed.notify_all();
    }
}

/// Runs `jobs` and every job they submit with `work`, which receives each
/// job with its ordinal. The job with the least ordinal is admitted once a
/// worker is free and its `reservation` fits beside the admitted ones.
/// Returns the results in ordinal order.
pub(crate) fn run_ordered<J, R>(
    limits: PoolLimits,
    jobs: Vec<(Ordinal, J)>,
    reservation: impl Fn(&J) -> u64 + Sync,
    work: impl Fn(Ordinal, J, &Submitter<'_, J>) -> R + Sync,
) -> (Vec<(Ordinal, R)>, PoolPeaks)
where
    J: Send,
    R: Send,
{
    let state = Mutex::new(State {
        queued: jobs.into_iter().collect(),
        running: 0,
        reserved: 0,
        peaks: PoolPeaks::default(),
    });
    let changed = Condvar::new();
    let results = Mutex::new(BTreeMap::new());
    std::thread::scope(|scope| {
        for _ in 0..limits.workers.max(1) {
            scope.spawn(|| {
                let submitter = Submitter {
                    state: &state,
                    changed: &changed,
                };
                while let Some((ordinal, job, reserved)) =
                    next_job(&state, &changed, limits, &reservation)
                {
                    // Releases the job's slot even when `work` unwinds, so
                    // the other workers drain and the panic propagates.
                    let _release = Release {
                        state: &state,
                        changed: &changed,
                        reserved,
                    };
                    let result = work(ordinal, job, &submitter);
                    results
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(ordinal, result);
                }
            });
        }
    });
    let peaks = state
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .peaks;
    let results = results
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .into_iter()
        .collect();
    (results, peaks)
}

/// Returns a running job's worker and memory reservation to the pool.
struct Release<'a, J> {
    state: &'a Mutex<State<J>>,
    changed: &'a Condvar,
    reserved: u64,
}

impl<J> Drop for Release<'_, J> {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.running -= 1;
        state.reserved -= self.reserved;
        self.changed.notify_all();
    }
}

/// Waits until the least queued job may start and takes it, or returns
/// `None` once nothing is queued or running.
fn next_job<J>(
    state: &Mutex<State<J>>,
    changed: &Condvar,
    limits: PoolLimits,
    reservation: &impl Fn(&J) -> u64,
) -> Option<(Ordinal, J, u64)> {
    let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
    loop {
        let admissible = match state.queued.first_key_value() {
            None if state.running == 0 => {
                changed.notify_all();
                return None;
            }
            None => None,
            Some((_, job)) => {
                let reserved = reservation(job);
                let fits = state.reserved.saturating_add(reserved) <= limits.memory_bytes;
                (state.running < limits.workers && (state.running == 0 || fits)).then_some(reserved)
            }
        };
        if let Some(reserved) = admissible
            && let Some((ordinal, job)) = state.queued.pop_first()
        {
            state.running += 1;
            state.reserved += reserved;
            state.peaks.running = state.peaks.running.max(state.running);
            state.peaks.reserved_bytes = state.peaks.reserved_bytes.max(state.reserved);
            return Some((ordinal, job, reserved));
        }
        state = changed.wait(state).unwrap_or_else(PoisonError::into_inner);
    }
}

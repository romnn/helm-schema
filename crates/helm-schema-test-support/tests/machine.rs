//! Host-sized concurrency defaults: calibrated on the reference laptop and
//! scaled with memory and cores elsewhere.

use helm_schema_test_support::machine::Machine;
use test_util::prelude::sim_assert_eq;

const GIB: u64 = 1024 * 1024 * 1024;

/// The 11-core, 18 GiB laptop the fixed defaults were tuned on keeps exactly
/// those values: 4 producer jobs, 6 Helm workers, and a 6144 MiB Helm budget.
#[test]
fn the_reference_laptop_keeps_its_tuned_defaults() {
    let laptop = Machine {
        cores: 11,
        memory_bytes: 18 * GIB,
    };
    sim_assert_eq!(have: laptop.corpus_jobs(), want: 4);
    sim_assert_eq!(have: laptop.helm_workers(), want: 6);
    sim_assert_eq!(have: laptop.helm_memory_bytes(), want: 6144 * 1024 * 1024);
}

/// A larger host scales with its memory while leaving two cores free.
#[test]
fn a_larger_host_scales_with_memory() {
    let workstation = Machine {
        cores: 48,
        memory_bytes: 64 * GIB,
    };
    sim_assert_eq!(have: workstation.corpus_jobs(), want: 14);
    sim_assert_eq!(have: workstation.helm_workers(), want: 21);

    // Plenty of memory but few cores: the core count caps both.
    let narrow = Machine {
        cores: 6,
        memory_bytes: 256 * GIB,
    };
    sim_assert_eq!(have: narrow.corpus_jobs(), want: 4);
    sim_assert_eq!(have: narrow.helm_workers(), want: 4);
}

/// A tiny host still runs one job and one worker.
#[test]
fn a_tiny_host_runs_one_at_a_time() {
    let tiny = Machine {
        cores: 1,
        memory_bytes: 2 * GIB,
    };
    sim_assert_eq!(have: tiny.corpus_jobs(), want: 1);
    sim_assert_eq!(have: tiny.helm_workers(), want: 1);
}

#[test]
fn detection_reports_a_usable_host() {
    let host = Machine::detect();
    assert!(host.cores >= 1, "{host:?}");
    assert!(host.memory_bytes > 0, "{host:?}");
}

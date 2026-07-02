use super::*;
use test_util::prelude::sim_assert_eq;

#[test]
fn source_id_is_stable() {
    let url = "https://example.com/mirror/";
    let a = source_id_for_url(url);
    let b = source_id_for_url(url);
    sim_assert_eq!(have: a, want: b);
    sim_assert_eq!(have: a.len(), want: 12);
}

#[test]
fn source_id_differs_per_url() {
    let a = source_id_for_url("https://example.com/mirror-a/");
    let b = source_id_for_url("https://example.com/mirror-b/");
    assert_ne!(a, b);
}

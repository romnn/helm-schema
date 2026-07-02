use super::*;

#[test]
fn record_then_contains() {
    let nc = NegativeCache::new();
    assert!(!nc.contains("default", "v1.35.0", "foo.json"));
    nc.record("default", "v1.35.0", "foo.json");
    assert!(nc.contains("default", "v1.35.0", "foo.json"));
    assert!(!nc.contains("mirror-x", "v1.35.0", "foo.json"));
    assert!(!nc.contains("default", "v1.24.0", "foo.json"));
}

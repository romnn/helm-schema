use super::*;
use test_util::prelude::sim_assert_eq;

#[test]
fn explicit_only_preserves_order() {
    let chain = K8sVersionChain::new(vec!["v1.24.0".into(), "v1.35.0".into()], None);
    sim_assert_eq!(have: chain.ordered(), want: vec!["v1.24.0", "v1.35.0"]);
}

#[test]
fn auto_fallback_window_appends_descending() {
    let chain = K8sVersionChain::new(vec!["v1.35.0".into()], Some(5));
    sim_assert_eq!(
        have: chain.ordered(),
        want: vec![
            "v1.35.0", "v1.34.0", "v1.33.0", "v1.32.0", "v1.31.0", "v1.30.0",
        ]
    );
}

#[test]
fn auto_fallback_no_op_with_multi_explicit() {
    let chain = K8sVersionChain::new(vec!["v1.35.0".into(), "v1.24.0".into()], Some(5));
    sim_assert_eq!(have: chain.ordered(), want: vec!["v1.35.0", "v1.24.0"]);
}

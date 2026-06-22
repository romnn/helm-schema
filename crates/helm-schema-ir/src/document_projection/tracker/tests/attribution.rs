use super::placeholder_token;
use test_util::prelude::sim_assert_eq;

#[test]
fn short_placeholder_tokens_remain_distinct_for_dense_inline_actions() {
    let tokens = (0..36)
        .map(|index| placeholder_token(index, 5))
        .collect::<std::collections::BTreeSet<_>>();

    sim_assert_eq!(have: tokens.len(), want: 36);
    assert!(tokens.iter().all(|token| token.starts_with("__HS")));
}

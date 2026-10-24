use helm_schema_core::{GuardValue, ValuesPath};
use test_util::prelude::sim_assert_eq;

use crate::scalar_value::ScalarValueDispatch;

#[test]
fn legacy_empty_dispatch_keeps_a_present_content_view() {
    let stringified = ScalarValueDispatch::identity(ValuesPath::parse("text"))
        .length()
        .stringified();
    let trimmed = ScalarValueDispatch::constant(GuardValue::Bool(false)).trimmed("prefix", true);

    // An empty projection still suppresses fragment fallback at legacy consumers.
    for dispatch in [stringified, trimmed] {
        sim_assert_eq!(
            have: dispatch.content().map(|content| (content.arms.len(), content.complete)),
            want: Some((0, false)),
        );
    }
}

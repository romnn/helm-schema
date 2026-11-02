use test_util::prelude::sim_assert_eq;

#[test]
fn unsupported_constraint_shapes_abstain() {
    let unsupported = [
        "",
        "*",
        "1.2.3",
        "=1.2.3",
        "!=1.2.3",
        "~1.2",
        "^1.2",
        ">1.33-0",
        "<=1.33-0",
        ">=1.33-1",
        ">=1.33-alpha",
        "<1.33-10",
        "<1.33-alpha",
        "<0.0.0-0",
        ">=1.2 <2.0",
        ">=1.2, <2",
        ">=1.2 || >=3",
        "<3.0.0.0",
        "<18446744073709551616.0.0",
        "<x.y.z",
        "<1.2.x",
        "<0.0.0",
    ];
    for constraint in unsupported {
        sim_assert_eq!(have: super::NumericConstraint::parse(constraint).is_none(), want: true, "{constraint}");
    }
}

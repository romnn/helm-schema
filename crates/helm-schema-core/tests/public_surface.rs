use helm_schema_core::{ApiPresenceQuery, join_value_path, split_value_path};
use test_util::prelude::sim_assert_eq;

#[test]
fn api_presence_query_parses_resource_and_group_version_literals() {
    sim_assert_eq!(
        have: ApiPresenceQuery::parse_helm_literal("policy/v1/PodDisruptionBudget"),
        want: Some(ApiPresenceQuery::Resource {
            api_version: "policy/v1".to_string(),
            kind: "PodDisruptionBudget".to_string(),
        })
    );
    sim_assert_eq!(
        have: ApiPresenceQuery::parse_helm_literal("monitoring.coreos.com/v1"),
        want: Some(ApiPresenceQuery::GroupVersion {
            api_version: "monitoring.coreos.com/v1".to_string(),
        })
    );
}

#[test]
fn value_path_currency_preserves_literal_dots_and_backslashes() {
    let segments = ["grafana.ini", "paths", r"data\root"];
    let path = join_value_path(segments);

    sim_assert_eq!(have: &path, want: r"grafana\.ini.paths.data\\root");
    sim_assert_eq!(
        have: split_value_path(&path),
        want: segments.map(str::to_string).to_vec()
    );
}

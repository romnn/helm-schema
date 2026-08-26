use test_util::prelude::sim_assert_eq;

use super::*;

struct UnknownCapabilities;

impl CapabilityOracle for UnknownCapabilities {
    fn capability_has_query(&self, _query: &helm_schema_core::ApiPresenceQuery) -> Option<bool> {
        None
    }
}

fn resource(api_version: &str) -> ResourceRef {
    ResourceRef::concrete(api_version.to_string(), "Widget".to_string())
}

#[test]
fn local_override_unreadable_preempts_generic_missing_schema() {
    let attempted = resource("example.com/v1");
    let expected = Diagnostic::LocalOverrideUnreadable {
        kind: "Widget".to_string(),
        api_version: "example.com/v1".to_string(),
        override_path: "/tmp/widget.schema.json".to_string(),
        io_error: "permission denied".to_string(),
    };
    let diagnostics = MissingLookupDiagnostics::new(&[], &UnknownCapabilities)
        .project(&attempted, Some(expected.clone()));

    sim_assert_eq!(
        have: diagnostics,
        want: vec![expected]
    );
}

use test_util::prelude::sim_assert_eq;

use super::*;
use crate::lookup::ProviderLookupResult;

fn resource(api_version: &str) -> ResourceRef {
    ResourceRef {
        api_version: api_version.to_string(),
        kind: "Widget".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    }
}

#[test]
fn local_override_unreadable_uses_attempted_resource_from_trace_entry() {
    let attempted = resource("example.com/v1");
    let mut trace = LookupTrace::default();
    trace.record_provider(
        &attempted,
        ProviderOrigin::LocalOverride,
        &ProviderLookupResult::ResourceDocMissing {
            source_path: "/tmp/widget.schema.json".to_string(),
            io_error: "permission denied".to_string(),
        },
    );

    let diagnostic = local_override_unreadable(&trace).expect("diagnostic");

    sim_assert_eq!(
        have: diagnostic,
        want: Diagnostic::LocalOverrideUnreadable {
            kind: "Widget".to_string(),
            api_version: "example.com/v1".to_string(),
            override_path: "/tmp/widget.schema.json".to_string(),
            io_error: "permission denied".to_string(),
        }
    );
}

use test_util::prelude::sim_assert_eq;

use super::{DocumentSiteContext, ObservedOutputSite};
use crate::{SourceSpan, ValueKind, YamlPath};

#[test]
fn fragment_output_site_suppresses_mapping_keys() {
    let site = DocumentSiteContext {
        kind: ValueKind::Scalar,
        in_mapping_key: true,
        in_yaml_comment: false,
        entire_scalar_value: true,
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        resource: None,
        source_span: SourceSpan::new(0, 0),
    };

    sim_assert_eq!(have: site.fragment_output_site(), want: None);
}

#[test]
fn fragment_output_site_marks_partial_scalar_slots() {
    let site = DocumentSiteContext {
        kind: ValueKind::Scalar,
        in_mapping_key: false,
        in_yaml_comment: false,
        entire_scalar_value: false,
        path: YamlPath(vec!["spec".to_string(), "value".to_string()]),
        resource: None,
        source_span: SourceSpan::new(0, 0),
    };

    sim_assert_eq!(
        have: site.fragment_output_site(),
        want: Some(ObservedOutputSite {
            kind: ValueKind::PartialScalar,
            path: YamlPath(vec!["spec".to_string(), "value".to_string()]),
        })
    );
}

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{ApiPresenceQuery, CapabilityGuard};
use test_util::prelude::sim_assert_eq;

use super::decode_header_guard;
use crate::TemplateHeader;

/// The typed header of the condition in `{{ if <condition> }}`.
fn if_header(condition: &str) -> eyre::Result<TemplateHeader> {
    let source = format!("{{{{ if {condition} }}}}{{{{ end }}}}");
    let tree = crate::parse_go_template(&source).ok_or_eyre("tree-sitter parse")?;
    let mut cursor = tree.root_node().walk();
    let action = tree
        .root_node()
        .named_children(&mut cursor)
        .find(|node| node.kind() == "if_action")
        .ok_or_eyre("if action")?;
    let node = action
        .child_by_field_name("condition")
        .ok_or_eyre("condition")?;
    Ok(TemplateHeader::from_node(node, &source))
}

#[test]
fn decode_header_guard_recognises_capability_has() -> eyre::Result<()> {
    sim_assert_eq!(
        have: decode_header_guard(&if_header(".Capabilities.APIVersions.Has \"policy/v1\"")?),
        want: CapabilityGuard::Has {
            api: "policy/v1".to_string(),
        }
    );
    sim_assert_eq!(
        have: decode_header_guard(&if_header(
            "$.Capabilities.APIVersions.Has \"networking.k8s.io/v1/Ingress\""
        )?),
        want: CapabilityGuard::Has {
            api: "networking.k8s.io/v1/Ingress".to_string(),
        }
    );
    Ok(())
}

#[test]
fn decode_header_guard_recognises_negated_capability_has() -> eyre::Result<()> {
    sim_assert_eq!(
        have: decode_header_guard(&if_header(
            "not .Capabilities.APIVersions.Has \"extensions/v1beta1\""
        )?),
        want: CapabilityGuard::NotHas {
            api: "extensions/v1beta1".to_string(),
        }
    );
    Ok(())
}

#[test]
fn decode_header_guard_falls_back_to_opaque_for_values_refs() -> eyre::Result<()> {
    sim_assert_eq!(
        have: decode_header_guard(&if_header("$.Values.podDisruptionBudget.apiVersion")?),
        want: CapabilityGuard::Opaque {
            text: "$.Values.podDisruptionBudget.apiVersion".to_string(),
        }
    );
    Ok(())
}

#[test]
fn core_query_parser_understands_resource_literals() {
    sim_assert_eq!(
        have: ApiPresenceQuery::parse_helm_literal("policy/v1/PodDisruptionBudget"),
        want: Some(ApiPresenceQuery::Resource {
            api_version: "policy/v1".to_string(),
            kind: "PodDisruptionBudget".to_string(),
        })
    );
}

use color_eyre::eyre::{self, OptionExt as _};
use test_util::prelude::sim_assert_eq;

use crate::parse_yaml;

fn sexp(text: &str) -> eyre::Result<String> {
    Ok(parse_yaml(text)
        .ok_or_eyre("tree-sitter-yaml parse")?
        .root_node()
        .to_sexp())
}

/// tree-sitter-yaml 0.7.2 was developed against tree-sitter 0.25; the
/// workspace runs 0.27. Loading its language and parsing a mapping proves
/// the runtime accepts the grammar's ABI.
#[test]
fn tree_sitter_yaml_parses_on_the_workspace_runtime() -> eyre::Result<()> {
    sim_assert_eq!(have: sexp("a: 1\n")?, want: "(stream (document (block_node (block_mapping (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (plain_scalar (integer_scalar))))))))");
    Ok(())
}

/// Helm 4.2.3 reads YAML 1.1 (`phase2/helm/dialect.log`: `fromYaml` turns
/// `y`, `yes`, `on` into `true`, `010` into `8`, a `y:` key into `"true"`,
/// honours `!!str` and expands `<<` merges). The grammar resolves the same
/// scalars with the YAML 1.2 core schema, so its plain-scalar kinds disagree
/// with Helm and must never type a value: these trees pin that disagreement.
/// The grammar also leaves `!!str 1` an integer under its tag and the `<<`
/// merge an ordinary key over an alias, so tags and aliases are left
/// unresolved and must make a layout abstain.
#[test]
fn grammar_scalar_kinds_follow_yaml_1_2_not_helm() -> eyre::Result<()> {
    sim_assert_eq!(
        have: sexp("y: y\nyes: yes\non: on\noctal: 010\n")?,
        want: "(stream (document (block_node (block_mapping (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (plain_scalar (string_scalar)))) (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (plain_scalar (string_scalar)))) (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (plain_scalar (string_scalar)))) (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (plain_scalar (integer_scalar))))))))"
    );
    sim_assert_eq!(
        have: sexp("s: !!str 1\nb: &x {c: 1}\nd:\n  <<: *x\n")?,
        want: "(stream (document (block_node (block_mapping (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (tag) (plain_scalar (integer_scalar)))) (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (anchor (anchor_name)) (flow_mapping (flow_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (plain_scalar (integer_scalar))))))) (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (block_node (block_mapping (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (alias (alias_name)))))))))))"
    );
    Ok(())
}

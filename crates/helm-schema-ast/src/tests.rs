use crate::{DefineIndex, FusedRustParser, HelmAst, HelmParser, TreeSitterParser};

fn prometheusrule_src() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/bitnami-redis/templates/prometheusrule.yaml"
    );
    std::fs::read_to_string(path).expect("read prometheusrule.yaml")
}

fn helpers_src() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/bitnami-redis/templates/_helpers.tpl"
    );
    std::fs::read_to_string(path).expect("read _helpers.tpl")
}

/// Both parsers must produce structurally equivalent AST for a simple template.
#[test]
fn both_parsers_produce_equivalent_ast_simple() {
    let src = "{{- if .Values.enabled }}\nfoo: bar\n{{- end }}\n";

    let rust_ast = FusedRustParser.parse(src).expect("fused rust parse");
    let ts_ast = TreeSitterParser.parse(src).expect("tree-sitter parse");

    // Both should have an If node with then_branch containing a Mapping with a Pair.
    let rust_if = find_first_if(&rust_ast).expect("rust AST should have If");
    let ts_if = find_first_if(&ts_ast).expect("ts AST should have If");

    // Condition text should match.
    assert_eq!(extract_if_cond(rust_if), extract_if_cond(ts_if));

    // Both then_branches should contain a Mapping with key "foo" and value "bar".
    let rust_then = extract_if_then(rust_if);
    let ts_then = extract_if_then(ts_if);
    assert!(
        has_pair_with_key(rust_then, "foo"),
        "rust: missing key 'foo'"
    );
    assert!(has_pair_with_key(ts_then, "foo"), "ts: missing key 'foo'");
}

/// FusedRustParser can parse the bitnami-redis prometheusrule template.
#[test]
fn fused_rust_parses_prometheusrule() {
    let src = prometheusrule_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    let if_node = find_first_if(&ast).expect("should have If");
    let cond = extract_if_cond(if_node);
    assert!(
        cond.contains(".Values.metrics.enabled"),
        "condition should reference metrics.enabled, got: {cond}"
    );
}

/// TreeSitterParser can parse the bitnami-redis prometheusrule template.
#[test]
fn tree_sitter_parses_prometheusrule() {
    let src = prometheusrule_src();
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let if_node = find_first_if(&ast).expect("should have If");
    let cond = extract_if_cond(if_node);
    assert!(
        cond.contains(".Values.metrics.enabled"),
        "condition should reference metrics.enabled, got: {cond}"
    );
}

/// DefineIndex collects definitions from helpers using both parsers.
#[test]
fn define_index_from_helpers() {
    let helpers = helpers_src();

    let mut idx_rust = DefineIndex::new();
    idx_rust
        .add_source(&FusedRustParser, &helpers)
        .expect("rust define index");
    assert!(
        idx_rust.get("redis.image").is_some(),
        "rust define index should find 'redis.image'"
    );

    let mut idx_ts = DefineIndex::new();
    idx_ts
        .add_source(&TreeSitterParser, &helpers)
        .expect("ts define index");
    assert!(
        idx_ts.get("redis.image").is_some(),
        "ts define index should find 'redis.image'"
    );
}

// --- helpers ---

fn find_first_if(node: &HelmAst) -> Option<&HelmAst> {
    match node {
        HelmAst::If { .. } => Some(node),
        HelmAst::Document { items } | HelmAst::Mapping { items } | HelmAst::Sequence { items } => {
            items.iter().find_map(find_first_if)
        }
        HelmAst::Pair { value, .. } => value.as_ref().and_then(|v| find_first_if(v)),
        _ => None,
    }
}

fn extract_if_cond(node: &HelmAst) -> &str {
    match node {
        HelmAst::If { cond, .. } => cond.as_str(),
        _ => panic!("not an If node"),
    }
}

fn extract_if_then(node: &HelmAst) -> &[HelmAst] {
    match node {
        HelmAst::If { then_branch, .. } => then_branch.as_slice(),
        _ => panic!("not an If node"),
    }
}

fn has_pair_with_key(items: &[HelmAst], key: &str) -> bool {
    for item in items {
        match item {
            HelmAst::Pair { key: k, .. } => {
                if let HelmAst::Scalar { text } = k.as_ref() {
                    if text == key {
                        return true;
                    }
                }
            }
            HelmAst::Mapping { items } => {
                if has_pair_with_key(items, key) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

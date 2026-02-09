use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};

pub fn prometheusrule_src() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/bitnami-redis/templates/prometheusrule.yaml"
    );
    std::fs::read_to_string(path).expect("read prometheusrule.yaml")
}

pub fn helpers_src() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/bitnami-redis/templates/_helpers.tpl"
    );
    std::fs::read_to_string(path).expect("read _helpers.tpl")
}

pub fn networkpolicy_src() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/bitnami-redis/templates/networkpolicy.yaml"
    );
    std::fs::read_to_string(path).expect("read networkpolicy.yaml")
}

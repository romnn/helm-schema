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

pub fn cert_manager_deployment_src() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/cert-manager/templates/deployment.yaml"
    );
    std::fs::read_to_string(path).expect("read cert-manager deployment.yaml")
}

pub fn cert_manager_helpers_src() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/cert-manager/templates/_helpers.tpl"
    );
    std::fs::read_to_string(path).expect("read cert-manager _helpers.tpl")
}

use helm_schema_ast::{DefineIndex, HelmParser};

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

pub fn cert_manager_service_src() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/cert-manager/templates/service.yaml"
    );
    std::fs::read_to_string(path).expect("read cert-manager service.yaml")
}

pub fn common_helpers_srcs() -> Vec<String> {
    let base = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/bitnami-redis/charts/common/templates"
    );
    let mut srcs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|e| e == "tpl")
                && let Ok(content) = std::fs::read_to_string(entry.path())
            {
                srcs.push(content);
            }
        }
    }
    srcs
}

pub fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(parser, &helpers_src()).expect("helpers");
    for src in common_helpers_srcs() {
        let _ = idx.add_source(parser, &src);
    }
    idx
}

pub fn build_cert_manager_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(parser, &cert_manager_helpers_src())
        .expect("cert-manager helpers");
    idx
}

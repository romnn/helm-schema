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

pub fn common_helpers_srcs() -> Vec<String> {
    let base = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/bitnami-redis/charts/common/templates"
    );
    let mut srcs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            if entry.path().extension().map_or(false, |e| e == "tpl") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    srcs.push(content);
                }
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

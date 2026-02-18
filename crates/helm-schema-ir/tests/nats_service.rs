#![recursion_limit = "1024"]

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};

fn build_nats_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();

    idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats/templates/_helpers.tpl"),
    )
    .expect("nats helpers");
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats/templates/_jsonpatch.tpl"),
    )
    .expect("nats jsonpatch");
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats/templates/_tplYaml.tpl"),
    )
    .expect("nats tplYaml");
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats/templates/_toPrettyRawJson.tpl"),
    )
    .expect("nats toPrettyRawJson");

    // Files loaded via `.Files.Get`.
    idx.add_file_source(
        "files/service.yaml",
        &test_util::read_testdata("charts/nats/files/service.yaml"),
    );
    idx
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_full() {
    let src = test_util::read_testdata("charts/nats/templates/service.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = build_nats_define_index(&TreeSitterParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);

    let actual: serde_json::Value = serde_json::to_value(&ir).expect("serialize");

    if std::env::var("SYMBOLIC_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let svc = serde_json::json!({"api_version": "v1", "kind": "Service"});
    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});

    // Note: `nats.defaultValues` is intentionally ignored for IR purposes; it's a side-effect helper.
    let expected = serde_json::json!([
        {
            "source_expr": "config",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.cluster.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.cluster.port",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.cluster.tls.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.gateway.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.gateway.port",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.gateway.tls.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.leafnodes.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.leafnodes.port",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.leafnodes.tls.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.monitor.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.monitor.port",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.monitor.tls.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.mqtt.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.mqtt.port",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.mqtt.tls.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.nats.port",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.nats.tls.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.profiling.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.profiling.port",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.profiling.tls.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.websocket.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.websocket.port",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "config.websocket.tls.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "global.labels",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "nameOverride",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "namespaceOverride",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "service",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "service",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": null
        },
        {
            "source_expr": "service.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service")],
            "resource": null
        },
        {
            "source_expr": "service.merge",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": null
        },
        {
            "source_expr": "service.name",
            "path": ["metadata", "name"],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "service.patch",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": null
        },
        {
            "source_expr": "service.ports",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "service.ports.cluster.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "service.ports.gateway.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "service.ports.leafnodes.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "service.ports.monitor.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "service.ports.mqtt.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "service.ports.nats.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "service.ports.profiling.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        },
        {
            "source_expr": "service.ports.websocket.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service"), t("service.enabled")],
            "resource": svc
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}

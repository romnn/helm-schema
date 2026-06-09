//! Round-7 Finding 4: end-to-end regression for the `kind: List`
//! envelope. Vendored alertmanager templates
//! (`ingressperreplica.yaml`, `serviceperreplica.yaml`) wrap real
//! resources in a `kind: List` envelope. The envelope itself is a
//! transparent K8s wrapper, not a meaningful schema target — emitting
//! `MissingSchema(kind=List, api_version=v1)` for the wrapper is
//! noise the user can't act on.
//!
//! Round-5 and round-6 added chain-layer suppression in two paths
//! (`Chain::schema_for_use` and `Chain::commit_missing_schema`).
//! This test pins the full pipeline (parser → SymbolicIrGenerator →
//! Chain) end-to-end so the suppression is exercised through the
//! real IR shape vendored charts produce, not just synthetic
//! ResourceRefs in chain-only unit tests.

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_gen::generate_values_schema_with_values_yaml;
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};
use helm_schema_k8s::{Chain, Diagnostic, DiagnosticSink, KubernetesJsonSchemaProvider};

const KIND_LIST_TEMPLATE: &str = "\
apiVersion: v1
kind: List
items:
{{- range $index, $replica := until (.Values.replicas | int) }}
  - apiVersion: networking.k8s.io/v1
    kind: Ingress
    metadata:
      name: {{ $.Release.Name }}-{{ $index }}
    spec:
      rules:
        - host: {{ $.Values.host | quote }}
{{- end }}
";

const KIND_LIST_VALUES: &str = "\
replicas: 2
host: example.com
";

#[test]
fn kind_list_envelope_emits_no_missing_schema_diagnostic() {
    let ast = TreeSitterParser.parse(KIND_LIST_TEMPLATE).expect("parse");
    let idx = DefineIndex::new();
    let ir = SymbolicIrGenerator.generate(KIND_LIST_TEMPLATE, &ast, &idx);

    let diagnostics = DiagnosticSink::new();
    // Use an offline provider — the List envelope short-circuits
    // before any K8s lookup, so download isn't needed for the
    // diagnostic assertion. (Inner items[*] uses currently carry the
    // wrapper's identity per the documented dual-detector limitation
    // in AGENTS.md, which is why we only assert the wrapper isn't
    // emitted — not that the inner Ingress is validated.)
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_allow_download(false)
        .with_diagnostic_sink(diagnostics.clone());
    let chain = Chain::new(vec![Box::new(k8s_provider)]).with_diagnostic_sink(diagnostics.clone());

    let _schema = generate_values_schema_with_values_yaml(&ir, &chain, Some(KIND_LIST_VALUES));

    let snapshot = diagnostics.snapshot();
    let list_missing = snapshot
        .iter()
        .filter(|d| {
            matches!(
                d,
                Diagnostic::MissingSchema { kind, .. } if kind == "List"
            )
        })
        .count();
    assert_eq!(
        list_missing, 0,
        "Chain must not emit MissingSchema(kind=List, ...) for the K8s List envelope; got diagnostics: {snapshot:?}"
    );
}

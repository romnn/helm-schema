//! An unquoted `--flag=` argument does not hide the typed slots rendered after it.

use color_eyre::eyre;
use helm_schema_gen::{AuthoringPolicy, DeclaredTypes, SchemaProfile};
use helm_schema_test_support::{generate, registry::ChartRecipe};
use indoc::indoc;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

const DEPLOYMENT: &str = indoc! {r"
    {{- if .Values.enabled }}
    apiVersion: apps/v1
    kind: Deployment
    metadata:
      name: web
    spec:
      selector:
        matchLabels:
          app: web
      template:
        metadata:
          labels:
            app: web
        spec:
          containers:
            - name: web
              image: nginx
              args:
              - --metrics-bind-address=:{{ .Values.metricsPort }}
              - --zap-log-level={{ .Values.logLevel }}
              readinessProbe:
                httpGet:
                  path: /readyz
                  port: 8080
                successThreshold: {{ .Values.probe.successThreshold }}
    {{- end }}
"};

const VALUES: &str = indoc! {"
    enabled: true
    metricsPort: 8080
    logLevel: info
    probe:
      successThreshold: 1
"};

fn chart_schema() -> eyre::Result<Value> {
    let chart = ScratchDir::new("dash-led-arguments")?;
    std::fs::create_dir(chart.path().join("templates"))?;
    std::fs::write(
        chart.path().join("Chart.yaml"),
        "apiVersion: v2\nname: dash-args\nversion: 1.0.0\n",
    )?;
    std::fs::write(chart.path().join("values.yaml"), VALUES)?;
    std::fs::write(chart.path().join("templates/deployment.yaml"), DEPLOYMENT)?;
    let recipe = ChartRecipe {
        authoring: AuthoringPolicy {
            declared_types: DeclaredTypes::Annotate,
            ..AuthoringPolicy::default()
        },
        ..ChartRecipe::corpus("dash-args", SchemaProfile::Full)
    };
    generate::session_schema(
        &helm_schema::AnalysisSession::new(generate::generate_options_at(chart.path(), &recipe)),
        &recipe,
    )
}

/// Verdicts from Helm 4.2.3 and kubeconform strict 1.29.0
/// (transfer/helm-receipts/dash-plain-start/receipts.txt): `abc` renders an invalid
/// Deployment whatever the flag values, `"1"` renders the integer 1, and a boolean
/// `enabled: false` omits the Deployment while the string `"false"` keeps it.
#[test]
fn a_probe_after_dash_led_arguments_keeps_its_provider_type() -> eyre::Result<()> {
    let validator = jsonschema::validator_for(&chart_schema()?)?;
    for (port, level, enabled, threshold, want) in [
        (json!(8080), "info", json!(true), json!("abc"), false),
        (json!(8080), "info", json!(true), json!("1"), true),
        (json!(8080), "info", json!(true), json!(1), true),
        (json!("9090"), "debug", json!(true), json!("abc"), false),
        (json!("9090"), "debug", json!(true), json!("1"), true),
        (json!(8080), "info", json!(false), json!("abc"), true),
        (json!(8080), "info", json!("false"), json!("abc"), false),
    ] {
        let document = json!({
            "enabled": enabled,
            "metricsPort": port,
            "logLevel": level,
            "probe": {"successThreshold": threshold},
        });
        sim_assert_eq!(have: validator.is_valid(&document), want: want, "{document}");
    }
    Ok(())
}

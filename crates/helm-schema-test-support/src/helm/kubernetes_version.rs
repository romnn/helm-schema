//! The Kubernetes version the corpus renders each chart under.

use std::fs;
use std::path::Path;

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema_ast::semver_constraint_matches_version;
use serde_json::Value;

/// The Kubernetes versions the corpus renders with, the corpus policy
/// version first.
pub const PINNED_KUBERNETES_VERSIONS: [&str; 2] = ["1.29.0", "1.33.0"];

/// Constraint spellings the shared evaluator abstains on, each with the first
/// pinned version Helm v4.2.3 admits. Masterminds reads `x` as a wildcard:
/// Helm renders a chart requiring `>=1.21.x-0` under 1.29.0 and refuses it
/// under 1.20.0. An entry is keyed by the exact spelling, so a changed
/// manifest falls back to the evaluator instead of inheriting a verdict.
const HELM_DECIDED_CONSTRAINTS: [(&str, &str); 1] = [(">=1.21.x-0", "1.29.0")];

/// The `--kube-version` Helm renders the chart at `chart_dir` under.
///
/// Helm v4.2.3 refuses a chart whose root `Chart.yaml` `kubeVersion`
/// constraint rejects the target version before it reads any values
/// (`renderResources`, pkg/action/action.go:287-291); dependency manifests
/// are not checked. The first pinned version the root constraint admits is
/// chosen, and a chart without a constraint renders under the policy
/// version. The constraint is decided by the evaluator `semverCompare`
/// guards use, and only a spelling it abstains on by the Helm-verified
/// verdicts of `HELM_DECIDED_CONSTRAINTS`.
///
/// # Errors
///
/// Returns an error when the manifest cannot be read, its `kubeVersion` is
/// not a string, the shared constraint evaluator cannot decide it, or no
/// pinned version satisfies it.
pub fn chart_kubernetes_version(chart_dir: &Path) -> eyre::Result<&'static str> {
    // The adjudicator renders a chart that ships only a template manifest
    // with that manifest copied to `Chart.yaml`.
    let mut path = chart_dir.join("Chart.yaml");
    if !path.is_file() {
        path = chart_dir.join("Chart.template.yaml");
    }
    let manifest: Value = serde_yaml::from_str(
        &fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?,
    )
    .wrap_err_with(|| format!("parse {}", path.display()))?;
    let constraint = match manifest.get("kubeVersion") {
        None | Some(Value::Null) => return Ok(PINNED_KUBERNETES_VERSIONS[0]),
        Some(Value::String(constraint)) if constraint.is_empty() => {
            return Ok(PINNED_KUBERNETES_VERSIONS[0]);
        }
        Some(Value::String(constraint)) => constraint,
        Some(other) => eyre::bail!("{}: kubeVersion {other} is not a string", path.display()),
    };
    for version in PINNED_KUBERNETES_VERSIONS {
        match semver_constraint_matches_version(constraint, version) {
            Some(true) => return Ok(version),
            Some(false) => {}
            None => {
                for (spelling, admitted) in HELM_DECIDED_CONSTRAINTS {
                    if spelling == constraint {
                        return Ok(admitted);
                    }
                }
                eyre::bail!(
                    "{}: kubeVersion {constraint:?} is outside the supported constraint syntax",
                    path.display()
                );
            }
        }
    }
    eyre::bail!(
        "{}: no pinned Kubernetes version satisfies kubeVersion {constraint:?}",
        path.display()
    )
}

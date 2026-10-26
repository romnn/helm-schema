//! Exact hard failures inherited by the battery, with no allowance for missing entries.

use std::borrow::Cow;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::helm::lint::{LintClassification, LintErrorKind, classify_lint};
use helm_schema_test_support::source_digest::sha256_hex;
use serde::{Deserialize, Serialize};

use super::HelmAdjudicationCoverage;
use super::known_false_acceptances::{
    Baseline, ContextSelector, ContextTransport, Probe, Rejection,
};

pub(crate) enum FailureSelector {
    Context(ContextSelector),
    Probe {
        probe: Probe,
        kubernetes_version: &'static str,
    },
}

impl FailureSelector {
    fn case(&self, chart: &str) -> eyre::Result<String> {
        match self {
            Self::Context(selector) => selector.case(chart),
            Self::Probe { probe, .. } => Ok(format!("{chart}: {} <- {}", probe.path, probe.value)),
        }
    }

    fn kubernetes_version(&self) -> &str {
        match self {
            Self::Context(selector) => selector.kubernetes_version,
            Self::Probe {
                kubernetes_version, ..
            } => kubernetes_version,
        }
    }
}

pub(crate) enum FailureClass {
    Adjudication {
        reason: &'static str,
    },
    OracleFalseAcceptance {
        rejection: Rejection,
        baseline: Baseline,
        kubernetes_errors: &'static [&'static str],
    },
}

/// Exact input bytes and the ordered applicable controls, including the defaults control.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct FailureInput {
    pub(crate) values_file_sha256: Cow<'static, str>,
    pub(crate) controls_sha256: Cow<'static, str>,
}

impl FailureInput {
    pub(crate) fn new(values: &[u8], controls: &[serde_json::Value]) -> eyre::Result<Self> {
        let controls = serde_json::json!({"defaults": {}, "applicable": controls});
        Ok(Self {
            values_file_sha256: sha256_hex(values).into(),
            controls_sha256: sha256_hex(&serde_json::to_vec(&controls)?).into(),
        })
    }

    pub(crate) fn for_probe(probe: &super::lint_probes::ScreenedProbe) -> eyre::Result<Self> {
        match &probe.document {
            Some(document) => Self::new(&document.values_file, &document.control_overlays),
            None => Self::new(
                &serde_json::to_vec_pretty(&probe.overlay)?,
                &[serde_json::json!({})],
            ),
        }
    }
}

pub(crate) struct KnownInheritedFailure {
    pub(crate) chart: &'static str,
    pub(crate) selector: FailureSelector,
    pub(crate) failure: FailureClass,
    pub(crate) input: FailureInput,
    pub(crate) lint_diagnostics_sha256: Option<Cow<'static, str>>,
    pub(crate) owner: &'static str,
    pub(crate) observed_on: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct ObservedFailure {
    pub(crate) case: String,
    pub(crate) kubernetes_version: String,
    pub(crate) reason: String,
    pub(crate) input: Option<FailureInput>,
    pub(crate) evidence: Option<std::path::PathBuf>,
}

impl ObservedFailure {
    pub(crate) fn from_adjudication(
        case: String,
        kubernetes_version: String,
        message: &str,
        input: Option<FailureInput>,
    ) -> Self {
        let reason = message
            .strip_prefix(&format!("{case}: "))
            .unwrap_or(message);
        let (reason, evidence) = match reason.rsplit_once("; evidence=") {
            Some((reason, path)) => (reason, Some(std::path::PathBuf::from(path))),
            None => (reason, None),
        };
        let reason = case.split_once(": ").map_or(reason, |(_, probe)| {
            reason
                .strip_suffix(&format!("; case={probe}"))
                .unwrap_or(reason)
        });
        Self {
            case,
            kubernetes_version,
            reason: reason.to_string(),
            input,
            evidence,
        }
    }
}

impl KnownInheritedFailure {
    pub(crate) fn lists(&self, observed: &ObservedFailure) -> eyre::Result<bool> {
        let FailureClass::Adjudication { reason } = self.failure else {
            return Ok(false);
        };
        Ok(self.selector.case(self.chart)? == observed.case
            && self.selector.kubernetes_version() == observed.kubernetes_version
            && reason == observed.reason
            && observed.input.as_ref() == Some(&self.input)
            && self.matches_lint_evidence(observed.evidence.as_deref())?)
    }

    fn matches_lint_evidence(&self, evidence: Option<&std::path::Path>) -> eyre::Result<bool> {
        let Some(expected) = self.lint_diagnostics_sha256.as_deref() else {
            return Ok(true);
        };
        let Some(evidence) = evidence else {
            return Ok(false);
        };
        Ok(sha256_hex(&std::fs::read(evidence.join("values.json"))?)
            == self.input.values_file_sha256
            && lint_diagnostics_sha256(evidence)? == expected)
    }
}

/// Hashes complete rule-qualified error blocks, including metaschema continuation lines.
pub(crate) fn lint_diagnostics_sha256(evidence: &std::path::Path) -> eyre::Result<String> {
    let mut executions = Vec::new();
    let mut inputs = vec![(String::new(), evidence.to_owned())];
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(evidence.join("schema-verdicts.json"))?)?;
    if let Some(controls) = report.get("controls").and_then(serde_json::Value::as_array) {
        for control in controls {
            let path = control["lint_evidence"]
                .as_str()
                .ok_or_eyre("missing lint control evidence")?;
            inputs.push((path.to_owned(), evidence.join(path)));
        }
    }
    for (control, directory) in inputs {
        let stages: &[_] = if control.is_empty() {
            &["baseline", "candidate", "schema-free"]
        } else {
            &["schema-free"]
        };
        for stage in stages {
            let status = std::fs::read_to_string(directory.join(format!("lint-{stage}.status")))?;
            let code: i32 = status
                .trim()
                .strip_prefix("exit status: ")
                .ok_or_eyre("lint evidence has no exit status")?
                .parse()?;
            let mut log = std::fs::read(directory.join(format!("lint-{stage}.yaml")))?;
            log.extend(std::fs::read(
                directory.join(format!("lint-{stage}.stderr")),
            )?);
            let class = classify_lint(code, &log);
            eyre::ensure!(
                !matches!(class, LintClassification::Unresolved(_)),
                "unresolved inherited lint evidence: {class:?}"
            );
            let text = String::from_utf8(log)?
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            let mut errors = Vec::new();
            let mut in_error = false;
            for line in text.lines() {
                if line.starts_with("[ERROR]") {
                    in_error = true;
                } else if line.starts_with("[INFO]")
                    || line.starts_with("[WARNING]")
                    || line.starts_with("==> Linting ")
                    || line == "Error: 1 chart(s) linted, 1 chart(s) failed"
                    || line == "1 chart(s) linted, 0 chart(s) failed"
                {
                    in_error = false;
                }
                if in_error && !line.trim().is_empty() {
                    errors.push(line.trim_end().to_owned());
                }
            }
            let errors = normalize_lint_errors(errors, &class)?;
            executions.push((control.clone(), stage, code, class.class(), errors));
        }
    }
    Ok(sha256_hex(&serde_json::to_vec(&executions)?))
}

/// Go maps can reorder siblings; parent relationships and repeated errors still identify failures.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct SchemaDiagnostic {
    line: String,
    children: Vec<Self>,
}

impl SchemaDiagnostic {
    fn append_lines(self, output: &mut Vec<String>) {
        output.push(self.line);
        for child in self.children {
            child.append_lines(output);
        }
    }
}

fn schema_diagnostics(
    lines: &mut std::iter::Peekable<std::vec::IntoIter<String>>,
    indent: usize,
) -> eyre::Result<Vec<SchemaDiagnostic>> {
    let mut siblings = Vec::new();
    while let Some(line) = lines.peek() {
        let depth = line.len() - line.trim_start_matches(' ').len();
        if depth < indent {
            break;
        }
        eyre::ensure!(depth == indent, "invalid lint diagnostic indentation");
        let line = lines.next().ok_or_eyre("missing lint diagnostic")?;
        let mut children = Vec::new();
        if let Some(next) = lines.peek() {
            let child_depth = next.len() - next.trim_start_matches(' ').len();
            if child_depth > depth {
                children = schema_diagnostics(lines, child_depth)?;
            }
        }
        siblings.push(SchemaDiagnostic { line, children });
    }
    siblings.sort();
    Ok(siblings)
}

fn normalize_lint_errors(
    errors: Vec<String>,
    class: &LintClassification,
) -> eyre::Result<Vec<String>> {
    let mut lines = errors.into_iter().peekable();
    let mut output = Vec::new();
    let mut schema_rule = false;
    while let Some(mut line) = lines.next() {
        if line.starts_with("[ERROR]") {
            schema_rule = matches!(class, LintClassification::Reject { errors }
                if errors.iter().any(|error| error.kind != LintErrorKind::Semantic
                    && error.diagnostic.trim_end() == line));
            if schema_rule
                && let Some((header, body)) = line.split_once(": ")
                && body.starts_with("- at ")
            {
                let body = body.to_owned();
                output.push(format!("{header}:"));
                line = body;
            }
        }
        if schema_rule && line.trim_start_matches(' ').starts_with("- at ") {
            let indent = line.len() - line.trim_start_matches(' ').len();
            let mut segment = vec![line];
            while lines
                .peek()
                .is_some_and(|line| line.trim_start_matches(' ').starts_with("- at "))
            {
                segment.push(lines.next().ok_or_eyre("missing schema diagnostic")?);
            }
            let mut segment = segment.into_iter().peekable();
            for diagnostic in schema_diagnostics(&mut segment, indent)? {
                diagnostic.append_lines(&mut output);
            }
            eyre::ensure!(segment.peek().is_none(), "unparsed schema diagnostics");
        } else {
            output.push(line);
        }
    }
    Ok(output)
}

pub(crate) fn oracle_false_acceptance_is_listed(
    rows: &[KnownInheritedFailure],
    observed: &super::ObservedFalseAcceptance,
) -> eyre::Result<bool> {
    for row in rows {
        if row.lists_oracle_false_acceptance(observed)? {
            return Ok(true);
        }
    }
    Ok(false)
}

impl KnownInheritedFailure {
    fn lists_oracle_false_acceptance(
        &self,
        observed: &super::ObservedFalseAcceptance,
    ) -> eyre::Result<bool> {
        let FailureClass::OracleFalseAcceptance {
            rejection,
            baseline,
            kubernetes_errors,
        } = &self.failure
        else {
            return Ok(false);
        };
        if self.selector.case(self.chart)? != observed.case
            || *rejection != observed.rejection
            || *baseline != observed.baseline
        {
            return Ok(false);
        }
        let Some(evidence) = &observed.evidence else {
            return Ok(false);
        };
        let evidence: serde_json::Value =
            serde_json::from_slice(&std::fs::read(evidence.join("schema-verdicts.json"))?)?;
        if evidence.get("failure_input") != Some(&serde_json::to_value(&self.input)?)
            || !self.matches_lint_evidence(observed.evidence.as_deref())?
        {
            return Ok(false);
        }
        Ok(
            evidence.get("new_kubernetes_errors") == Some(&serde_json::json!(kubernetes_errors))
                && evidence.get("kubernetes_version")
                    == Some(&serde_json::json!(self.selector.kubernetes_version()))
                && evidence.get("transport") == Some(&serde_json::json!("values-file")),
        )
    }
}

/// Every observed hard failure must be listed, and every entry must recur.
/// A filtered diagnostic run cannot certify the complete inherited list.
pub(crate) fn validate_inherited_failures(
    coverage: &HelmAdjudicationCoverage,
    rows: &[KnownInheritedFailure],
) -> eyre::Result<usize> {
    let mut problems = Vec::new();
    let mut identities = std::collections::BTreeSet::new();
    for row in rows {
        let case = row.selector.case(row.chart)?;
        eyre::ensure!(
            identities.insert((
                case.clone(),
                row.selector.kubernetes_version().to_owned(),
                row.input.clone()
            )),
            "duplicate inherited failure: {case}"
        );
        eyre::ensure!(
            !row.owner.is_empty() && !row.observed_on.is_empty(),
            "inherited failure has no owner or observation: {case}"
        );
        let mut observed = false;
        match row.failure {
            FailureClass::Adjudication { .. } => {
                for failure in &coverage.adjudication_failures {
                    observed |= row.lists(failure)?;
                }
            }
            FailureClass::OracleFalseAcceptance { .. } => {
                for failure in &coverage.false_acceptances {
                    observed |= row.lists_oracle_false_acceptance(failure)?;
                }
            }
        }
        if observed {
            eprintln!(
                "INHERITED_HARD_FAILURE owner={} observed_on={}: {case}",
                row.owner, row.observed_on
            );
        } else {
            problems.push(format!(
                "stale inherited failure; remove it: {case} (owner={}, observed_on={})",
                row.owner, row.observed_on
            ));
        }
    }
    for observed in &coverage.adjudication_failures {
        let mut listed = false;
        for row in rows {
            listed |= row.lists(observed)?;
        }
        if !listed {
            problems.push(format!("unlisted hard failure: {observed:?}"));
        }
    }
    let mut oracle_false_acceptances = 0;
    for observed in &coverage.false_acceptances {
        if oracle_false_acceptance_is_listed(rows, observed)? {
            oracle_false_acceptances += 1;
        }
    }
    eyre::ensure!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(oracle_false_acceptances)
}

pub(crate) const KNOWN_INHERITED_FAILURES: &[KnownInheritedFailure] = &[
    KnownInheritedFailure {
        chart: "datadog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"datadog":{"operator":{"enabled":true}},"operator":{"installCRDs":null},"tags":{"install-crds":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f4f4e7ebff602262e568be858adad63c1062145ae8f709122fd3f711e1b311e9",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b4a3a63b8939ecba8f1e0055dc60a1aeeb1a26dc0aae650e7c23b4696f8cda03",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "datadog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"datadog":{"operator":{"enabled":true}},"operator":{"installCRDs":null},"tags":{"install-crds":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f2f9f199865f8183d5fd50b43919dcf8c7f6a634c7b3258fdf52b111ee232e95",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b4a3a63b8939ecba8f1e0055dc60a1aeeb1a26dc0aae650e7c23b4696f8cda03",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "eck-stack",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"eck-kibana":{"annotations":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "01db5456cbd416dae4674fa69473161a5bafed2aefbe09289c4d0b75fb8c1d38",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0dbc57332eb74a47dc189a691cd3410e8d4a942a2530fba0c30ae9c613c9a57f",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "eck-stack",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"eck-logstash":{"annotations":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "028b153d3a087662a05765c4640792dbbc93c0c48ad6385f996f70f4f68385dd",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0dbc57332eb74a47dc189a691cd3410e8d4a942a2530fba0c30ae9c613c9a57f",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "eck-stack",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"eck-elasticsearch":{"annotations":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "24b6c15de467c906eaa942422c6ff2016883b5b946336d66e0d950495e0f5f22",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0dbc57332eb74a47dc189a691cd3410e8d4a942a2530fba0c30ae9c613c9a57f",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "eck-stack",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"eck-package-registry":{"annotations":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "bc8b389751c341a0a3da7b640a83293c529e46f0f30c435f3c4122b644f60164",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0dbc57332eb74a47dc189a691cd3410e8d4a942a2530fba0c30ae9c613c9a57f",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "eck-stack",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"eck-apm-server":{"annotations":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e05a8ef36f6d97931a582851856ec4f5c9c509926ef420d220c8d37b73671dd1",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0dbc57332eb74a47dc189a691cd3410e8d4a942a2530fba0c30ae9c613c9a57f",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "gitea",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql-ha":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0c9f830c6ee5a0dc8e1854e342f5ce231a7a09456ae455559ef27592d95648b5",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "712aaf039dd721b1d4f7070aad0b25f3897c6f4d7b2ac39fde66a0907d6a0b48",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "gitea",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"valkey-cluster":{"cluster":{"externalAccess":{"enabled":false}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3d410e15198de15ff6a2b198b67d7ab8cab51343abf552b8fb403ca7e8185cb1",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "712aaf039dd721b1d4f7070aad0b25f3897c6f4d7b2ac39fde66a0907d6a0b48",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "gitea",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql-ha":{"backup":{"cronjob":{"affinity":{}}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c9a7ab5e6620be7899538db5710195c933d600ade09bd59d229969084cf7f8ec",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "712aaf039dd721b1d4f7070aad0b25f3897c6f4d7b2ac39fde66a0907d6a0b48",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "gitea",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"tags":{"bitnami-common":true},"valkey-cluster":{"common":{"exampleValue":"common-chart"},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "cadae6e41113874ea412b5b8e494d8a9a2a01b4c61f359a8b5a660c2502b9ed1",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "712aaf039dd721b1d4f7070aad0b25f3897c6f4d7b2ac39fde66a0907d6a0b48",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "goldilocks",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"vpa":{"enabled":true,"metrics-server":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2e08416f1f61eaa892d1c7ec1b192a87b0fc48550c0fb8780fa936ef2a7fb927",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "38984209b9d7363d665213810e3f647b4625c36d1b3af7d3d4ad57fd31075b72",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "goldilocks",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"vpa":{"enabled":true,"metrics-server":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2e08416f1f61eaa892d1c7ec1b192a87b0fc48550c0fb8780fa936ef2a7fb927",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "38984209b9d7363d665213810e3f647b4625c36d1b3af7d3d4ad57fd31075b72",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "goldilocks",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"vpa":{"enabled":true,"metrics-server":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2e08416f1f61eaa892d1c7ec1b192a87b0fc48550c0fb8780fa936ef2a7fb927",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "38984209b9d7363d665213810e3f647b4625c36d1b3af7d3d4ad57fd31075b72",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"community-operator":null,"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2e8fa215883c3188f755abda818c05d86689276f38045c6c6e8f8e4b431e2e04",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f8200389f4cb1a3ea1e543901a853dafa99c5bd4556a047170e877239bf27bc",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"community-operator":{"community-operator-crds":null},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5ddc3341f4cf2262ea340e52e3748b83e1d2829670650487e77993f93a493d42",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f8200389f4cb1a3ea1e543901a853dafa99c5bd4556a047170e877239bf27bc",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"community-operator":{"community-operator-crds":{"enabled":null}},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e6639f8cce3c5b67c6d97d7e41e81c6c4313e7d26575ca8ea6f434047911d706",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f8200389f4cb1a3ea1e543901a853dafa99c5bd4556a047170e877239bf27bc",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"community-operator":{"community-operator-crds":{"enabled":true}},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "45e98b5c2c7a04d8e7c3c871fd50c9b78c483123383136e7daaedd58c4767fbc",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f8200389f4cb1a3ea1e543901a853dafa99c5bd4556a047170e877239bf27bc",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"community-operator":null,"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2e8fa215883c3188f755abda818c05d86689276f38045c6c6e8f8e4b431e2e04",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f8200389f4cb1a3ea1e543901a853dafa99c5bd4556a047170e877239bf27bc",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"community-operator":{"community-operator-crds":null},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5ddc3341f4cf2262ea340e52e3748b83e1d2829670650487e77993f93a493d42",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f8200389f4cb1a3ea1e543901a853dafa99c5bd4556a047170e877239bf27bc",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"community-operator":{"community-operator-crds":{"enabled":null}},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e6639f8cce3c5b67c6d97d7e41e81c6c4313e7d26575ca8ea6f434047911d706",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f8200389f4cb1a3ea1e543901a853dafa99c5bd4556a047170e877239bf27bc",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"community-operator":{"community-operator-crds":{"enabled":true}},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "45e98b5c2c7a04d8e7c3c871fd50c9b78c483123383136e7daaedd58c4767fbc",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f8200389f4cb1a3ea1e543901a853dafa99c5bd4556a047170e877239bf27bc",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"community-operator":null,"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2e8fa215883c3188f755abda818c05d86689276f38045c6c6e8f8e4b431e2e04",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f8200389f4cb1a3ea1e543901a853dafa99c5bd4556a047170e877239bf27bc",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"community-operator":{"community-operator-crds":null},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5ddc3341f4cf2262ea340e52e3748b83e1d2829670650487e77993f93a493d42",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f8200389f4cb1a3ea1e543901a853dafa99c5bd4556a047170e877239bf27bc",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"community-operator":{"community-operator-crds":{"enabled":true}},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "45e98b5c2c7a04d8e7c3c871fd50c9b78c483123383136e7daaedd58c4767fbc",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f8200389f4cb1a3ea1e543901a853dafa99c5bd4556a047170e877239bf27bc",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "headscale",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"architecture":"standalone","enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6acc2a6d0485a78705005ff0187ef06aa0e8e0e0c5a45ee6bed2b7948b4c716d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "737f82e6f6fde652046745b6c6ae371e897ad6888fd48cc6425903541a879f25",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "headscale",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b3ec24a982b9bd1b7d5ab09e923e7f29264bbcb13cd78f2334a6434aef9cdfed",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "737f82e6f6fde652046745b6c6ae371e897ad6888fd48cc6425903541a879f25",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "headscale",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"common":{"addons":{"codeserver":{"args":["--auth","none"]}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "be8a0cd69b7ec7f63e3b47ce93a28da2b69b9cfacf649635c91c01eadbd45bf6",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "737f82e6f6fde652046745b6c6ae371e897ad6888fd48cc6425903541a879f25",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "kube-starrocks",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"operator":{"global":{"rbac":{"create":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ebc052f47bc2a4f75459048bab6969ffbb54e3b5152220f78f56fa636a915a0a",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "a446858176b0aba5cb869f7296c7915b2a7f043674ca784361b44adacdf31f8a",
        )),
        owner: "generator: inherited lint rejection without Kubernetes attribution",
        observed_on: "crd-oracle/extract/2087e952",
    },
    KnownInheritedFailure {
        chart: "kube-starrocks",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"starrocks":{"configMaps":[]}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f05ba401503d82a6c21325cc1f2656e69a782a6434910104a53d099a5654223f",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "a446858176b0aba5cb869f7296c7915b2a7f043674ca784361b44adacdf31f8a",
        )),
        owner: "generator: inherited lint rejection without Kubernetes attribution",
        observed_on: "crd-oracle/extract/2087e952",
    },
    KnownInheritedFailure {
        chart: "kyverno",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"annotations":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0c2c2ff538ff027a8f17370d525f58cb7eaeb4b6f9efc12d847c21743852e851",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b5be2dcb15f809c43e99e0de6fc96e3c7c0d978b69af92de92f47b28719911e9",
        )),
        owner: "generator: tpl name yaml-fragment false rejection (kyverno grafana.configMapName)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"temporal":{"prometheus":{"alertmanager":{"enabled":null},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d16b01dd27dbc6a62c6be45216e89750ca202337dd70b58ca101496f0c6fa533",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "e228d8f01489a1b24b24ce95be58816547615b7fe7f41b65f1d2915528bb9ab1",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"temporal":{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8addc37c26f50526f0235ad58d61935b2fe9ce0ccfa8910acd0ce95d234b7200",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "31406b4175bf3d442aa2878ef43ad2f1558b1ee9668e2cda8668d07ea094d483",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"temporal":{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "01901d760a587e6acc6619679f76aab3033843ead70519429849f8918bc059f5",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "60221a49fae7c14d3e51e385d9ce176c0871a958b459dceba941e0f28645c3a5",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"temporal":{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c537ad62ed5650b6605f0d3dac2fbbf464b3c4e56be490462d39263dda9cc90f",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "cf399d1741cb6cf2719997a12535b4c4601bcc5362c5bd7d14520236b9ed86cb",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"temporal":{"elasticsearch":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "33757d45e763a5f17ee45e030100b61ea6104504facc0647d1d1c5ddaa80e28e",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "145242793b7bd0f18a87a0937bdb68b78f8c0ad47f5e4202bcf475e9afb28093",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"temporal":{"prometheus":{"alertmanager":{"enabled":null},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d16b01dd27dbc6a62c6be45216e89750ca202337dd70b58ca101496f0c6fa533",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "e228d8f01489a1b24b24ce95be58816547615b7fe7f41b65f1d2915528bb9ab1",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"temporal":{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8addc37c26f50526f0235ad58d61935b2fe9ce0ccfa8910acd0ce95d234b7200",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "31406b4175bf3d442aa2878ef43ad2f1558b1ee9668e2cda8668d07ea094d483",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"temporal":{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "01901d760a587e6acc6619679f76aab3033843ead70519429849f8918bc059f5",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "60221a49fae7c14d3e51e385d9ce176c0871a958b459dceba941e0f28645c3a5",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"temporal":{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c537ad62ed5650b6605f0d3dac2fbbf464b3c4e56be490462d39263dda9cc90f",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "cf399d1741cb6cf2719997a12535b4c4601bcc5362c5bd7d14520236b9ed86cb",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "loki",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana-agent-operator":{"affinity":{}},"monitoring":{"selfMonitoring":{"grafanaAgent":{"installOperator":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults/context render control; template abort cannot establish an attributable rejection",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "690456f495583cddf90632dd843bbe053e08695bdd935773a9916c8098255492",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "83db049ac1de818e6808a634360605a79a7ae03b8467b02519d4968cf82bea3f",
        )),
        owner: "chart: loki defaults abort (chart_corpus KNOWN_VALUES_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "loki",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rollout_operator":{"affinity":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults/context render control; template abort cannot establish an attributable rejection",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ae9098c096715f1d93650f212efa650dae5171e387e62d9ff7a3a039389ddbfd",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "83db049ac1de818e6808a634360605a79a7ae03b8467b02519d4968cf82bea3f",
        )),
        owner: "chart: loki defaults abort (chart_corpus KNOWN_VALUES_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "loki",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"minio":{"additionalAnnotations":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults/context render control; template abort cannot establish an attributable rejection",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f7e7a64560dd8d7398a8f07dd087f5acfc63a88d8f962ff35af09c232d5a8d9f",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "83db049ac1de818e6808a634360605a79a7ae03b8467b02519d4968cf82bea3f",
        )),
        owner: "chart: loki defaults abort (chart_corpus KNOWN_VALUES_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "metallb",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"frr-k8s":{"crds":{"enabled":null}},"frrk8s":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f5867b29c9af041e4a81160d0cc1352aaf27c14e34b7b81eeb8aa61f46321e03",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "a5336ee7b1836e51ef008ee39de67c0be006dfdf4dc525c97c14f43e1567d173",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "metallb",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"frr-k8s":{"crds":{"enabled":null}},"frrk8s":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f5867b29c9af041e4a81160d0cc1352aaf27c14e34b7b81eeb8aa61f46321e03",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "a5336ee7b1836e51ef008ee39de67c0be006dfdf4dc525c97c14f43e1567d173",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "metallb",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"frr-k8s":{"crds":{"enabled":null}},"frrk8s":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f5867b29c9af041e4a81160d0cc1352aaf27c14e34b7b81eeb8aa61f46321e03",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "a5336ee7b1836e51ef008ee39de67c0be006dfdf4dc525c97c14f43e1567d173",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "nacos",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mysql":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "caedda65581e41e594b0d3ce894669705303121b55a83dbf197b668fea5a16c2",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "2f3d38cf999c4662e64c08f0a30b537350360666b16ae55edb9a5d80813337ce",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "nacos",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"common":{"exampleValue":"common-chart"},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "28f90dd6b60110aed9681f7cc4932401c41569b1f6125ffd66ea49ef65868f29",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "d8df17c2260902843c092083f40f23611c9b8177da93c73d7c83a40146c514b4",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "nacos",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mysql":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b4b334e0237b3f9bb2e38935e53b22e61c29068a0c8c33538e7e582784a9a164",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "d8df17c2260902843c092083f40f23611c9b8177da93c73d7c83a40146c514b4",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "nacos",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mysql":{"architecture":"standalone","enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e62da100c18fbc90e2422b565dfb74335d524ee668e0b42cbf2133a968a5a79a",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "d8df17c2260902843c092083f40f23611c9b8177da93c73d7c83a40146c514b4",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "netbox",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"common":{"exampleValue":"common-chart"},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "28f90dd6b60110aed9681f7cc4932401c41569b1f6125ffd66ea49ef65868f29",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "ef633989337042ee28dcdeea6478a83f058852f26a2e395196f8a777a5c53c5e",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "netbox",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"valkey":{"apiVersions":[],"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "53be8c9021d8069764f2f0ae17171ed5e3bbbfc58f0ca0822c6c578791ae8649",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "ef633989337042ee28dcdeea6478a83f058852f26a2e395196f8a777a5c53c5e",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "netbox",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"tags":{"bitnami-common":true},"valkey":{"common":{"exampleValue":"common-chart"},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6a83f214ee91af327363bca5be583a5f5102f7d060995827c2fbd5a5eced12b4",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "ef633989337042ee28dcdeea6478a83f058852f26a2e395196f8a777a5c53c5e",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "netbox",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"architecture":"standalone","enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6acc2a6d0485a78705005ff0187ef06aa0e8e0e0c5a45ee6bed2b7948b4c716d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "ef633989337042ee28dcdeea6478a83f058852f26a2e395196f8a777a5c53c5e",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "netbox",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b3ec24a982b9bd1b7d5ab09e923e7f29264bbcb13cd78f2334a6434aef9cdfed",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "ef633989337042ee28dcdeea6478a83f058852f26a2e395196f8a777a5c53c5e",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6f201d1c3e8aed5c5b019fb96e6d48410cd7a0974b6881af728ed42eef4fd5d7",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"cainjector":{"containerSecurityContext":{"allowPrivilegeEscalation":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "702532ff5a47406587a047cd2f14cbb35c5ec5b4ed671bd7f8b6a9ae4d417e0d",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"containerSecurityContext":{"allowPrivilegeEscalation":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a951718b40e190eb0ad1f6a05b8ae66629ad997fe4b3127cf047a7fc3a99ce31",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "608bc34f48ec247a0ab86e64a71adeab60f1fbb9a92ac698fc9c0d802bdc886d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e9305997f23029b3b4169365ab96e678e57e3eb66d217b27082e0056a9c57d7e",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true,"global":{"podSecurityPolicy":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "24f791f3e1c62c25842c07efceea4363d25e5db913386b7c081d68c165170c8b",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true,"prometheus":{"servicemonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c8690de964b690a49b35ff272ead0641e40dffc0fa8eed098059ed3beec9fac2",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true,"prometheus":{"servicemonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ef2c765ca0ae9226fe4c8cbf9f294631d602f5b339fe9c04c2ec3adc2da3e468",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true,"startupapicheck":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2b938f593a266f6e7e51375d0a3b191c045838f16eb9e9e3f87273f78dfe84ab",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true,"webhook":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f109d0953208ee12d1e313d7b57efbc555c16ed60fefaf91fdebd3d3ff0078ee",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true,"webhook":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "4fc5d59ef2a8b4f01e80b268b631d0d4e06db5ef6dd9534352fc5c231fca15aa",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "61ff8c3240c55333cfe4019cba3bdfb594ff0e2f33d7f71423517964ff9907ad",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f244a76b0ceca403bbef0fe788a25a01dfbaa489a000e120942e3250d406e150",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "4b393e66eed7b421ae6ba43592c743cc7fc08156387a3dcc34eb7ece94300df0",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"autoscaling":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5b21f13d208509af14b6f77882c42a40fe7ab3637eea9e8e9a1ca5903bc172a7",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"containerSecurityContext":{"allowPrivilegeEscalation":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "35963196b2813fe4eb5416768548744ffc1cb8d744214cb0359e4b97a1199114",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"downloadDashboards":{"securityContext":{"allowPrivilegeEscalation":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "31974cbad09f653d5a1d5864ea891b5cd020403740fd6261fe5412d0607cb794",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enableKubeBackwardCompatibility":true,"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "006ff55b3e84d914f3998ca7990b0c0b0f657823a433e929c2c61c693acdfbf4",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "64e63dc85f7d4e30a0f6c0eda8bafe9dca93a9fcdbfbd7d833ca92b24810013e",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"headlessService":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9687e72eb1fe72b2722c06280385cbe27e96721316c1dd4568c5768183a68ba5",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"autoscaling":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f9f1ffc064b9c5fa0533453a0294c8b15aa926873da9efc77d773b71e8e09bb5",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "791a03724689f8a9c869bc03bf0cf3a4f9891b4f9a08238b2794ef713c9162c4",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c285fb0a53f4d485319248b68dd76e8b771bc8c8fa895246d4d87ca6f7d1a941",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"networkPolicy":{"limitEgress":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "bd1f7205a8ee8f7198c1d4b4c1f43b9b26fb8eb328649c80df2fd44c9dbcc6d2",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "cff3a83e30b6e8494f1b6929b3830a04b04f43f6b4335a73cc3319669d842abe",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"ingress":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e226d1dfec23f779d16585871367ff711fef49476545ceceffe153f6aa593ca4",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"initChownData":{"securityContext":{"runAsNonRoot":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "82ccae09888d183613d7cc34d22a58ca99f96c509d37bfa8d83b6a11ee862745",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"ldap":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ae08aaaea5268ed12e01db3fea609788739827436d853de230b2c81c49d762bb",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"networkPolicy":{"egress":{"blockDNSResolution":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b519d208d8f639b77aa533f1576ed1fac39a339e78e3237d48b0c92c6434adef",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"networkPolicy":{"egress":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e1f07002820618cf678e5b57e5bfd4026267f66806d5a8fd0ae921208b5a75bc",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "08490198d8d74cc5fc055c7780df782efa292b06c3ab868cd6b3abf00077fb9f",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"persistence":{"disableWarning":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "65c908ea4a5b18023db7b589ab7ea4ec468d7230d64afdf9fb0bdd7ed1bf0d4e",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"persistence":{"inMemory":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "335802bf09e6de1e7a5b4fe13203df1cdece1f3b6011d820f993b49873196b56",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"rbac":{"namespaced":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "bed9df24263a975e631c7e0a9a6e0fe6262fcd393a2d1ccc7db53e296018b448",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"rbac":{"pspUseAppArmor":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "de57968f7ce7988fe4893ee7b450823164d839b647d8d844edf275479db15fad",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"serviceAccount":{"automountServiceAccountToken":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ff009adab232af70ae418e05104fd11f27cca70d840856422d0469921b3d92bb",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"serviceMonitor":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8a09d4d1637597b2e6c2bc5efc35a563841448eeeff738ca4b98df396a449891",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"alerts":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f2d03704a2f79f09282669c12b24562599cfc6d60de6e050edcffd34d75b43bd",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"alerts":{"initAlerts":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8a92e54ac9cc99cb50a1958ce337651844168e77b9242f9c7e195f3e4d8c2240",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"alerts":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3fcfea54b0a60880465bf30481df5774f61da127880ed234da6ee17ba76f0b90",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"dashboards":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a596eb280af490192efe0054befcca3855aee1d141c1df8273b0c09df124c263",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"dashboards":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "edf0887b89b1b5870a7682898353fddec8bae9bc975083ac4527c367920d7eb4",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"datasources":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f22b32ce28b6933bcefb1848781006293267dd7df2351ef3c7ee97be85bdeeed",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"datasources":{"initDatasources":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1c16810214cb08f984ec858ef5a51f02d140fe7e49b6590b6f29e3e1751dc269",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"datasources":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "cb7de90394b00afde5cafe5d9d71cb4e863ddf779aec240f17df548d0936ea03",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"enableUniqueFilenames":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2e48697070970eb89af5197e0ce7a480fbd073776083317fc1b8c4f86c7d01a8",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"notifiers":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "83743c27a6e6666dd92dc74ad9204a5edfe5eac52b44941718341fc1465ca220",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"notifiers":{"initNotifiers":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e49ab02956f05df7e596adccfaf75eb7531d95c3905ea85c94f4fa9f5a4d1e90",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"notifiers":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "918880a81cd6c0c200cb6e5556424c55aa60117a230b17b9a4c1d0c48b22a8ca",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"plugins":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f4c43e8b36528225a428246010a5cab9191a79308d86dc261cf1843e3e400fbd",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"plugins":{"initPlugins":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "05d6c4b1713b6b257e9ab1ef122cd255c63949975e73ad2ab2159d0abf17fd3c",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"plugins":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "482dc47192109a537499190b488f4f44f01848ad56f7422215cb3e1090e06233",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"securityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "babb9df691043f14c35fbd5c2a015720cb84c593fead09395ebb45b84c6f0005",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"useStatefulSet":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "4155dafe1323b5b5fe6f29c5509bada92c37711d4853fd37e33e734f9319f685",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "47eb7dacdbbd622cb6f19675e9825d84e71a069b674c6b2556dfcda09e131b00",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "aff189457f1e81b9da9a38e06a3f3254110176a3cbf8112b6a75486ed651cdf2",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "042a1fa1523491069bcc2292abd0a334ab2e2c89e0d7b37e755dc618b9a02d65",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"autoscaling":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "edbfd1afcbafa6c8c4ad83006d538a5ed9efa7cf17920a4f684c34cb962a1484",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"hostNetwork":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "286e3e041cb9d3f4ff5af635a5f423f545c590c47f08df439fa8dfcae743a7ef",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"hostPort":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2d9ba6a18f422c456360f0d2ac4f4954fc582d0889a486f2dbe258e1993da49b",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"image":{"chroot":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "08fd02be7a23e0a1465142cccfb1e7125b97cc9fd74a7d2d6bf3c533d30ea5f1",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"ingressClassByName":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b2f87db54ece31a66a78cc390c6d0008aae0dd90faf5a6c217eea54a48973c26",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"ingressClassResource":{"default":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "96509d4d41eb30a25d7061d49f54d45d5ddf41957b90d6b5846fd0a20d4bdd17",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"keda":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "72a1ac5d14f2265a5139f866ec0793a9c292c874a0811ebd8a62bc2551d3ee80",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"keda":{"restoreToOriginalReplicaCount":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d9108e13a820ada3d4f99c1e13bbec412e6d755a628eb3cbba31b16e3045db7f",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"metrics":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9430dcef0a5ef59c013be3625fca8b5904acdd7019c53b925353d6d93fc0fb44",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"reportNodeInternalIp":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "011d72f886ee9e7752732563a296222e100c4cf116ca17bd50f338984a7c5fbb",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"scope":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c14c36dab9fbea35fd082a65235bea9d605829e45a9de878028bc5aa67896c4a",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"shareProcessNamespace":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1a51459c8ef4d5da7b75056310df79fb9e0070a2fa7d9a6fa7557d660a263215",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"watchIngressWithoutClass":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "219e4d343b5db1eb9c4a8d39726b23449b18adfe533027ba3a7fcd5800f99696",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"defaultBackend":{"autoscaling":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6b2b59a2a715764e01a903355cee444a81b6740fa52f51ae42b7d8414d26e73e",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"defaultBackend":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "56f531dc695f7216f93da38d709857aef03e20b5cac6aba2c5e03a0e85eef5fb",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"defaultBackend":{"image":{"allowPrivilegeEscalation":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d51532562494e701f9217853b6547fa0b36ebbe41ff0809c073e0da06b190280",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d46058f4059977e17f8171132d292657653015bf14bd8a91f0fe8d69f4073e23",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "523c37e0b7ba90ee010f77a0ea493bddb971008a1d56c7bbfa93770eae69cc02",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"enabled":true,"podSecurityPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c63387221c74b708a8cb5703c091dd4b1922e4646eaaa07888fc27695c1578a9",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "77fc1edd8701636f59cd08b2783e9ec526bdd40f4648e2569dc1ea9afe0a29b9",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c53f4c07c1fe2370fbe610b3bd3c4e0110da16d228718fe92e77c908cc072245",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d548823ecf9ca074983563af3f6841877dc871f807081b3820eff903e83d639b",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"auth":{"usePasswordFiles":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b81a3fc26c7691c31609b4d0b25858c3bfcb81ffa343c5f025ff5e8ac5c2cda7",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "849186c0b9516cde8ff01f876f5de5495d1821785cf9595106cfa06aaeb105a3",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e5371209c2334730ea623f85678ec58f4ed34835fe8698c2892e627f82c612e1",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "642c940e1ddd1c31b8b8575ae4f38318bee7cde65010401035de0c8d448aacfd",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a809d20816de2899d1583c3473a8fe8d12683afa74050eec80a8ab11d778c01d",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"containerSecurityContext":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9c6057cdbd0edc8cbb1ac05fc71df4cd8041b9f5f145173784109c7944ca5316",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"containerSecurityContext":{"privileged":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1bd214e22f41684fa5f512000978ca3ae3d2fcb2e0981eb8b3f361b5ae37c5e8",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "4162f62fde3c5cc2947c5a2cb27b0790ae16713692b79e5b2c0b1ea75901504b",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d9574fe8a432f3a9e96c7e57389d63854d39edec7f83f43c7045847d4eeb0d0a",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "822329e9572ce854c6775177e6bf1aa9ad2755f91fdee250bf8e883e6d79f8d7",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "61f81ad69e550487b690d438b79b05f6268e15972c9d4a8bad593710efda0dc1",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"networkPolicy":{"egressRules":{"denyConnectionsToExternal":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b6aea96d6d1a5fd10bd40861de5a84aa394d0fd29d7ca9b7074a80cff8ba77c0",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "eff053c8184962a7d62bc57d73771685ff94c9a4c08b18c6553b64fe62935dcb",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"networkPolicy":{"metrics":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "961e95b9d84cbbbaaaa4dc71c541ef47d484e7d63eb1dd2b343e0479c4459dbf",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "7e8ce287cec6c9817a9c9f52c578538aa4d1ee1e45e26b3e41e3abbf2930e6a6",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"containerSecurityContext":{"privileged":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0a05ed7150e961994f5df36994645fee00f283b63f9189324af83614bd741e9b",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"pdb":{"create":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f6a60bbd6635ee41482fcd70fd0eb008572216595274295b8ae8e554ea72778a",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "36814dff648fae4274efcb04c4e678e1e94a1b436679eb9837583db43cfc28e4",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"rbac":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "58a6fd01054ec675414afa662e4bc00a1d4f4751f3a6ffbd85598ac1a02c4fff",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "84afc646d3ceaa06f1455fa69c15f8b04a846aaed1ebff74242d5ffce6f08b31",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"containerSecurityContext":{"privileged":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "10e617fd26e5771f5bde6a8765fdc542d0569155ff0e7e2a6d86e1b76a1f44bb",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"pdb":{"create":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d0393faa1d90c3669d8f7f67d39626fb3092fae1ac1d75072fd71a0d69582fcd",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "26331ba6a1cbc7771497c20e3e69b9c966daa34814c6e5cb6243f951fec50d6a",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"serviceAccount":{"automountServiceAccountToken":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "86331fd8de88974214e9c150984ea502c2a460814830cf0ae20a8dbe7b39f160",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"serviceBindings":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d71c21de3462f5c910e355ad4dd32d16b2163c5ebf1dd99ec19ed4d4ca3ca865",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5525c76cd216928b8f2384a1c67b30c2f4b607a00f86c57b6cd02af0e7dcc447",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3a14b1a7c779b8d4c463bc18b6bad1c07e3e8e8cfb171d5b7bb3d7df381a4155",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "46b840a34dfffa93fc69d06cd792ef26602cc0ea467d18fd4d41b1d17de0b40f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "24d00d97a13c1788d5f46499d667067485f6d826eb8ba4aa34c5770a971122fe",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6837282308cbeab82e2f3c7ceb1c799011671918d785b92781523c5e6790f308",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c33720c5d09075601b15d88352cef8287d4b56d83e7c8c05bc51b90567df3498",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "629092af7b433ea1801e058abc9a3a9e7e8d7490c85e0e0ce653a09cca8cf971",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"audit":{"logConnections":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "77281cd4d35b3a86dd6e1dff1c8728acd0be30e514d2940ead0b22e6e41810f6",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"audit":{"logDisconnections":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "99ae66e6d675666d225ed5350a79a29f8e9cfa6fccd98ce9f3a81159e470df36",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"audit":{"logHostname":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "86b2f125c5ff676d84fde4773acab5c86ffd59e3188a45eae614924b49171901",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"auth":{"usePasswordFiles":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9975888db9eba8fc600aa0088f055f82113b6f3882795258bbc92b56a7d3ac15",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6845aaaa9fe980fe2faf99f87e69be04a6d49695e1b910e0e4b3519d368a0094",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8791fdc87c59e560d7f3e3c1e166568c450a532f90a26843403bbc0a367b8654",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5f3c69e53f6015097ba21f93ba303a6f80a7a3a06c3b030b1c67ce9c5363b176",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e0bb171606d5b8acfd3f066c181df46b04cf80e2238928bbe050fc05fed722dc",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"ldap":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3868265171ecf7649a78c8927951e1d233106812d2ea0e8e31a42594296ddb2e",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"ldap":{"tls":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "13311de3fa774b53a152f972117191dd8bdc5c8952bac8c0b17c8b750499a7be",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "fb7297ae8a14a84bc2183c7eef34c5f7e784981f7bd0bfcffdf6613fc4465bdb",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "62979209353968f3542fc40521797e77dfdc9920f403722d9ed0ec43104f7620",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "32d1bbf527a0c0215a9c4c794f277e7d03f9c377ff81342e6cf94c97faccf15e",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3e2e01cd72e50c2abec92b2a0341c9e52c89dc1d6a681e436750873ff286f69e",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "baab9169020fcb57757c48ee54d5767345de2f41632ac1ca6ba08ecd17a935fd",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"networkPolicy":{"egressRules":{"denyConnectionsToExternal":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d209a0f6bd4a1c94111f05be50fe52fc4227e6cda7da177244ada120952a4362",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6b2ebfad45a1efff35ca8d04e060828ffb66e33dc56caed2d4fd488e06636e9d",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"networkPolicy":{"metrics":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "cd608ac34ccb64baeeae80c2a2e9a222a3a62dd56bcc84999a2dd8318daa5770",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"hostIPC":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "85f4e09928a60e1d76cce80cd28c0b5bfa742489f88c686fff7c5a9200bc43b4",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "374a2b3b46a5ee497f56cef66a4f3783d6ff9551600b2b023fe7ee6f2b889a53",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"standby":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "699cb86079c16d7dab2b3a982af73cfea822fc3a6f2f0910067668ee185f9860",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "61e800deec87c29c11657e2cd2a30320bfb5aed8313c7ca5d30e8f0fc197ce65",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"psp":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f98a4545fe19b6189b530c2e23dbf65c34668e9ce80bffb8904daa29142931b9",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"rbac":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "198eeb26dafe6008714fd181d3f1cb345a7ec753aab59da173706d3434d61475",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"readReplicas":{"hostIPC":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6366adaecab47a91be0df184ab64c4717508e5c39e25004a7aadbabac2ddf175",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"readReplicas":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5a16545e8816cac994d717e59de3577efb3826062ac9e790032ef5f5b9f7487e",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"readReplicas":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "00ff0d43de745e65f4fa87c3196eeb04942e5e971ae83e82002e0ec6742eb6a0",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"serviceAccount":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8b0d61104ee65afe4a316a2a324debf68697a1725df1cc8ac9bc8f72d35918ab",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"tls":{"autoGenerated":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8ca04cf0fc8447e2274bdaf27944733245b2495557f913a1bc644f8f3a929eb5",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1899117ca20a21b1d5a831e523bfc24cf9230ee2204459767e42660c3b49bc6f",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "fb6cd310add2b716d0f1f34dfaddd802a0bf5476aa17c3741de2e7fcd65d58e1",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "46b840a34dfffa93fc69d06cd792ef26602cc0ea467d18fd4d41b1d17de0b40f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2ab95ecaa9cfc0c3a4eb5ef98d8f7398ebb6cd43f7c164d2bdcf3f1096aa41b6",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0e17ad3087e6ccb8fabde25fade9373058646399a5cc600251408d1c66660ecd",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1f81105527cccb13e752dba9ad9b2ee2040e6ce3020a3edd6b77c5f0bd2f820b",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "05f4093f8f05fe5a15adc4f431353a8dbd4fc26bbba1a22d19af2ce8d99b2492",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"alertmanager":null,"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f5bee2b3bb041d39d4c66c86cf4177ac1b7ee04f7400f8efa8c6d2724334eb40",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":false},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5f0eabd3a855648f0975319fe9af7c5c137dc89e9167ecf6ab5018a51ed1de42",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":null},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "fe1e4c92f7504c57d7c24b5111adbfca0e2233eb0540098f8439b54fe697c8a0",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "eba9357da953e8894d7ac605e9310e058db1b4852502f5cb5c5f815b1688cfa9",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a56932eb7627a39d0ab83e67510de774e75758a0d96fb4790dfcef11a317d1e8",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "496edcf88a823f53836a0eb681050ba62be801280a087c0cc611062ce0ab07f5",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "12abc7254b48631cea8cf48fe206669e9936e8041b11d02dda0bdd0ee4844e7d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "24c622bab3fcb0bf6c569bd894b8f341bcfccb957e712b8cac5c20363da02bbe",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":false}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b352b0e897461324e2ad50c0f2da1324c93dd8bff3572d8b740a8511e437c7c7",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "27d94b46fd097664c0384c7ad88293c6f5b9e4dcf60208ad5f2ba294121700b4",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "04ea2574585863762e203a94cad8e095ab6f8161afde98adabcf1d43771a9ae8",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0ad2f2d5c06bacb4b41c3eb0d2c1756e1b37e8b924316eb473cb4327ef249c1d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "de1402c514a7b71504bae69d59dcb892008fb6ce75ecfb030658f69219423f0a",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"podSecurityPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "76de3df90168751df054b5fe84b712f5b85a3c1fc6838a0b41069717a4fe297f",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a2025d2f08488874074fb7ab5ccefe6120780910b66689ee3e9fb91d5d9a1bd9",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9d517396e741d4f9b110a5406caf7287a96478701b2adda3de971ca3a4fb4c94",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":false}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5a8f6f6fcb2bcb7f8e34e15b34dd6c4e88e40ead4d726896c9f02419da177eaa",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "722fd69db23cdc26cc59386c827bd825ea9ed2959a4c7c2ff4b77a1da5ceb7df",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "a7e4870a3f46d2cde6ddd5f16034708b64a9a5f1e22b2259c1787ad2476e1fe7",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "771d84b3eb7c58fd96f9d9dd7af35d01e4f1fff4a1997ea08847033fc802370a",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"rbac":{"pspEnabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "983ef502f0f4c0a4c9783cc5056dd73b1ab96c9dc82f90b3b269967a76fa5598",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "31726f85a35d6244acc097920c19dbc198a9c2e19ed9f9ce3242bdfbca092201",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":false}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "56c981398124d4d5565e977a9f4e7d1f5ada1b1bf08e7aab9135f462b064b625",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "bdd2e58297b20add31ab0f0f9d5e36fc47b1b184dafd3bd1c6ac40013e8170db",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "909adfba8dd0747c16c4c3bc4b70591423d308d2d91ec62bb3e24f035364915e",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "410861228075e72da9112b121fac007fae31c90d66587bf7907c937a59590392",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c712c4e55c162d2715c305611233c2631aa6cf5d6b4173001ce74d63fffd4f56",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"ingress":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0dde9b3a2383fb54a9f6409a7f1c91743c5cefe3a05fa625a1c29dd196e931cb",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"podDisruptionBudget":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a8ceb2128117685be51ef07584b9e677f74f99bc206db4221c7ea4838641c5f3",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"releaseNamespace":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1215d35cb7dd6713a9554fa9ed6c38d6688fa5e909e1ca0d6a77c4cde9f0561e",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "53d80cfe455a7d3f873dc4b0b3f58e80344330a3987c8585f595cb5fbe91c345",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"statefulSet":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5857e26b7a1a75775bb5c7b965e0e04ee047f2ae455d2285cc6d312bd30666f6",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"statefulSet":{"pvcDeleteOnStsDelete":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e6df5caf65b93305f72608f5db3486ecdc51551a258851b30dfaf4a7967545b4",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"statefulSet":{"pvcDeleteOnStsScale":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3c72296a89de093bf60a53242946109f86118c9465f2813e1e8bbd9c92e35df1",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"tcpSocketProbeEnabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "194dc17b62ee9358c2711353b8bc445dfeb389c4a3abd072b337a1e0ad382925",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"verticalAutoscaler":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c222e7e589cfb21f0339aaa522d827939c89e4df520414533d2c6b6644ba8a7b",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "15c5df79d49de848903ef40f8245ae51672cc06c9b1bbe133f82393cdaaddb25",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5a160dab0cb9f4fff7011e0d0139a27b518dc1989047df849514930c1b2a69c1",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b0bb7f304a629556852998ded1b51874f99093e3d1157ee3959fddc791ddeafb",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"auth":{"enableLoopbackUser":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "056f396de2ae87ab7c454171b4c7fa7b8c99f965f6daec784d46c094eec31c7d",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"auth":{"tls":{"autoGenerated":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ce40db2cb01e01516cbb1a7c82aafbae78a37ed4ad0d59ee75bf76575d99b7b7",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"auth":{"tls":{"existingSecretFullChain":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3de2ebadcf07da8f293dbdbdb7d8a6ce9f82df563ceee5584a945944349bf11b",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"clustering":{"forceBoot":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d9780ed0c00b9955e318c6ef76e1294dcfcb7da2754822b8da41ba8a5e77ec8a",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"clustering":{"rebalance":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5861a01e09971b2373ea6c102a9a472d7abe47643be9bd848813543257d9063d",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "aaa5ce6f3293677436f1ff6dad21ef22209de1f88905795f396bb3e0eff0350e",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "73fdb21ad77926969164bd4fd3dab422950450e58a0822be944daf131204de50",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"extraSecretsPrependReleaseName":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3c7cdde0ef1f6971d65ffccd543babd31aa619cbb0c6fd280a8095e6b6c8c635",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e53237be9a5eb436813fcdbe3bfd22babf656134e10f4e984c3f031b80ed09ea",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ingress":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "09eb0a5233c54521dfbae9936ce0dcf799410de5ca47bdaf566750d62e98f59e",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ingress":{"selfSigned":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ed0d5a73c7f7270ebd804c241d484e68a192ad11f083d628e9d510fb8830b89b",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ingress":{"tls":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "20fd7fb1dc42ba3c693e32a71b918a7baee9b44f4acae04ede3100756372ee1a",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"authorisationEnabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e704f68bc3c0e715f6f8b97119dc70ec273a3e34c981f3368be3dc606b055af5",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c9a3fb48ca0b48053d5bcbd8d16575c8a5fb337c0ea27d634c3ebabaabd5ba87",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"tls":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8b54a9fc16336e0c3f2b0a8f44a55c663dd5154e35099351ec9b5427a66f9532",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"tls":{"skipVerify":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "356c30e7f4aeefeec4773a4d11a523ab81600fd67d254ed8469cda4e5cca9145",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"tls":{"startTls":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9aa1acb3cd413497295f2f7d1e30ab2226cc339224fd34d2ecdeab6c6a19ddb0",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"loadDefinition":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "05a2675e670ecb733211a7f9013ef9f82c5708e7b754207daf9eed6d48ccd488",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3a9e2033426ba62fa93d83b3dbd2d4dd1804b372c969ddeef1c3655e17703da1",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c5183eb0a80a199ffde0e6b753450832aba9bdb1f28574c8c0df1f5bb3f3c2e9",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ede4ad3aeffb6052a661b7ca1368a2d81dff7c816bd77390b2ef9b31555af026",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f2cb8d6e367bf0a8e747fd034da5476db8a6534bb29e91d8514ee4251b0fd143",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "590191ca5c5a3e60d88ea386849f07aba9bddd128d8b811371a9ac3ba27239d3",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"pdb":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e9d0bfcd425ce4eebee17786cdbbb0cc60bf4926c04c3c9a7cbc1138688693f2",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"serviceBindings":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6f660ad8eee575f4df33cf1b7ddb2e79f7504ba4618ea2746ccf203207c68b61",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"startupProbe":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c7a26202fd56fd813067c53b6505c0cdd332a85c398c4ea00608758ad2ebee19",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"tcpListenOptions":{"keepalive":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "74b7170ea7f6dac766b7b2fc87b0c9cd30d96ef67ade53fd9f0398e709a05783",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ab74c245d6e0456919432369d96900e0da11e48031a2fa54ced4bc656f63f752",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b913222abd4fdba7b3ef695c78a278ffa993855b1311704f11c200f7ce13f6aa",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "46b840a34dfffa93fc69d06cd792ef26602cc0ea467d18fd4d41b1d17de0b40f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "da503517ae3f408c816ed877a3114a85028661d42ffd8bee2edc962b87aafda8",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "164777841848d150d1be80764a9352dcec1e8d8a79d6ee304e852a75eceb697a",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "712c96a12b282107f20c886f7fe53728192609920968c06eae18627ffdbd48ef",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2bb3a321e7281fe270b0338c53fab0b5e709fd38cf00900536ab9dc7ee82a6a8",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"auth":{"usePasswordFiles":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "05b68d3f6e0c88e0ac6720370a9275da15752148649b444d52a80898ac883570",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ecce95fc85f6f3bb74fec28a361e8802f780a261890d3fae345068dba740a028",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "cfd926126a7a4df605b20ad4a344eff1a7e1d7a6b3c1194dd7d7638887da0749",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2858d872827fd2f106eaa4031a389b83b3113df5cb0fe3d02e76fada44c94d7a",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"master":{"shareProcessNamespace":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "fff12f7ad4fc71d3d02f2d26a98bbc0d7a40621e5e82f4790dce194ac345514d",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"master":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f05b26b9fe4befeddeac9065959e4b142e83e9f51f97ef9bdc38c8d2f2b11a84",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9be72cbd7c6258ee140fa6090a130eccb905e8e5fd58875d6433bb71a67ec10a",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "7dcd5ec90083518534edfe5d964da20b2a40f97cc0b228a4d1db3cfe3e3fa1af",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "094f77548683dac536a6f440ef9d505cd38e3dcd53586f6cfa7ac2b86d8aee89",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a3d829ef200d392ec0ce5ffaef38dfb5989f7ff6d8ab045264a0f3c43d54f91a",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6de137b03441b03bc01ab9e0a56256d7b61ba4f0f10fe99ee2384f4d801af944",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"pdb":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5cf2ba13c76e8e882a031bfe3d37bfca38be5a930b62e996a553b900ae1a9ad0",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"podSecurityPolicy":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "cb2bf7fd8164a5f803b484bde9411ecc7bb387a5cbb5f37609c8e2b472cc9836",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"podSecurityPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "bee2f07f3ca7e6215999a329b9dd428049047678918043ca82982ed67402c6ed",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"rbac":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "70150cd1b26256743d7c9f2dccf0fd50ef4b163c3b2accd79787aa81a60e66f2",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"replica":{"autoscaling":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b3915d398d1bc729dba2c83e1f8484920ed37119858582f58a0f5437d31c2212",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"replica":{"externalMaster":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "51fbb5d857f7727f7c4b3eeacfb6d88554e1c6b35a0590c6978ae07e98352a33",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"replica":{"shareProcessNamespace":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0d6f0ad24bc18db296fa609926c335704fbc9297bfb3d645e7ea1e77019231e9",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"automateClusterRecovery":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "13e73ff88669ad75b19795e7045286bb8a0377c65f0a56d01626cf70b6328d1f",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6cec4c03cd4f1ac92069936a462081ba7bf572db2ab6a1f48c1b0c43ab4afb5e",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"externalMaster":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "83b8183473eb5f2a11ca49e90605b1c03359807575c3d99d77061f1ec32776cf",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"image":{"debug":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "824860ac3d703c1c88438a34e447d1df17109af9faadd3693b94a307474b6845",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"persistence":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "43433a509f3b8b93f5a49577ebb7ce0498fd6e13c8af3711eaedf40782905c53",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sysctl":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2c6736b889f7d69240188cb6b4232d6cf5217341e6c8c58a28b74a9ed67022ea",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sysctl":{"mountHostSys":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5653ef7eaa403de73dfc9196f8936983da80d7f2e71826d73b35dad8efe338f5",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"tls":{"autoGenerated":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "550e873a580a0621cf51a1a371f9df253b42640435157796014198f35f9cc706",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"useExternalDNS":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a605b4507e1393632ca116c6b3a72edaade52884f0c6231cc1b7f9de47fc2878",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3c57bf0662cd00224d90a959e29d74baf5f245562b93f9776c0c56a0f435fb8e",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "14256d213ac9af84293b0aa642922a1334302ef5c0b054a1a19321f4c6404cd3",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "46b840a34dfffa93fc69d06cd792ef26602cc0ea467d18fd4d41b1d17de0b40f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6462b38b59978e43b0ab9d527127324118f9f4ae6c9a8b3732a1b066247412cf",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "284b18ea1dc195ee2a0675ba6f0de42863652b16b56c5f6349cb60bf6460032f",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5f8ad3bf77a36dba453702269642cc1699bcae2e5f3d4229c8a0d9f30850a910",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: "{}",
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6f201d1c3e8aed5c5b019fb96e6d48410cd7a0974b6881af728ed42eef4fd5d7",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"cainjector":{"containerSecurityContext":{"allowPrivilegeEscalation":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "702532ff5a47406587a047cd2f14cbb35c5ec5b4ed671bd7f8b6a9ae4d417e0d",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"containerSecurityContext":{"allowPrivilegeEscalation":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a951718b40e190eb0ad1f6a05b8ae66629ad997fe4b3127cf047a7fc3a99ce31",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "608bc34f48ec247a0ab86e64a71adeab60f1fbb9a92ac698fc9c0d802bdc886d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e9305997f23029b3b4169365ab96e678e57e3eb66d217b27082e0056a9c57d7e",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true,"global":{"podSecurityPolicy":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "24f791f3e1c62c25842c07efceea4363d25e5db913386b7c081d68c165170c8b",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true,"prometheus":{"servicemonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c8690de964b690a49b35ff272ead0641e40dffc0fa8eed098059ed3beec9fac2",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true,"prometheus":{"servicemonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ef2c765ca0ae9226fe4c8cbf9f294631d602f5b339fe9c04c2ec3adc2da3e468",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true,"startupapicheck":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2b938f593a266f6e7e51375d0a3b191c045838f16eb9e9e3f87273f78dfe84ab",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true,"webhook":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f109d0953208ee12d1e313d7b57efbc555c16ed60fefaf91fdebd3d3ff0078ee",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true,"webhook":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "4fc5d59ef2a8b4f01e80b268b631d0d4e06db5ef6dd9534352fc5c231fca15aa",
            ),
            controls_sha256: Cow::Borrowed(
                "b1333dedf83af5e126e254439efb761ace3de291b90ae7218ed4af25015e83d6",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "61ff8c3240c55333cfe4019cba3bdfb594ff0e2f33d7f71423517964ff9907ad",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f244a76b0ceca403bbef0fe788a25a01dfbaa489a000e120942e3250d406e150",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"autoscaling":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5b21f13d208509af14b6f77882c42a40fe7ab3637eea9e8e9a1ca5903bc172a7",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"containerSecurityContext":{"allowPrivilegeEscalation":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "35963196b2813fe4eb5416768548744ffc1cb8d744214cb0359e4b97a1199114",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"downloadDashboards":{"securityContext":{"allowPrivilegeEscalation":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "31974cbad09f653d5a1d5864ea891b5cd020403740fd6261fe5412d0607cb794",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enableKubeBackwardCompatibility":true,"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "006ff55b3e84d914f3998ca7990b0c0b0f657823a433e929c2c61c693acdfbf4",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"headlessService":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9687e72eb1fe72b2722c06280385cbe27e96721316c1dd4568c5768183a68ba5",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"autoscaling":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f9f1ffc064b9c5fa0533453a0294c8b15aa926873da9efc77d773b71e8e09bb5",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "791a03724689f8a9c869bc03bf0cf3a4f9891b4f9a08238b2794ef713c9162c4",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c285fb0a53f4d485319248b68dd76e8b771bc8c8fa895246d4d87ca6f7d1a941",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"networkPolicy":{"limitEgress":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "bd1f7205a8ee8f7198c1d4b4c1f43b9b26fb8eb328649c80df2fd44c9dbcc6d2",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "cff3a83e30b6e8494f1b6929b3830a04b04f43f6b4335a73cc3319669d842abe",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"ingress":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e226d1dfec23f779d16585871367ff711fef49476545ceceffe153f6aa593ca4",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"initChownData":{"securityContext":{"runAsNonRoot":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "82ccae09888d183613d7cc34d22a58ca99f96c509d37bfa8d83b6a11ee862745",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"ldap":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ae08aaaea5268ed12e01db3fea609788739827436d853de230b2c81c49d762bb",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"networkPolicy":{"egress":{"blockDNSResolution":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b519d208d8f639b77aa533f1576ed1fac39a339e78e3237d48b0c92c6434adef",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"networkPolicy":{"egress":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e1f07002820618cf678e5b57e5bfd4026267f66806d5a8fd0ae921208b5a75bc",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "08490198d8d74cc5fc055c7780df782efa292b06c3ab868cd6b3abf00077fb9f",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"persistence":{"disableWarning":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "65c908ea4a5b18023db7b589ab7ea4ec468d7230d64afdf9fb0bdd7ed1bf0d4e",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"persistence":{"inMemory":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "335802bf09e6de1e7a5b4fe13203df1cdece1f3b6011d820f993b49873196b56",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"rbac":{"namespaced":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "bed9df24263a975e631c7e0a9a6e0fe6262fcd393a2d1ccc7db53e296018b448",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"rbac":{"pspUseAppArmor":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "de57968f7ce7988fe4893ee7b450823164d839b647d8d844edf275479db15fad",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"serviceAccount":{"automountServiceAccountToken":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ff009adab232af70ae418e05104fd11f27cca70d840856422d0469921b3d92bb",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"serviceMonitor":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8a09d4d1637597b2e6c2bc5efc35a563841448eeeff738ca4b98df396a449891",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"alerts":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f2d03704a2f79f09282669c12b24562599cfc6d60de6e050edcffd34d75b43bd",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"alerts":{"initAlerts":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8a92e54ac9cc99cb50a1958ce337651844168e77b9242f9c7e195f3e4d8c2240",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"alerts":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3fcfea54b0a60880465bf30481df5774f61da127880ed234da6ee17ba76f0b90",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"dashboards":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a596eb280af490192efe0054befcca3855aee1d141c1df8273b0c09df124c263",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"dashboards":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "edf0887b89b1b5870a7682898353fddec8bae9bc975083ac4527c367920d7eb4",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"datasources":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f22b32ce28b6933bcefb1848781006293267dd7df2351ef3c7ee97be85bdeeed",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"datasources":{"initDatasources":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1c16810214cb08f984ec858ef5a51f02d140fe7e49b6590b6f29e3e1751dc269",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"datasources":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "cb7de90394b00afde5cafe5d9d71cb4e863ddf779aec240f17df548d0936ea03",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"enableUniqueFilenames":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2e48697070970eb89af5197e0ce7a480fbd073776083317fc1b8c4f86c7d01a8",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"notifiers":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "83743c27a6e6666dd92dc74ad9204a5edfe5eac52b44941718341fc1465ca220",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"notifiers":{"initNotifiers":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e49ab02956f05df7e596adccfaf75eb7531d95c3905ea85c94f4fa9f5a4d1e90",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"notifiers":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "918880a81cd6c0c200cb6e5556424c55aa60117a230b17b9a4c1d0c48b22a8ca",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"plugins":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f4c43e8b36528225a428246010a5cab9191a79308d86dc261cf1843e3e400fbd",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"plugins":{"initPlugins":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "05d6c4b1713b6b257e9ab1ef122cd255c63949975e73ad2ab2159d0abf17fd3c",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"plugins":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "482dc47192109a537499190b488f4f44f01848ad56f7422215cb3e1090e06233",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"securityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "babb9df691043f14c35fbd5c2a015720cb84c593fead09395ebb45b84c6f0005",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"useStatefulSet":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "4155dafe1323b5b5fe6f29c5509bada92c37711d4853fd37e33e734f9319f685",
            ),
            controls_sha256: Cow::Borrowed(
                "25304e0e22e88977a4e4f117ba8a81b40e7229b308f5666463ac7d4a2391672c",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "47eb7dacdbbd622cb6f19675e9825d84e71a069b674c6b2556dfcda09e131b00",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "aff189457f1e81b9da9a38e06a3f3254110176a3cbf8112b6a75486ed651cdf2",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "042a1fa1523491069bcc2292abd0a334ab2e2c89e0d7b37e755dc618b9a02d65",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"autoscaling":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "edbfd1afcbafa6c8c4ad83006d538a5ed9efa7cf17920a4f684c34cb962a1484",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"hostNetwork":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "286e3e041cb9d3f4ff5af635a5f423f545c590c47f08df439fa8dfcae743a7ef",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"hostPort":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2d9ba6a18f422c456360f0d2ac4f4954fc582d0889a486f2dbe258e1993da49b",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"image":{"chroot":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "08fd02be7a23e0a1465142cccfb1e7125b97cc9fd74a7d2d6bf3c533d30ea5f1",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"ingressClassByName":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b2f87db54ece31a66a78cc390c6d0008aae0dd90faf5a6c217eea54a48973c26",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"ingressClassResource":{"default":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "96509d4d41eb30a25d7061d49f54d45d5ddf41957b90d6b5846fd0a20d4bdd17",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"keda":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "72a1ac5d14f2265a5139f866ec0793a9c292c874a0811ebd8a62bc2551d3ee80",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"keda":{"restoreToOriginalReplicaCount":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d9108e13a820ada3d4f99c1e13bbec412e6d755a628eb3cbba31b16e3045db7f",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"metrics":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9430dcef0a5ef59c013be3625fca8b5904acdd7019c53b925353d6d93fc0fb44",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"reportNodeInternalIp":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "011d72f886ee9e7752732563a296222e100c4cf116ca17bd50f338984a7c5fbb",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"scope":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c14c36dab9fbea35fd082a65235bea9d605829e45a9de878028bc5aa67896c4a",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"shareProcessNamespace":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1a51459c8ef4d5da7b75056310df79fb9e0070a2fa7d9a6fa7557d660a263215",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"watchIngressWithoutClass":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "219e4d343b5db1eb9c4a8d39726b23449b18adfe533027ba3a7fcd5800f99696",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"defaultBackend":{"autoscaling":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6b2b59a2a715764e01a903355cee444a81b6740fa52f51ae42b7d8414d26e73e",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"defaultBackend":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "56f531dc695f7216f93da38d709857aef03e20b5cac6aba2c5e03a0e85eef5fb",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"defaultBackend":{"image":{"allowPrivilegeEscalation":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d51532562494e701f9217853b6547fa0b36ebbe41ff0809c073e0da06b190280",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d46058f4059977e17f8171132d292657653015bf14bd8a91f0fe8d69f4073e23",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "523c37e0b7ba90ee010f77a0ea493bddb971008a1d56c7bbfa93770eae69cc02",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"enabled":true,"podSecurityPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c63387221c74b708a8cb5703c091dd4b1922e4646eaaa07888fc27695c1578a9",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"enabled":true,"rbac":{"scope":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "95c4e03104f893afdcee950bbd4ecacc8a736bddf04bee2998b40549ae4c16c4",
            ),
            controls_sha256: Cow::Borrowed(
                "e46948c9ee31c8f6bf8e85db569e8fb02fa17b6c2e145b05dc2fbf83c7f51815",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "77fc1edd8701636f59cd08b2783e9ec526bdd40f4648e2569dc1ea9afe0a29b9",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c53f4c07c1fe2370fbe610b3bd3c4e0110da16d228718fe92e77c908cc072245",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"auth":{"usePasswordFiles":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b81a3fc26c7691c31609b4d0b25858c3bfcb81ffa343c5f025ff5e8ac5c2cda7",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "849186c0b9516cde8ff01f876f5de5495d1821785cf9595106cfa06aaeb105a3",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "642c940e1ddd1c31b8b8575ae4f38318bee7cde65010401035de0c8d448aacfd",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a809d20816de2899d1583c3473a8fe8d12683afa74050eec80a8ab11d778c01d",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"containerSecurityContext":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9c6057cdbd0edc8cbb1ac05fc71df4cd8041b9f5f145173784109c7944ca5316",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"containerSecurityContext":{"privileged":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1bd214e22f41684fa5f512000978ca3ae3d2fcb2e0981eb8b3f361b5ae37c5e8",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "4162f62fde3c5cc2947c5a2cb27b0790ae16713692b79e5b2c0b1ea75901504b",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d9574fe8a432f3a9e96c7e57389d63854d39edec7f83f43c7045847d4eeb0d0a",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "822329e9572ce854c6775177e6bf1aa9ad2755f91fdee250bf8e883e6d79f8d7",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "61f81ad69e550487b690d438b79b05f6268e15972c9d4a8bad593710efda0dc1",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"networkPolicy":{"egressRules":{"denyConnectionsToExternal":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b6aea96d6d1a5fd10bd40861de5a84aa394d0fd29d7ca9b7074a80cff8ba77c0",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "eff053c8184962a7d62bc57d73771685ff94c9a4c08b18c6553b64fe62935dcb",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"networkPolicy":{"metrics":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "961e95b9d84cbbbaaaa4dc71c541ef47d484e7d63eb1dd2b343e0479c4459dbf",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "7e8ce287cec6c9817a9c9f52c578538aa4d1ee1e45e26b3e41e3abbf2930e6a6",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"containerSecurityContext":{"privileged":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0a05ed7150e961994f5df36994645fee00f283b63f9189324af83614bd741e9b",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"pdb":{"create":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f6a60bbd6635ee41482fcd70fd0eb008572216595274295b8ae8e554ea72778a",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "36814dff648fae4274efcb04c4e678e1e94a1b436679eb9837583db43cfc28e4",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"rbac":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "58a6fd01054ec675414afa662e4bc00a1d4f4751f3a6ffbd85598ac1a02c4fff",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "84afc646d3ceaa06f1455fa69c15f8b04a846aaed1ebff74242d5ffce6f08b31",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"containerSecurityContext":{"privileged":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "10e617fd26e5771f5bde6a8765fdc542d0569155ff0e7e2a6d86e1b76a1f44bb",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"pdb":{"create":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d0393faa1d90c3669d8f7f67d39626fb3092fae1ac1d75072fd71a0d69582fcd",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "26331ba6a1cbc7771497c20e3e69b9c966daa34814c6e5cb6243f951fec50d6a",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"serviceAccount":{"automountServiceAccountToken":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "86331fd8de88974214e9c150984ea502c2a460814830cf0ae20a8dbe7b39f160",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"serviceBindings":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d71c21de3462f5c910e355ad4dd32d16b2163c5ebf1dd99ec19ed4d4ca3ca865",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5525c76cd216928b8f2384a1c67b30c2f4b607a00f86c57b6cd02af0e7dcc447",
            ),
            controls_sha256: Cow::Borrowed(
                "b4749eabece7e6cc5c436bb0314c428d7a2a02b02e1df579aa09c2386cf42c39",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3a14b1a7c779b8d4c463bc18b6bad1c07e3e8e8cfb171d5b7bb3d7df381a4155",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "46b840a34dfffa93fc69d06cd792ef26602cc0ea467d18fd4d41b1d17de0b40f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "24d00d97a13c1788d5f46499d667067485f6d826eb8ba4aa34c5770a971122fe",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6837282308cbeab82e2f3c7ceb1c799011671918d785b92781523c5e6790f308",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c33720c5d09075601b15d88352cef8287d4b56d83e7c8c05bc51b90567df3498",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "629092af7b433ea1801e058abc9a3a9e7e8d7490c85e0e0ce653a09cca8cf971",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"audit":{"logConnections":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "77281cd4d35b3a86dd6e1dff1c8728acd0be30e514d2940ead0b22e6e41810f6",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"audit":{"logDisconnections":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "99ae66e6d675666d225ed5350a79a29f8e9cfa6fccd98ce9f3a81159e470df36",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"audit":{"logHostname":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "86b2f125c5ff676d84fde4773acab5c86ffd59e3188a45eae614924b49171901",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"auth":{"usePasswordFiles":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9975888db9eba8fc600aa0088f055f82113b6f3882795258bbc92b56a7d3ac15",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6845aaaa9fe980fe2faf99f87e69be04a6d49695e1b910e0e4b3519d368a0094",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8791fdc87c59e560d7f3e3c1e166568c450a532f90a26843403bbc0a367b8654",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5f3c69e53f6015097ba21f93ba303a6f80a7a3a06c3b030b1c67ce9c5363b176",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e0bb171606d5b8acfd3f066c181df46b04cf80e2238928bbe050fc05fed722dc",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"ldap":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3868265171ecf7649a78c8927951e1d233106812d2ea0e8e31a42594296ddb2e",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"ldap":{"tls":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "13311de3fa774b53a152f972117191dd8bdc5c8952bac8c0b17c8b750499a7be",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "fb7297ae8a14a84bc2183c7eef34c5f7e784981f7bd0bfcffdf6613fc4465bdb",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "62979209353968f3542fc40521797e77dfdc9920f403722d9ed0ec43104f7620",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "32d1bbf527a0c0215a9c4c794f277e7d03f9c377ff81342e6cf94c97faccf15e",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3e2e01cd72e50c2abec92b2a0341c9e52c89dc1d6a681e436750873ff286f69e",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "baab9169020fcb57757c48ee54d5767345de2f41632ac1ca6ba08ecd17a935fd",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"networkPolicy":{"egressRules":{"denyConnectionsToExternal":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d209a0f6bd4a1c94111f05be50fe52fc4227e6cda7da177244ada120952a4362",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6b2ebfad45a1efff35ca8d04e060828ffb66e33dc56caed2d4fd488e06636e9d",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"networkPolicy":{"metrics":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "cd608ac34ccb64baeeae80c2a2e9a222a3a62dd56bcc84999a2dd8318daa5770",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"hostIPC":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "85f4e09928a60e1d76cce80cd28c0b5bfa742489f88c686fff7c5a9200bc43b4",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "374a2b3b46a5ee497f56cef66a4f3783d6ff9551600b2b023fe7ee6f2b889a53",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"standby":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "699cb86079c16d7dab2b3a982af73cfea822fc3a6f2f0910067668ee185f9860",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "61e800deec87c29c11657e2cd2a30320bfb5aed8313c7ca5d30e8f0fc197ce65",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"psp":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f98a4545fe19b6189b530c2e23dbf65c34668e9ce80bffb8904daa29142931b9",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"rbac":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "198eeb26dafe6008714fd181d3f1cb345a7ec753aab59da173706d3434d61475",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"readReplicas":{"hostIPC":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6366adaecab47a91be0df184ab64c4717508e5c39e25004a7aadbabac2ddf175",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"readReplicas":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5a16545e8816cac994d717e59de3577efb3826062ac9e790032ef5f5b9f7487e",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"readReplicas":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "00ff0d43de745e65f4fa87c3196eeb04942e5e971ae83e82002e0ec6742eb6a0",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"serviceAccount":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8b0d61104ee65afe4a316a2a324debf68697a1725df1cc8ac9bc8f72d35918ab",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"tls":{"autoGenerated":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8ca04cf0fc8447e2274bdaf27944733245b2495557f913a1bc644f8f3a929eb5",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1899117ca20a21b1d5a831e523bfc24cf9230ee2204459767e42660c3b49bc6f",
            ),
            controls_sha256: Cow::Borrowed(
                "a4dd7074f04e4106ef5e00405e2981b928e17d135f9e2cdd9e91cc4be0d3bdf2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "fb6cd310add2b716d0f1f34dfaddd802a0bf5476aa17c3741de2e7fcd65d58e1",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "46b840a34dfffa93fc69d06cd792ef26602cc0ea467d18fd4d41b1d17de0b40f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2ab95ecaa9cfc0c3a4eb5ef98d8f7398ebb6cd43f7c164d2bdcf3f1096aa41b6",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0e17ad3087e6ccb8fabde25fade9373058646399a5cc600251408d1c66660ecd",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1f81105527cccb13e752dba9ad9b2ee2040e6ce3020a3edd6b77c5f0bd2f820b",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "05f4093f8f05fe5a15adc4f431353a8dbd4fc26bbba1a22d19af2ce8d99b2492",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":false},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5f0eabd3a855648f0975319fe9af7c5c137dc89e9167ecf6ab5018a51ed1de42",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":null},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "fe1e4c92f7504c57d7c24b5111adbfca0e2233eb0540098f8439b54fe697c8a0",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "eba9357da953e8894d7ac605e9310e058db1b4852502f5cb5c5f815b1688cfa9",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a56932eb7627a39d0ab83e67510de774e75758a0d96fb4790dfcef11a317d1e8",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "496edcf88a823f53836a0eb681050ba62be801280a087c0cc611062ce0ab07f5",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "12abc7254b48631cea8cf48fe206669e9936e8041b11d02dda0bdd0ee4844e7d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":false}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b352b0e897461324e2ad50c0f2da1324c93dd8bff3572d8b740a8511e437c7c7",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "27d94b46fd097664c0384c7ad88293c6f5b9e4dcf60208ad5f2ba294121700b4",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "04ea2574585863762e203a94cad8e095ab6f8161afde98adabcf1d43771a9ae8",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0ad2f2d5c06bacb4b41c3eb0d2c1756e1b37e8b924316eb473cb4327ef249c1d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "de1402c514a7b71504bae69d59dcb892008fb6ce75ecfb030658f69219423f0a",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"podSecurityPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "76de3df90168751df054b5fe84b712f5b85a3c1fc6838a0b41069717a4fe297f",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":false}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5a8f6f6fcb2bcb7f8e34e15b34dd6c4e88e40ead4d726896c9f02419da177eaa",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "722fd69db23cdc26cc59386c827bd825ea9ed2959a4c7c2ff4b77a1da5ceb7df",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "a7e4870a3f46d2cde6ddd5f16034708b64a9a5f1e22b2259c1787ad2476e1fe7",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":false}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "56c981398124d4d5565e977a9f4e7d1f5ada1b1bf08e7aab9135f462b064b625",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "bdd2e58297b20add31ab0f0f9d5e36fc47b1b184dafd3bd1c6ac40013e8170db",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "909adfba8dd0747c16c4c3bc4b70591423d308d2d91ec62bb3e24f035364915e",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "410861228075e72da9112b121fac007fae31c90d66587bf7907c937a59590392",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c712c4e55c162d2715c305611233c2631aa6cf5d6b4173001ce74d63fffd4f56",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"ingress":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0dde9b3a2383fb54a9f6409a7f1c91743c5cefe3a05fa625a1c29dd196e931cb",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"podDisruptionBudget":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a8ceb2128117685be51ef07584b9e677f74f99bc206db4221c7ea4838641c5f3",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"releaseNamespace":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1215d35cb7dd6713a9554fa9ed6c38d6688fa5e909e1ca0d6a77c4cde9f0561e",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "53d80cfe455a7d3f873dc4b0b3f58e80344330a3987c8585f595cb5fbe91c345",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"statefulSet":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5857e26b7a1a75775bb5c7b965e0e04ee047f2ae455d2285cc6d312bd30666f6",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"statefulSet":{"pvcDeleteOnStsDelete":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e6df5caf65b93305f72608f5db3486ecdc51551a258851b30dfaf4a7967545b4",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"statefulSet":{"pvcDeleteOnStsScale":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3c72296a89de093bf60a53242946109f86118c9465f2813e1e8bbd9c92e35df1",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"tcpSocketProbeEnabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "194dc17b62ee9358c2711353b8bc445dfeb389c4a3abd072b337a1e0ad382925",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"verticalAutoscaler":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c222e7e589cfb21f0339aaa522d827939c89e4df520414533d2c6b6644ba8a7b",
            ),
            controls_sha256: Cow::Borrowed(
                "99defa1074537aa02da2790fc85a21dc0d29de175daa318e2b6bef2f7c8d2382",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "15c5df79d49de848903ef40f8245ae51672cc06c9b1bbe133f82393cdaaddb25",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5a160dab0cb9f4fff7011e0d0139a27b518dc1989047df849514930c1b2a69c1",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"auth":{"enableLoopbackUser":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "056f396de2ae87ab7c454171b4c7fa7b8c99f965f6daec784d46c094eec31c7d",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"auth":{"tls":{"autoGenerated":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ce40db2cb01e01516cbb1a7c82aafbae78a37ed4ad0d59ee75bf76575d99b7b7",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"auth":{"tls":{"existingSecretFullChain":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3de2ebadcf07da8f293dbdbdb7d8a6ce9f82df563ceee5584a945944349bf11b",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"clustering":{"forceBoot":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d9780ed0c00b9955e318c6ef76e1294dcfcb7da2754822b8da41ba8a5e77ec8a",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"clustering":{"rebalance":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5861a01e09971b2373ea6c102a9a472d7abe47643be9bd848813543257d9063d",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "aaa5ce6f3293677436f1ff6dad21ef22209de1f88905795f396bb3e0eff0350e",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"extraSecretsPrependReleaseName":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3c7cdde0ef1f6971d65ffccd543babd31aa619cbb0c6fd280a8095e6b6c8c635",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e53237be9a5eb436813fcdbe3bfd22babf656134e10f4e984c3f031b80ed09ea",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ingress":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "09eb0a5233c54521dfbae9936ce0dcf799410de5ca47bdaf566750d62e98f59e",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ingress":{"selfSigned":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ed0d5a73c7f7270ebd804c241d484e68a192ad11f083d628e9d510fb8830b89b",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ingress":{"tls":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "20fd7fb1dc42ba3c693e32a71b918a7baee9b44f4acae04ede3100756372ee1a",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"authorisationEnabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e704f68bc3c0e715f6f8b97119dc70ec273a3e34c981f3368be3dc606b055af5",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c9a3fb48ca0b48053d5bcbd8d16575c8a5fb337c0ea27d634c3ebabaabd5ba87",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"tls":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8b54a9fc16336e0c3f2b0a8f44a55c663dd5154e35099351ec9b5427a66f9532",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"tls":{"skipVerify":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "356c30e7f4aeefeec4773a4d11a523ab81600fd67d254ed8469cda4e5cca9145",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"tls":{"startTls":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9aa1acb3cd413497295f2f7d1e30ab2226cc339224fd34d2ecdeab6c6a19ddb0",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"loadDefinition":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "05a2675e670ecb733211a7f9013ef9f82c5708e7b754207daf9eed6d48ccd488",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"memoryHighWatermark":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5b9e657cbf7b25ff97d60fa192acedfcb1f7c10d44b61d167586b1ad486ace0d",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b111866ac0ce37e877f9bd265e87818514f97c1113bc9336ce1cc5c8c8ad41ee",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3a9e2033426ba62fa93d83b3dbd2d4dd1804b372c969ddeef1c3655e17703da1",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c5183eb0a80a199ffde0e6b753450832aba9bdb1f28574c8c0df1f5bb3f3c2e9",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ede4ad3aeffb6052a661b7ca1368a2d81dff7c816bd77390b2ef9b31555af026",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f2cb8d6e367bf0a8e747fd034da5476db8a6534bb29e91d8514ee4251b0fd143",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "590191ca5c5a3e60d88ea386849f07aba9bddd128d8b811371a9ac3ba27239d3",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"pdb":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e9d0bfcd425ce4eebee17786cdbbb0cc60bf4926c04c3c9a7cbc1138688693f2",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"serviceBindings":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6f660ad8eee575f4df33cf1b7ddb2e79f7504ba4618ea2746ccf203207c68b61",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"startupProbe":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c7a26202fd56fd813067c53b6505c0cdd332a85c398c4ea00608758ad2ebee19",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"tcpListenOptions":{"keepalive":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "74b7170ea7f6dac766b7b2fc87b0c9cd30d96ef67ade53fd9f0398e709a05783",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ab74c245d6e0456919432369d96900e0da11e48031a2fa54ced4bc656f63f752",
            ),
            controls_sha256: Cow::Borrowed(
                "7ae333eac3cb147033a1b33f4546ad95af7d72e536db202a3ec91a5318645f5d",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b913222abd4fdba7b3ef695c78a278ffa993855b1311704f11c200f7ce13f6aa",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "46b840a34dfffa93fc69d06cd792ef26602cc0ea467d18fd4d41b1d17de0b40f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "da503517ae3f408c816ed877a3114a85028661d42ffd8bee2edc962b87aafda8",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "164777841848d150d1be80764a9352dcec1e8d8a79d6ee304e852a75eceb697a",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "712c96a12b282107f20c886f7fe53728192609920968c06eae18627ffdbd48ef",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"auth":{"usePasswordFiles":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "05b68d3f6e0c88e0ac6720370a9275da15752148649b444d52a80898ac883570",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ecce95fc85f6f3bb74fec28a361e8802f780a261890d3fae345068dba740a028",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2858d872827fd2f106eaa4031a389b83b3113df5cb0fe3d02e76fada44c94d7a",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"master":{"shareProcessNamespace":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "fff12f7ad4fc71d3d02f2d26a98bbc0d7a40621e5e82f4790dce194ac345514d",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"master":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f05b26b9fe4befeddeac9065959e4b142e83e9f51f97ef9bdc38c8d2f2b11a84",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9be72cbd7c6258ee140fa6090a130eccb905e8e5fd58875d6433bb71a67ec10a",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "7dcd5ec90083518534edfe5d964da20b2a40f97cc0b228a4d1db3cfe3e3fa1af",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "094f77548683dac536a6f440ef9d505cd38e3dcd53586f6cfa7ac2b86d8aee89",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a3d829ef200d392ec0ce5ffaef38dfb5989f7ff6d8ab045264a0f3c43d54f91a",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6de137b03441b03bc01ab9e0a56256d7b61ba4f0f10fe99ee2384f4d801af944",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"pdb":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5cf2ba13c76e8e882a031bfe3d37bfca38be5a930b62e996a553b900ae1a9ad0",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"podSecurityPolicy":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "cb2bf7fd8164a5f803b484bde9411ecc7bb387a5cbb5f37609c8e2b472cc9836",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"podSecurityPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "bee2f07f3ca7e6215999a329b9dd428049047678918043ca82982ed67402c6ed",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"rbac":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "70150cd1b26256743d7c9f2dccf0fd50ef4b163c3b2accd79787aa81a60e66f2",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"replica":{"autoscaling":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b3915d398d1bc729dba2c83e1f8484920ed37119858582f58a0f5437d31c2212",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"replica":{"externalMaster":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "51fbb5d857f7727f7c4b3eeacfb6d88554e1c6b35a0590c6978ae07e98352a33",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"replica":{"shareProcessNamespace":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0d6f0ad24bc18db296fa609926c335704fbc9297bfb3d645e7ea1e77019231e9",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"automateClusterRecovery":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "13e73ff88669ad75b19795e7045286bb8a0377c65f0a56d01626cf70b6328d1f",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6cec4c03cd4f1ac92069936a462081ba7bf572db2ab6a1f48c1b0c43ab4afb5e",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"externalMaster":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "83b8183473eb5f2a11ca49e90605b1c03359807575c3d99d77061f1ec32776cf",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"image":{"debug":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "824860ac3d703c1c88438a34e447d1df17109af9faadd3693b94a307474b6845",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"persistence":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "43433a509f3b8b93f5a49577ebb7ce0498fd6e13c8af3711eaedf40782905c53",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sysctl":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2c6736b889f7d69240188cb6b4232d6cf5217341e6c8c58a28b74a9ed67022ea",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sysctl":{"mountHostSys":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5653ef7eaa403de73dfc9196f8936983da80d7f2e71826d73b35dad8efe338f5",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"tls":{"autoGenerated":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "550e873a580a0621cf51a1a371f9df253b42640435157796014198f35f9cc706",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"useExternalDNS":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a605b4507e1393632ca116c6b3a72edaade52884f0c6231cc1b7f9de47fc2878",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3c57bf0662cd00224d90a959e29d74baf5f245562b93f9776c0c56a0f435fb8e",
            ),
            controls_sha256: Cow::Borrowed(
                "d5a0872a7da541b054665cd4fc7a84acbc33a4e92937b7ccad263d7028ad47a2",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "14256d213ac9af84293b0aa642922a1334302ef5c0b054a1a19321f4c6404cd3",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "46b840a34dfffa93fc69d06cd792ef26602cc0ea467d18fd4d41b1d17de0b40f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6462b38b59978e43b0ab9d527127324118f9f4ae6c9a8b3732a1b066247412cf",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "284b18ea1dc195ee2a0675ba6f0de42863652b16b56c5f6349cb60bf6460032f",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5f8ad3bf77a36dba453702269642cc1699bcae2e5f3d4229c8a0d9f30850a910",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: "{}",
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":null},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "fe1e4c92f7504c57d7c24b5111adbfca0e2233eb0540098f8439b54fe697c8a0",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "eba9357da953e8894d7ac605e9310e058db1b4852502f5cb5c5f815b1688cfa9",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "27d94b46fd097664c0384c7ad88293c6f5b9e4dcf60208ad5f2ba294121700b4",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "04ea2574585863762e203a94cad8e095ab6f8161afde98adabcf1d43771a9ae8",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "722fd69db23cdc26cc59386c827bd825ea9ed2959a4c7c2ff4b77a1da5ceb7df",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "a7e4870a3f46d2cde6ddd5f16034708b64a9a5f1e22b2259c1787ad2476e1fe7",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "bdd2e58297b20add31ab0f0f9d5e36fc47b1b184dafd3bd1c6ac40013e8170db",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "909adfba8dd0747c16c4c3bc4b70591423d308d2d91ec62bb3e24f035364915e",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"affinity":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0be176d67333b4b80bef9e14ae687e967013fff88d94d8bed630f0d887eaadb8",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"affinity":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0be176d67333b4b80bef9e14ae687e967013fff88d94d8bed630f0d887eaadb8",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"admin":{"existingSecret":""},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "15d8d1eb006b30ad0d57fa04e9e57e448ea411216e15c3dbb4dc0318bbfb0baa",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"admin":{"existingSecret":""},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "15d8d1eb006b30ad0d57fa04e9e57e448ea411216e15c3dbb4dc0318bbfb0baa",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"affinity":{},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1ac43f97481286cc8b34bdc3bdee7c406a389ab4c7a0e6d64dce00095929418c",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"affinity":{},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1ac43f97481286cc8b34bdc3bdee7c406a389ab4c7a0e6d64dce00095929418c",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"architecture":"replication","enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "24daa834bae01310d0e6e8c9422db24fee9b208bef6d75d176bf859b7032fe53",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"architecture":"replication","enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "24daa834bae01310d0e6e8c9422db24fee9b208bef6d75d176bf859b7032fe53",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"advancedConfiguration":"","enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "34d6200caec8d469456a519de96040f7aa5b660eae3bb4a1ef839688bb271c0a",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"advancedConfiguration":"","enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "34d6200caec8d469456a519de96040f7aa5b660eae3bb4a1ef839688bb271c0a",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"affinity":{},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "39e207a3f4098c22963c0cd0d62f42a7ba8d94f0edcaa71b67a8e1cbaad4065a",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"affinity":{},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "39e207a3f4098c22963c0cd0d62f42a7ba8d94f0edcaa71b67a8e1cbaad4065a",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"architecture":"standalone","enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6acc2a6d0485a78705005ff0187ef06aa0e8e0e0c5a45ee6bed2b7948b4c716d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"architecture":"standalone","enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6acc2a6d0485a78705005ff0187ef06aa0e8e0e0c5a45ee6bed2b7948b4c716d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9e24bb9a7178ee82e58583753f33664e0cc0ece50f4f6fee32e50e71a6fbc700",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9e24bb9a7178ee82e58583753f33664e0cc0ece50f4f6fee32e50e71a6fbc700",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"alertRelabelConfigs":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9fee834d81a10c1ce8b8255473348984695ffd22a78f1744c8c4defeedfe2272",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"alertRelabelConfigs":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9fee834d81a10c1ce8b8255473348984695ffd22a78f1744c8c4defeedfe2272",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"affinity":{},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a54da96925c8815af5f26f993ff3a23466f73f5608ce3d30b617c3b77eb98847",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "51da31307bc908154ee902fdb1dabead51f2a74686a40118aeb9fb24a2dec3dc",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"affinity":{},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a54da96925c8815af5f26f993ff3a23466f73f5608ce3d30b617c3b77eb98847",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"alertmanager":{"additionalPeers":[],"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a6952d7377b8e6b97103ce0536ee11c61f2cc3ab9ccf4dbebb4e5b411025909d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"alertmanager":{"additionalPeers":[],"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a6952d7377b8e6b97103ce0536ee11c61f2cc3ab9ccf4dbebb4e5b411025909d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a8f4d1f79eb404f0c2d0f39c5ace1f838be6a031a7710e3ec3c384a310c0fcfb",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a8f4d1f79eb404f0c2d0f39c5ace1f838be6a031a7710e3ec3c384a310c0fcfb",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"commonLabels":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ae0e35461c5149fa1b7db7d64b6326848a2047546bf421e5c31152757dd1b25d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"commonLabels":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ae0e35461c5149fa1b7db7d64b6326848a2047546bf421e5c31152757dd1b25d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b3ec24a982b9bd1b7d5ab09e923e7f29264bbcb13cd78f2334a6434aef9cdfed",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b3ec24a982b9bd1b7d5ab09e923e7f29264bbcb13cd78f2334a6434aef9cdfed",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"architecture":"standalone","enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d4834ec535538983864b34c0b94cfc0f72bcb9e48ff5fc5b47650b401c6d8334",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"architecture":"standalone","enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d4834ec535538983864b34c0b94cfc0f72bcb9e48ff5fc5b47650b401c6d8334",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f266033473cf3907a98f373c0aef6f362b115c855c5b8d2f68f3bf50e2e066f8",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "f266033473cf3907a98f373c0aef6f362b115c855c5b8d2f68f3bf50e2e066f8",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"alloy":{"crds":{"create":null},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "38051bf073805a1e3bea5925db615bbd25e15b325dac65d513ae507e966df4c8",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "36975efad01811eec10f010e7085ea537fedf86650a0cf9c55edf9d294c8e396",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"engines":{"local":{"rawfile":{"enabled":true}}},"rawfile-localpv":{"crds":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8e58e4364ed4e2cc9b05cd3dfddf39fbaa581353cd12553af21a32cdc28adaca",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "46951c786324b59a7be901c88fd5c56b5e01f54aa558f6bda22914e1596cab9a",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"alloy":{"crds":{"create":null},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "95e7b2dfbd3c58a828b9bc07502aad357e2419ad99082b8804b55279cb4f41a1",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "957dec03d18c701d1b591a4e8dbade02ee1cb29d553694c906829aa67fd3f0fe",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"loki":{"enabled":true,"rollout_operator":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ef0ae13604292e4980ef7e06044b9101210d8960682ce3b1fb12a4fb9a17fcac",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "2bc741c2252e860bcaa56454319d5c9eea7b180805294a4731efdd1a97430b35",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"loki":{"enabled":true,"rollout_operator":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2241a3e8f5513e9a8ce84322686d61f35af292f6456c2b65ee398859458fe473",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f3c2fbfb07677fe4400b45bce04db88e65fcddbfc6aba26b8508a908295f217",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"alloy":{"crds":{"create":null},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "38051bf073805a1e3bea5925db615bbd25e15b325dac65d513ae507e966df4c8",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "36975efad01811eec10f010e7085ea537fedf86650a0cf9c55edf9d294c8e396",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"lvm":{"enabled":true}}},"lvm-localpv":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "72e76f5519bf0709dd608ef524e5645b1d43f0dc7d7665a589fa64b31d9a346f",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b52d55f25d56121eaf4123f0a731945d1eb5380c93d993cfd0b008d9aef723d4",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"lvm":{"enabled":true}}},"lvm-localpv":{"crds":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "c9c2c00c1bd2523e7ab6c102c868d4b6d02f6ffd5191c9c4f8d08f12be061151",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b52d55f25d56121eaf4123f0a731945d1eb5380c93d993cfd0b008d9aef723d4",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"rawfile":{"enabled":true}}},"rawfile-localpv":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 47: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 48: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "55c1ae99482f2aa8e16175e1437353b1c22a3d8647730451927cb17438b63fbd",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b52d55f25d56121eaf4123f0a731945d1eb5380c93d993cfd0b008d9aef723d4",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"rawfile":{"enabled":true}}},"rawfile-localpv":{"crds":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 47: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 48: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "bf2479a90e309fe668a245d6804818d0cec811bcb57b82e2d464b20bcf378ac7",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b52d55f25d56121eaf4123f0a731945d1eb5380c93d993cfd0b008d9aef723d4",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"rawfile":{"enabled":true}}},"rawfile-localpv":{"crds":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8e58e4364ed4e2cc9b05cd3dfddf39fbaa581353cd12553af21a32cdc28adaca",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "46951c786324b59a7be901c88fd5c56b5e01f54aa558f6bda22914e1596cab9a",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"zfs":{"enabled":true}}},"zfs-localpv":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "49ed3f818e0842df1865cd7047c2367540c7aa9bfd5b2386104ff82702c45bb7",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b52d55f25d56121eaf4123f0a731945d1eb5380c93d993cfd0b008d9aef723d4",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"zfs":{"enabled":true}}},"zfs-localpv":{"crds":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "799921fa7bf5917f44d641504a0020cba8f52e998bef9302dca88ff6193ffe0e",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b52d55f25d56121eaf4123f0a731945d1eb5380c93d993cfd0b008d9aef723d4",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 55: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 56: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 57: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "79dfe0efb10510127f36802715b0bec9f26a980538425f88dfe191f735f7e69d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "5de75b623bd23b13a286d5f66e6afde0be89d40f1cc0be7345e601a461fd785c",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"alloy":{"crds":{"create":null},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "95e7b2dfbd3c58a828b9bc07502aad357e2419ad99082b8804b55279cb4f41a1",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "957dec03d18c701d1b591a4e8dbade02ee1cb29d553694c906829aa67fd3f0fe",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"base":{"jaeger":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2d11dcb2c5101a0dd04b101cc384d280a399b6821eac682c10588e9e993d9f05",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "c21b4dab76c589000b245b9fac4b0502dfcfa5fda1d147fc01c57626abaeb209",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"crds":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "5ed7c77b6283d98cbaf95777c41df8152608dd1833bacd7d1833d00ca85f240d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b52d55f25d56121eaf4123f0a731945d1eb5380c93d993cfd0b008d9aef723d4",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"crds":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "77b0f4bb75bd983666833ea63cbd1254f7b6a8a8ea540f64f0b946b07b15ab39",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b52d55f25d56121eaf4123f0a731945d1eb5380c93d993cfd0b008d9aef723d4",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"crds":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "22c94c6128d2d2e29f647bf039ab54f9bc283576f20d8bee2dfdabfe2163e3ff",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b52d55f25d56121eaf4123f0a731945d1eb5380c93d993cfd0b008d9aef723d4",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"eventing":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3c4ec2f4ec0a75eb8bace8376a66da063c207660b6b1b5f741b118918bf4dfb5",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "a392bee0f635cd000256b8a03dd1775aad7bfb2aa7d67d0f12db7dd300701890",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"loki":{"enabled":true,"monitoring":{"selfMonitoring":{"grafanaAgent":{"installOperator":null}}}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a3200475a56a3989ead8a111107ef182432ac9483ddf5ec37e4a7a988e02309f",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "2f21638f3a76c78a2abb53bb92cbd6d551a0f3f4cb24a8190f3c6756ac439634",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"loki":{"enabled":true,"rollout_operator":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ef0ae13604292e4980ef7e06044b9101210d8960682ce3b1fb12a4fb9a17fcac",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "2bc741c2252e860bcaa56454319d5c9eea7b180805294a4731efdd1a97430b35",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"loki":{"enabled":true,"monitoring":{"selfMonitoring":{"grafanaAgent":{"installOperator":null}}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "eec10e166b5c3a65b5f6574d4a0596ec9a6345f92f41372f7df6fbdaba7e603e",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "dc0f61e7bbacb937f2679c81990a2bcbb7eab973a43dbd48ffbd3984396b2fd3",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"loki":{"enabled":true,"rollout_operator":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2241a3e8f5513e9a8ce84322686d61f35af292f6456c2b65ee398859458fe473",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f3c2fbfb07677fe4400b45bce04db88e65fcddbfc6aba26b8508a908295f217",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"alloy":{"crds":{"create":null},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "38051bf073805a1e3bea5925db615bbd25e15b325dac65d513ae507e966df4c8",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "36975efad01811eec10f010e7085ea537fedf86650a0cf9c55edf9d294c8e396",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"engines":{"local":{"rawfile":{"enabled":true}}},"rawfile-localpv":{"crds":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8e58e4364ed4e2cc9b05cd3dfddf39fbaa581353cd12553af21a32cdc28adaca",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "46951c786324b59a7be901c88fd5c56b5e01f54aa558f6bda22914e1596cab9a",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"alloy":{"crds":{"create":null},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "95e7b2dfbd3c58a828b9bc07502aad357e2419ad99082b8804b55279cb4f41a1",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "957dec03d18c701d1b591a4e8dbade02ee1cb29d553694c906829aa67fd3f0fe",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"loki":{"enabled":true,"rollout_operator":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "ef0ae13604292e4980ef7e06044b9101210d8960682ce3b1fb12a4fb9a17fcac",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "2bc741c2252e860bcaa56454319d5c9eea7b180805294a4731efdd1a97430b35",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"loki":{"enabled":true,"rollout_operator":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2241a3e8f5513e9a8ce84322686d61f35af292f6456c2b65ee398859458fe473",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "0f3c2fbfb07677fe4400b45bce04db88e65fcddbfc6aba26b8508a908295f217",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"lvm":{"enabled":true}}},"lvm-localpv":{"crds":{"csi":{"volumeSnapshots":{"enabled":true}},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0f746c121bbd69f165241386f36401ba6440d9a49a5950865087bd0c016028f9",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b52d55f25d56121eaf4123f0a731945d1eb5380c93d993cfd0b008d9aef723d4",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"crds":{"csi":{"volumeSnapshots":{"annotations":{}}},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3539f0ec823d442394e82089c0964581f9fa7a3f48c45c70aca9e099b551c69f",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b52d55f25d56121eaf4123f0a731945d1eb5380c93d993cfd0b008d9aef723d4",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"zfs":{"enabled":true}}},"zfs-localpv":{"crds":{"csi":{"volumeSnapshots":{"enabled":true}},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6b47c8e126cfd2ddd12b14d30216bf76f5e35b1f85db1d156562b24ab88b7e1d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b52d55f25d56121eaf4123f0a731945d1eb5380c93d993cfd0b008d9aef723d4",
        )),
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "openldap-stack-ha",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"common":{"exampleValue":"common-chart"},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "28f90dd6b60110aed9681f7cc4932401c41569b1f6125ffd66ea49ef65868f29",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "e58455b88f9cc405e8a0914c696fc4af528cdfbc0d02e507641803354bc05012",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "openldap-stack-ha",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"phpldapadmin":{"affinity":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "7a997ee4a707ce8a61ef80da59c2c2fea6afb98ce5d317ee024f07fa1e2fd6ec",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "e58455b88f9cc405e8a0914c696fc4af528cdfbc0d02e507641803354bc05012",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "openldap-stack-ha",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ltb-passwd":{"affinity":{},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "cf8bf03632a37f8934d4307de0ce5f65a1e2434aedab039c2a9340a41e3928cd",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "e58455b88f9cc405e8a0914c696fc4af528cdfbc0d02e507641803354bc05012",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "redmine",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d548823ecf9ca074983563af3f6841877dc871f807081b3820eff903e83d639b",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "f89a8c2920c35b39008a22aadc860d9f410710bf9924ab0e519cc050c55bfc1d",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b06018cbe2bf4b98ff75169333267760ee74a72785990bdc30eed94ab5dc5b4c",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "a26b89523cf56ada91a7e970fe02019ff95aff64d8e96642c4d59d16e4a53215",
        )),
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"clickhouse":{"enabled":true,"zookeeper":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "de0efcd0304b20f832f5a24bc86479e36d62b9b0db139ff287aed6f74757ce5b",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "e73e6f0de3c21d7a2e5d01a2567dbe69ea2ab8abd006e1cdaab4ac4b2c70a70f",
        )),
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"clickhouse":{"enabled":true,"zookeeper":{"common":null,"enabled":true}},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3cd4228fc61f8a2edf03552fad70da953851beef6f4311a2c4a18b864665ada4",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "78227230b49d94309828ef76ced5fc846dce43d5296594dc2e21db6a83e83288",
        )),
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"common":null,"enabled":true}},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "7de309379647d1d8db79d56bc61ca8ae8409f4398a573998dc8f82801af44a21",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "ac1687a898a90f6f1aaff0b45a92109d3dac6673470d4dd12b60c0e0a6542967",
        )),
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b06018cbe2bf4b98ff75169333267760ee74a72785990bdc30eed94ab5dc5b4c",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "a26b89523cf56ada91a7e970fe02019ff95aff64d8e96642c4d59d16e4a53215",
        )),
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"enabled":true}},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8523b8c72de6453d390e0968609b6c196a4f514713a194cbbf20f1da4cdda7e4",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "2f50a513a9c9a18f4cd3ca7c36b26212ba549f3b6fc7dc3e1252aae60d39a76d",
        )),
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"enabled":true}},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "9a6b8b5b6f63dc3c392a26108b655d8563d0e40ac8992b9b1c5502cd28889b8e",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "e97366805db0b78ba093813ce6beee1806b961ebb1741f00536b1241a644fd89",
        )),
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "60f394b1c0817b632c2583236f1c3e4b2e0b1d60da4c35933aeb83152627cbb9",
            ),
            controls_sha256: Cow::Borrowed(
                "477b6dca4ebef61a4ebe0be5b58c320ac4e44f9a7bf62f69ac9696db7fad4df1",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "b66d53a4f311c8094a578645a17906d06264d20b4d02e8d4f8d6f0faa76f203d",
        )),
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d686305e72026de1bc0639255a9035f9cd273c4d5a6a8deed196127bc1f1d5cc",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "e97366805db0b78ba093813ce6beee1806b961ebb1741f00536b1241a644fd89",
        )),
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b06018cbe2bf4b98ff75169333267760ee74a72785990bdc30eed94ab5dc5b4c",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "a26b89523cf56ada91a7e970fe02019ff95aff64d8e96642c4d59d16e4a53215",
        )),
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "empty array",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "27be805660806df4a20a4c009f4878f9c60d50a10daeb7dd6da0f5645972e943",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: None,
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "empty object",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1befec73a78b649c1a8a376c781bfc00680984fc956e1412018fe1dcbcc46411",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: None,
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "empty object item",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "d3fd0d9e11720475caf3d3d20b62e319b65ba5f855ca626f76d37f96473433c3",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: None,
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "false",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "e37a3ff9f26a2a25b8dd95be8707f96b4347d467e29545f226f6e2ae448edf71",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: None,
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "integer",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "3a6f234c21ed7a5ec8dcb13d509acceca768b10e46ee0aee9f864cd3f1783f8d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: None,
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "number",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a27f99f927be505a1183af92d18aa0820b07a6e58ac2a93b1b26cbb9707400dd",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: None,
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "true",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a296cad7e5e9c5d2353c94df9329c2eb3534901515ba279761f1753fab36501c",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: None,
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "unknown object member",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "7216dc4cfaeafeeb3ee8ccc881ad9d40311ee7e53a0ab10f7b28a057870c1b4a",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: None,
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"common":{"exampleValue":"common-chart"},"enabled":true}},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "2c815f3622d2010530114cff6b20afa5c85c98c92338213e25256179e6ce3ea3",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "e97366805db0b78ba093813ce6beee1806b961ebb1741f00536b1241a644fd89",
        )),
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"architecture":"standalone","enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "bc2180a1961e58acbeddd0802481fb6ec0ba79f5dc3207df6c05d1cdd61a6eeb",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "e97366805db0b78ba093813ce6beee1806b961ebb1741f00536b1241a644fd89",
        )),
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"stacks-blockchain":{"bitcoin-core":{"enabled":null},"enabled":true},"tags":{"hirosystems-bitcoin-core":true,"hirosystems-stacks-blockchain":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "37f898a32d91ee83862d9a9ede889da0286e9b2d3e4262d8e1bdb6340e202177",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "2307bbc19e3a41ff524b0aaf56bafb574aae2a3341634dac7a76f3f8095ab4e9",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"stacks-blockchain":{"bitcoin-core":{"enabled":null},"enabled":true},"tags":{"hirosystems-bitcoin-core":false,"hirosystems-stacks-blockchain":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "00e1833320aeeca6404b128721f4c43ec483a4ae827d4a4bde6e3587c4271805",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "c0ba1c15844243c2b7fa033bf1cf77d370fa173b3356f88c20765fa88b99bcc6",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"stacks-blockchain":{"bitcoin-core":{"enabled":null},"enabled":true},"tags":{"hirosystems-bitcoin-core":true,"hirosystems-stacks-blockchain":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "37f898a32d91ee83862d9a9ede889da0286e9b2d3e4262d8e1bdb6340e202177",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "2307bbc19e3a41ff524b0aaf56bafb574aae2a3341634dac7a76f3f8095ab4e9",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"stacks-blockchain":{"bitcoin-core":{"enabled":null},"enabled":true},"tags":{"hirosystems-bitcoin-core":true,"hirosystems-stacks-blockchain":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "37f898a32d91ee83862d9a9ede889da0286e9b2d3e4262d8e1bdb6340e202177",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "2307bbc19e3a41ff524b0aaf56bafb574aae2a3341634dac7a76f3f8095ab4e9",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"architecture":"standalone","enabled":true},"tags":{"bitnami-postgresql":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0133eb500fb4311d56cd4022b23ac12cfdf5283d580af4df188fe3fe4012faba",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "5ef3085d2918a183837f2787cc31e442226a4cdcecb6286ba9b7ddf37df6bc62",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"common":{"exampleValue":"common-chart"},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "28f90dd6b60110aed9681f7cc4932401c41569b1f6125ffd66ea49ef65868f29",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "5ef3085d2918a183837f2787cc31e442226a4cdcecb6286ba9b7ddf37df6bc62",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"stacks-blockchain":{"bitcoin-core":{"affinity":{},"enabled":true},"enabled":true},"tags":{"hirosystems-bitcoin-core":true,"hirosystems-stacks-blockchain":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6cd5e5d601abeb7ffecb5730733e2635d3fed0e561e00dc78b97cb4c0975e0e7",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "5ef3085d2918a183837f2787cc31e442226a4cdcecb6286ba9b7ddf37df6bc62",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"stacks-blockchain":{"affinity":{},"enabled":true},"tags":{"hirosystems-stacks-blockchain":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "7f5394acb710f9ada7d91cba872291c7ce2ede59f4b96dc447f059dfe5820ae7",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "5ef3085d2918a183837f2787cc31e442226a4cdcecb6286ba9b7ddf37df6bc62",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true,"bitnami-postgresql":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "809ca614a21a1d3ff3b9ac3661bc288fa813af1046ae18510b1484c0d70a5e6f",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "5ef3085d2918a183837f2787cc31e442226a4cdcecb6286ba9b7ddf37df6bc62",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"stacks-blockchain":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true,"hirosystems-stacks-blockchain":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "896b6a3f5bcafefcb5c3d755c5fd16d7068e080b1040ff91fce2e71fae6d89c2",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "5ef3085d2918a183837f2787cc31e442226a4cdcecb6286ba9b7ddf37df6bc62",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"stacks-blockchain":{"bitcoin-core":{"common":{"exampleValue":"common-chart"},"enabled":true},"enabled":true},"tags":{"bitnami-common":true,"hirosystems-bitcoin-core":true,"hirosystems-stacks-blockchain":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a88e57ca6308e34f84de89d8c15f02063c4f9e14f03bdaed99a8d68f2e3d0477",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "5ef3085d2918a183837f2787cc31e442226a4cdcecb6286ba9b7ddf37df6bc62",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "synapse",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"architecture":"replication","enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "24daa834bae01310d0e6e8c9422db24fee9b208bef6d75d176bf859b7032fe53",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "009b1a66352e493e0664c09464280860e970bda40dc3e970925802aea4a80587",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "synapse",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"architecture":"standalone","enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "6acc2a6d0485a78705005ff0187ef06aa0e8e0e0c5a45ee6bed2b7948b4c716d",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "009b1a66352e493e0664c09464280860e970bda40dc3e970925802aea4a80587",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "synapse",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "a8f4d1f79eb404f0c2d0f39c5ace1f838be6a031a7710e3ec3c384a310c0fcfb",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "009b1a66352e493e0664c09464280860e970bda40dc3e970925802aea4a80587",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "synapse",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "b3ec24a982b9bd1b7d5ab09e923e7f29264bbcb13cd78f2334a6434aef9cdfed",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "009b1a66352e493e0664c09464280860e970bda40dc3e970925802aea4a80587",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "yourls",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"common":{"exampleValue":"common-chart"},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "28f90dd6b60110aed9681f7cc4932401c41569b1f6125ffd66ea49ef65868f29",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "473a3bea01918d63deb653d8870ee436c911eeec87cd9302ff678c035db1bce5",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "yourls",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"architecture":"standalone","enabled":true},"tags":{"yourls-database":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "43239b366ff9a275fa59175306e8bd1534781e16aa2b8f1ab220278d614c1547",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "473a3bea01918d63deb653d8870ee436c911eeec87cd9302ff678c035db1bce5",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
    KnownInheritedFailure {
        chart: "yourls",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"common":{"exampleValue":"common-chart"},"enabled":true},"tags":{"bitnami-common":true,"yourls-database":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "8043aa804fba70e50a05766fa9d45f5b20991ac7d69766ad216097303cbff931",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "473a3bea01918d63deb653d8870ee436c911eeec87cd9302ff678c035db1bce5",
        )),
        owner: "generator: quarantined defaults false rejection (chart_corpus QUARANTINED_FALSE_REJECTIONS)",
        observed_on: "lint10c/d3cf7a2f",
    },
];

//! Exact hard failures inherited by the battery, with no allowance for missing entries.

use std::borrow::Cow;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::helm::lint::{LintClassification, LintErrorKind, classify_lint};
use helm_schema_test_support::source_digest::sha256_hex;

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

pub(crate) use helm_schema_test_support::acceptance::{FailureInput, ObservedFailure};

pub(crate) struct KnownInheritedFailure {
    pub(crate) chart: &'static str,
    pub(crate) selector: FailureSelector,
    pub(crate) failure: FailureClass,
    pub(crate) input: FailureInput,
    pub(crate) lint_diagnostics_sha256: Option<Cow<'static, str>>,
    pub(crate) owner: &'static str,
    pub(crate) observed_on: &'static str,
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
            "9551df0719ec31e1c45daa869b2541574072019dc036a3856b1ace6ad73072d3",
        )),
        owner: "generator: lint-floor null deletion (A)",
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
            "a333e0af24736e12f375eba3ddfee4f78d0af781017b7d1354837b6a06c65642",
        )),
        owner: "generator: lint-floor null deletion (A)",
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
            "c89ec153f6390557822711a016e013dbabf44410bdddd9f500e1aba4f17abdc9",
        )),
        owner: "generator: lint-floor null deletion (A)",
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
            "3675dc73ae53d363181ca221778189a6f935cf32a210f1a98392bf4bcc5edd08",
        )),
        owner: "generator: lint-floor null deletion (A)",
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
            "9551df0719ec31e1c45daa869b2541574072019dc036a3856b1ace6ad73072d3",
        )),
        owner: "generator: lint-floor null deletion (A)",
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
            "a333e0af24736e12f375eba3ddfee4f78d0af781017b7d1354837b6a06c65642",
        )),
        owner: "generator: lint-floor null deletion (A)",
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
            "c89ec153f6390557822711a016e013dbabf44410bdddd9f500e1aba4f17abdc9",
        )),
        owner: "generator: lint-floor null deletion (A)",
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
            "3675dc73ae53d363181ca221778189a6f935cf32a210f1a98392bf4bcc5edd08",
        )),
        owner: "generator: lint-floor null deletion (A)",
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
            "9551df0719ec31e1c45daa869b2541574072019dc036a3856b1ace6ad73072d3",
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
            "a333e0af24736e12f375eba3ddfee4f78d0af781017b7d1354837b6a06c65642",
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
            "c89ec153f6390557822711a016e013dbabf44410bdddd9f500e1aba4f17abdc9",
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
            "3675dc73ae53d363181ca221778189a6f935cf32a210f1a98392bf4bcc5edd08",
        )),
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    // The W4 regexp compile blocker masked this inherited raw lint-floor failure.
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
            "31e3712c208286f5370653d9489332ff4ca2166aa6582d7c4f04bd40c08dfe9e",
        )),
        owner: "generator: raw lint-floor control masked by W4 regexp compile blocker",
        observed_on: "/Volumes/T7/dev/round8-regexu/target/evidence/helm-schema/adjudication-51716-98e5dc0a86fad45a-23/probe-6E03av/LintRaw-FBKCCX",
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

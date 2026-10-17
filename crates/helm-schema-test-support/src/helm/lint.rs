//! Conservative, rule-qualified classification of pinned Helm lint diagnostics.

/// The Helm lint rule that emitted an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LintRule {
    /// The values rule, before dependency processing.
    Values,
    /// The template rule, after dependency processing and coalescing.
    Templates,
}

/// The meaning supported by one rule-qualified diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LintErrorKind {
    /// A values-rule rejection of the raw lint document.
    RawValuesSchema,
    /// A template-rule schema rejection of the twice-coalesced document.
    TemplatesSchema,
    /// A template parse, execution or manifest error.
    Semantic,
}

/// One complete `[ERROR]` line, retaining the rule and original diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintError {
    /// The rule Helm reports.
    pub rule: LintRule,
    /// The diagnostic's supported interpretation.
    pub kind: LintErrorKind,
    /// The full error line.
    pub diagnostic: String,
}

/// Why a lint run cannot serve as acceptance evidence.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LintUnresolved {
    /// Helm did not return its normal success or failure status.
    #[error("exit-{0}")]
    Exit(i32),
    /// Kubernetes version argument could not be parsed.
    #[error("invalid-kube-version")]
    InvalidKubeVersion,
    /// The supplied values file could not be read or parsed.
    #[error("values-file")]
    ValuesFile,
    /// The command arguments were rejected.
    #[error("usage")]
    Usage,
    /// The chart has no readable Chart.yaml because it is missing.
    #[error("loader")]
    Loader,
    /// A required dependency is absent.
    #[error("missing-dependency")]
    MissingDependency,
    /// A successful exit lacked the expected one-chart summary.
    #[error("no-lint-summary")]
    NoLintSummary,
    /// Failure output did not identify only supported lint rules.
    #[error("unknown-lint-failure")]
    UnknownLintFailure,
}

/// A classified lint run; unresolved runs never establish a verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LintClassification {
    /// Helm reported a successful one-chart lint summary.
    Pass,
    /// Every reported error belongs to a recognized lint rule.
    Reject {
        /// All rule-qualified errors in output order.
        errors: Vec<LintError>,
    },
    /// Evidence is insufficient or the invocation failed operationally.
    Unresolved(LintUnresolved),
}

impl LintClassification {
    /// The existing sweep classifier's compatibility label.
    #[must_use]
    pub fn class(&self) -> String {
        match self {
            Self::Pass => "pass".to_string(),
            Self::Reject { .. } => "reject".to_string(),
            Self::Unresolved(reason) => format!("unresolved:{reason}"),
        }
    }
}

/// Classifies a combined stdout/stderr lint log using the sweep's diagnostic rules.
/// Unicode line boundaries match Python `splitlines`; summary and operational
/// checks use only universal newline conversion, as the sweep does. A values
/// error is conservatively retained as schema rejection without parsing its payload.
#[must_use]
pub fn classify_lint(exit_code: i32, log: &[u8]) -> LintClassification {
    let unresolved = LintClassification::Unresolved;
    if !matches!(exit_code, 0 | 1) {
        return LintClassification::Unresolved(LintUnresolved::Exit(exit_code));
    }
    let text = String::from_utf8_lossy(log)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if text.contains("invalid kube version") {
        return unresolved(LintUnresolved::InvalidKubeVersion);
    }
    for line in text.split('\n') {
        if line.starts_with("Error: open ") || line.starts_with("Error: failed to parse ") {
            return unresolved(LintUnresolved::ValuesFile);
        }
        if [
            "Error: unknown flag",
            "Error: unknown command",
            "Error: unknown shorthand",
            "Error: accepts ",
            "Error: requires ",
        ]
        .iter()
        .any(|prefix| line.starts_with(prefix))
        {
            return unresolved(LintUnresolved::Usage);
        }
    }
    let lines: Vec<_> = text
        .split([
            '\n', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .collect();
    let has_errors = lines.iter().any(|line| line.starts_with("[ERROR]"));
    if !has_errors {
        for line in &lines {
            if line
                .strip_prefix("Error unable to check Chart.yaml file in chart: stat ")
                .is_some_and(|path| {
                    path.ends_with("Chart.yaml: no such file or directory")
                        || path.ends_with("Chart.yaml: The system cannot find the file specified.")
                })
            {
                return unresolved(LintUnresolved::Loader);
            }
        }
        for line in &lines {
            if line.starts_with("[WARNING] ")
                && line.contains(": chart directory is missing these dependencies: ")
            {
                return unresolved(LintUnresolved::MissingDependency);
            }
        }
    }
    if exit_code == 0 {
        return if text
            .split('\n')
            .any(|line| line == "1 chart(s) linted, 0 chart(s) failed")
        {
            LintClassification::Pass
        } else {
            unresolved(LintUnresolved::NoLintSummary)
        };
    }
    let mut errors = Vec::new();
    for line in lines {
        if !line.starts_with("[ERROR]") {
            continue;
        }
        let (rule, kind) = if line.starts_with("[ERROR] values.yaml: ") {
            (LintRule::Values, LintErrorKind::RawValuesSchema)
        } else if line.starts_with("[ERROR] templates/") {
            let kind = if line.contains("values don't meet the specifications of the schema") {
                LintErrorKind::TemplatesSchema
            } else {
                LintErrorKind::Semantic
            };
            (LintRule::Templates, kind)
        } else {
            return unresolved(LintUnresolved::UnknownLintFailure);
        };
        errors.push(LintError {
            rule,
            kind,
            diagnostic: line.to_string(),
        });
    }
    if !errors.is_empty()
        && text.contains("==> Linting ")
        && text.contains("Error: 1 chart(s) linted, 1 chart(s) failed")
    {
        LintClassification::Reject { errors }
    } else {
        unresolved(LintUnresolved::UnknownLintFailure)
    }
}

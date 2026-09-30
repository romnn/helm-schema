//! The frozen-witness gate: every family witness against the adopted fixture.
//!
//! The round-74 battery adjudicates only verdicts that change between a
//! baseline and a candidate, so a false rejection or acceptance present in
//! both is never seen. This gate is absolute instead: it evaluates every
//! frozen witness of `common/family_witnesses.rs` against the adopted
//! `testdata/chart-corpus-schemas/{chart}.schema.json` alone, composing the
//! witness values over the chart defaults the way Helm coalesces them. It
//! runs no Helm; the Helm verdicts are the frozen oracle each row pins.
//!
//! A known-open row passes only while its defect persists: reaching the
//! verdict its oracle calls for fails it as an unexpected fix, so the row
//! must be promoted to `Fixed`, and a `Fixed` row fails on regression.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema::output::{HELM_MAX_CHART_FILE_BYTES, shorten_definition_names};
use helm_schema_test_support::helm::kubernetes_version;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use test_util::prelude::sim_assert_eq;

#[path = "common/family_witnesses.rs"]
mod family_witnesses;

#[path = "common/known_false_acceptances.rs"]
mod known_false_acceptances;

use family_witnesses::{
    CAMPAIGN_FAMILIES, ExpectedViolation, FAMILY_WITNESSES, FamilyWitnesses, KubernetesExpectation,
    ObligationExpectation, OracleExpectation, Overrides, PolicyOption, SchemaExpectation,
    SchemaVerdict, SetPair, SetValue, SizeObligation, Witness,
};
use known_false_acceptances::{Family, KNOWN_FALSE_ACCEPTANCES, ROSTER_BASELINE};

/// Schema errors printed beside a failing row.
const CONTEXT_ERRORS: usize = 5;
const CONTEXT_ERROR_CHARS: usize = 240;

/// The generated policy annotation key that records the authoring settings.
const POLICY_ANNOTATION_KEY: &str = "x-helm-schema-policy";

/// Where the gate finds charts, fixtures and witness values files.
struct Layout {
    charts: PathBuf,
    schemas: PathBuf,
    /// Fixtures generated with one caller authoring option, for the rows
    /// whose default rejection is a decided policy exception.
    policy_schemas: PathBuf,
    values_files: PathBuf,
}

impl Layout {
    fn corpus() -> Self {
        let testdata = test_util::workspace_testdata();
        Self {
            charts: testdata.join("charts"),
            schemas: testdata.join("chart-corpus-schemas"),
            policy_schemas: testdata.join("chart-corpus-policy-schemas"),
            values_files: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/family_witnesses"),
        }
    }

    fn chart_dir(&self, chart: &str) -> PathBuf {
        self.charts.join(chart)
    }

    fn fixture(&self, chart: &str) -> PathBuf {
        self.schemas.join(format!("{chart}.schema.json"))
    }

    fn policy_fixture(&self, chart: &str, option: PolicyOption) -> PathBuf {
        self.policy_schemas
            .join(format!("{chart}.{}.schema.json", option.fixture_name()))
    }
}

/// A row as the gate reports it. The gate compares the observed outcome with
/// the one the catalog records, so a failing row's diff is the catalog edit.
#[derive(Debug, PartialEq)]
struct RowOutcome {
    family: Family,
    id: &'static str,
    chart: &'static str,
    /// The digest of the witness inputs; `None` for an obligation.
    adjudicated: Option<String>,
    state: RowState,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RowState {
    Schema(SchemaExpectation),
    /// The verdicts of a policy-exception row: under the default fixture and
    /// under the fixture generated with its option. `None` is a fixture Helm
    /// refuses. The catalog requires `Rejects` and then `Accepts`.
    PolicyException {
        option: PolicyOption,
        default: Option<SchemaVerdict>,
        with_option: Option<SchemaVerdict>,
    },
    FileSize(ObligationExpectation),
    /// Helm refuses the chart's schema itself, so no values document on the
    /// chart can be judged. Never a recorded state: every verdict row on the
    /// chart fails until the fixture is usable.
    FixtureUnusable,
}

struct RowResult {
    have: RowOutcome,
    want: RowOutcome,
    /// Evidence printed beside a failing row's diff.
    context: Vec<String>,
}

/// Witnesses of one chart that compose to the same values document while
/// their oracles call for opposite verdicts. No schema can satisfy both, so
/// the families involved cannot close until a policy decides between them.
#[derive(Debug, PartialEq)]
struct Conflict {
    chart: &'static str,
    accepts: Vec<&'static str>,
    rejects: Vec<&'static str>,
    families: BTreeSet<Family>,
}

/// The witnesses of one chart that compose to one document, by the verdict
/// their oracles call for.
#[derive(Default)]
struct SameDocument {
    accepts: Vec<(Family, &'static str)>,
    rejects: Vec<(Family, &'static str)>,
}

struct GateRun {
    rows: Vec<RowResult>,
    conflicts: Vec<Conflict>,
}

/// An adopted fixture, compiled once per chart.
enum Fixture {
    Usable(jsonschema::Validator),
    /// Helm refuses the schema itself, so it rejects every values document.
    Unusable(String),
}

#[test]
fn fixture_verdicts() -> eyre::Result<()> {
    let layout = Layout::corpus();
    let run = run_gate(FAMILY_WITNESSES, &layout)?;
    println!("{}", family_report(FAMILY_WITNESSES, &run)?);
    let failures = report_failures(&run);
    eyre::ensure!(
        failures == 0,
        "{failures} frozen witness rows failed; each diff above shows the observed outcome \
         (have) against the catalog row (want)"
    );
    Ok(())
}

/// Prints one diff per failing row and returns how many failed.
fn report_failures(run: &GateRun) -> usize {
    let mut failures = 0;
    for row in &run.rows {
        let diff = std::panic::catch_unwind(|| {
            sim_assert_eq!(have: &row.have, want: &row.want);
        });
        if diff.is_err() {
            failures += 1;
            eprintln!("{}: {}", row.want.id, failure_hint(&row.have, &row.want));
            for line in &row.context {
                eprintln!("    {line}");
            }
        }
    }
    failures
}

fn failure_hint(have: &RowOutcome, want: &RowOutcome) -> String {
    let mut hints = Vec::new();
    if let Some(digest) = &have.adjudicated
        && have.adjudicated != want.adjudicated
    {
        hints.push(format!(
            "the witness inputs changed since adjudication: re-run Helm on the row, then \
             record `adjudicated: \"{digest}\"`"
        ));
    }
    if have.state != want.state {
        let hint = match (have.state, want.state) {
            (RowState::FixtureUnusable, _) => {
                "Helm refuses the chart's fixture itself, so no row on the chart can be judged"
                    .to_string()
            }
            (_, RowState::PolicyException { .. }) => {
                "a policy exception must reject under the default fixture and accept under the \
                 fixture generated with its option"
                    .to_string()
            }
            (_, RowState::Schema(SchemaExpectation::PolicyUnresolved { .. })) => {
                "the verdict moved on a row whose policy is unresolved: decide the policy, \
                 then record the row's state"
                    .to_string()
            }
            (
                _,
                RowState::Schema(
                    SchemaExpectation::KnownFalseRejection
                    | SchemaExpectation::KnownFalseAcceptance,
                )
                | RowState::FileSize(ObligationExpectation::KnownUnmet),
            ) => format!("unexpected fix: promote the row to {:?}", have.state),
            _ => "regression".to_string(),
        };
        hints.push(hint);
    }
    hints.join("; ")
}

/// The rows of `chart` that call for both verdicts of one composed document
/// judged under one policy.
fn same_document_conflicts(
    chart: &'static str,
    documents: impl Iterator<Item = SameDocument>,
) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    for same in documents {
        if same.accepts.is_empty() || same.rejects.is_empty() {
            continue;
        }
        conflicts.push(Conflict {
            chart,
            accepts: same.accepts.iter().map(|(_, id)| *id).collect(),
            rejects: same.rejects.iter().map(|(_, id)| *id).collect(),
            families: same
                .accepts
                .iter()
                .chain(&same.rejects)
                .map(|(family, _)| *family)
                .collect(),
        });
    }
    conflicts
}

fn run_gate(catalog: &[FamilyWitnesses], layout: &Layout) -> eyre::Result<GateRun> {
    let problems = registration_problems(catalog, layout)?;
    eyre::ensure!(
        problems.is_empty(),
        "the witness catalog is malformed:\n{}",
        problems.join("\n")
    );

    let mut by_chart: BTreeMap<&'static str, Vec<(Family, &Witness)>> = BTreeMap::new();
    for entry in catalog {
        for witness in entry.witnesses {
            by_chart
                .entry(witness.chart)
                .or_default()
                .push((entry.family, witness));
        }
    }

    let mut rows = Vec::new();
    let mut conflicts = Vec::new();
    for (chart, witnesses) in by_chart {
        let fixture = load_fixture(&layout.fixture(chart))?;
        let mut option_fixtures: BTreeMap<PolicyOption, Fixture> = BTreeMap::new();
        let chart_tree = chart_tree_digest(&layout.chart_dir(chart))?;
        // Documents by the policy they are judged under: `None` is the
        // default policy.
        let mut documents: BTreeMap<(Option<PolicyOption>, String), SameDocument> = BTreeMap::new();
        for (family, witness) in witnesses {
            let overlay = witness.overrides.overlay(&layout.values_files)?;
            let document =
                test_util::helm_values::coalesce_chart_values(&layout.chart_dir(chart), overlay)
                    .wrap_err_with(|| format!("compose witness {}", witness.id))?;
            let (verdict, mut context) = judge(&fixture, &document);
            let state = match witness.schema {
                SchemaExpectation::PolicyException { option } => {
                    let option_fixture = match option_fixtures.entry(option) {
                        Entry::Occupied(entry) => entry.into_mut(),
                        Entry::Vacant(entry) => {
                            entry.insert(load_fixture(&layout.policy_fixture(chart, option))?)
                        }
                    };
                    let (with_option, option_context) = judge(option_fixture, &document);
                    context.push(format!("with {}:", option.fixture_name()));
                    context.extend(option_context);
                    let key = document.to_string();
                    documents
                        .entry((None, key.clone()))
                        .or_default()
                        .rejects
                        .push((family, witness.id));
                    documents
                        .entry((Some(option), key))
                        .or_default()
                        .accepts
                        .push((family, witness.id));
                    RowState::PolicyException {
                        option,
                        default: verdict,
                        with_option,
                    }
                }
                SchemaExpectation::PolicyUnresolved { .. } => observed_state(witness, verdict),
                _ => {
                    let same = documents.entry((None, document.to_string())).or_default();
                    match witness.oracle.desired() {
                        SchemaVerdict::Accepts => same.accepts.push((family, witness.id)),
                        SchemaVerdict::Rejects => same.rejects.push((family, witness.id)),
                    }
                    observed_state(witness, verdict)
                }
            };
            rows.push(RowResult {
                have: RowOutcome {
                    family,
                    id: witness.id,
                    chart,
                    adjudicated: Some(adjudicated_digest(witness, &chart_tree, layout)?),
                    state,
                },
                want: RowOutcome {
                    family,
                    id: witness.id,
                    chart,
                    adjudicated: Some(witness.adjudicated.to_string()),
                    state: expected_state(witness),
                },
                context,
            });
        }
        conflicts.extend(same_document_conflicts(chart, documents.into_values()));
    }

    for entry in catalog {
        for obligation in entry.size_obligations {
            rows.push(evaluate_size(entry.family, obligation, layout)?);
        }
    }
    Ok(GateRun { rows, conflicts })
}

/// The state the catalog records for `witness`.
fn expected_state(witness: &Witness) -> RowState {
    match witness.schema {
        SchemaExpectation::PolicyException { option } => RowState::PolicyException {
            option,
            default: Some(SchemaVerdict::Rejects),
            with_option: Some(SchemaVerdict::Accepts),
        },
        schema => RowState::Schema(schema),
    }
}

/// The state the fixture's `verdict` puts `witness` in; no verdict means
/// the fixture is unusable.
fn observed_state(witness: &Witness, verdict: Option<SchemaVerdict>) -> RowState {
    let Some(verdict) = verdict else {
        return RowState::FixtureUnusable;
    };
    if let SchemaExpectation::PolicyUnresolved { question, .. } = witness.schema {
        return RowState::Schema(SchemaExpectation::PolicyUnresolved {
            current: verdict,
            question,
        });
    }
    let desired = witness.oracle.desired();
    let state = if verdict == desired {
        SchemaExpectation::Fixed(verdict)
    } else {
        match desired {
            SchemaVerdict::Accepts => SchemaExpectation::KnownFalseRejection,
            SchemaVerdict::Rejects => SchemaExpectation::KnownFalseAcceptance,
        }
    };
    RowState::Schema(state)
}

fn read_fixture(path: &Path) -> eyre::Result<Value> {
    let source =
        std::fs::read_to_string(path).wrap_err_with(|| format!("read {}", path.display()))?;
    serde_json::from_str(&source).wrap_err_with(|| format!("parse {}", path.display()))
}

/// The bytes Helm reads for `schema`: the compact document with short
/// `$defs` keys that `helm-schema lint` / `template` write into the chart
/// copy they hand to Helm (`helm_schema::helm`). The fixture's own bytes on
/// disk are the readable corpus serialization, not what ships.
fn shipped_bytes(schema: &Value) -> eyre::Result<usize> {
    Ok(serde_json::to_vec(&shorten_definition_names(schema).schema)?.len())
}

fn load_fixture(path: &Path) -> eyre::Result<Fixture> {
    let schema = read_fixture(path)?;
    let size = shipped_bytes(&schema)?;
    if size > HELM_MAX_CHART_FILE_BYTES {
        return Ok(Fixture::Unusable(format!(
            "the shipped schema has {size} bytes, over Helm's {HELM_MAX_CHART_FILE_BYTES}-byte \
             file limit"
        )));
    }
    match jsonschema::validator_for(&schema) {
        Ok(validator) => Ok(Fixture::Usable(validator)),
        Err(error) => Ok(Fixture::Unusable(format!(
            "the schema does not compile: {error}"
        ))),
    }
}

/// The fixture's verdict on `document`, with its sorted errors as context;
/// no verdict when Helm would refuse the fixture itself.
fn judge(fixture: &Fixture, document: &Value) -> (Option<SchemaVerdict>, Vec<String>) {
    match fixture {
        Fixture::Unusable(reason) => (None, vec![format!("fixture unusable: {reason}")]),
        Fixture::Usable(validator) => {
            let mut errors: Vec<String> = validator
                .iter_errors(document)
                .map(|error| {
                    let line = format!(
                        "at '{}' (schema '{}'): {error}",
                        error.instance_path(),
                        error.schema_path()
                    );
                    line.chars().take(CONTEXT_ERROR_CHARS).collect()
                })
                .collect();
            if errors.is_empty() {
                return (
                    Some(SchemaVerdict::Accepts),
                    vec!["the fixture accepts".to_string()],
                );
            }
            errors.sort();
            let total = errors.len();
            errors.truncate(CONTEXT_ERRORS);
            errors.insert(0, format!("the fixture rejects with {total} errors:"));
            (Some(SchemaVerdict::Rejects), errors)
        }
    }
}

fn evaluate_size(
    family: Family,
    obligation: &SizeObligation,
    layout: &Layout,
) -> eyre::Result<RowResult> {
    let size = shipped_bytes(&read_fixture(&layout.fixture(obligation.chart))?)?;
    let observed = if size <= HELM_MAX_CHART_FILE_BYTES {
        ObligationExpectation::Met
    } else {
        ObligationExpectation::KnownUnmet
    };
    let outcome = |state| RowOutcome {
        family,
        id: obligation.id,
        chart: obligation.chart,
        adjudicated: None,
        state: RowState::FileSize(state),
    };
    Ok(RowResult {
        have: outcome(observed),
        want: outcome(obligation.expectation),
        context: vec![format!(
            "Helm reads {size} shipped bytes; its limit is {HELM_MAX_CHART_FILE_BYTES}"
        )],
    })
}

/// Everything wrong with the catalog's shape, before any row is evaluated.
fn registration_problems(
    catalog: &[FamilyWitnesses],
    layout: &Layout,
) -> eyre::Result<Vec<String>> {
    let mut problems = Vec::new();
    let mut ids = BTreeSet::new();
    let mut inputs: BTreeMap<String, &str> = BTreeMap::new();
    let mut charts_checked = BTreeSet::new();
    let mut option_fixtures_checked = BTreeSet::new();
    for pair in catalog.windows(2) {
        if let [first, second] = pair
            && first.family >= second.family
        {
            problems.push(format!(
                "families must be listed once, in order: {:?} before {:?}",
                first.family, second.family
            ));
        }
    }
    for entry in catalog {
        let family = entry.family;
        if entry.witnesses.is_empty()
            && entry.size_obligations.is_empty()
            && entry.unfrozen.is_empty()
        {
            problems.push(format!("{family:?} lists nothing"));
        }
        for note in entry.unfrozen {
            if note.trim().is_empty() {
                problems.push(format!("{family:?} has an empty unfrozen note"));
            }
        }
        for witness in entry.witnesses {
            let id = witness.id;
            if !ids.insert(id) {
                problems.push(format!("{id}: duplicate id"));
            }
            if !id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                problems.push(format!("{id}: an id is lowercase ASCII, digits and '-'"));
            }
            if charts_checked.insert(witness.chart) {
                problems.extend(chart_problems(witness.chart, layout)?);
            }
            if let SchemaExpectation::PolicyException { option } = witness.schema
                && option_fixtures_checked.insert((witness.chart, option))
            {
                problems.extend(option_fixture_problems(witness.chart, option, layout)?);
            }
            // The row's version is frozen evidence; the corpus policy owns the
            // version the chart renders under.
            match kubernetes_version::chart_kubernetes_version(&layout.chart_dir(witness.chart)) {
                Ok(selected) if selected == witness.kubernetes_version => {}
                Ok(selected) => problems.push(format!(
                    "{id}: adjudicated under Kubernetes {}, but the chart renders under \
                     {selected}",
                    witness.kubernetes_version
                )),
                Err(error) => problems.push(format!("{id}: {error:#}")),
            }
            problems.extend(override_problems(family, witness, layout));
            problems.extend(expectation_problems(witness));
            if witness.adjudicated.is_empty() {
                problems.push(format!("{id}: never adjudicated"));
            }
            if let Ok(text) = overrides_text(&witness.overrides, layout) {
                let key = format!("{} {}\n{text}", witness.chart, witness.kubernetes_version);
                if let Some(first) = inputs.insert(key, id) {
                    problems.push(format!("{id}: the same inputs as {first}"));
                }
            }
        }
        for obligation in entry.size_obligations {
            let id = obligation.id;
            if !ids.insert(id) {
                problems.push(format!("{id}: duplicate id"));
            }
            if !layout.fixture(obligation.chart).is_file() {
                problems.push(format!("{id}: no fixture for chart {}", obligation.chart));
            }
        }
    }
    Ok(problems)
}

/// An option fixture must exist and record, in its generated policy
/// annotation, that it was generated with exactly that caller option.
fn option_fixture_problems(
    chart: &str,
    option: PolicyOption,
    layout: &Layout,
) -> eyre::Result<Vec<String>> {
    let name = option.fixture_name();
    let path = layout.policy_fixture(chart, option);
    if !path.is_file() {
        return Ok(vec![format!("chart {chart}: no {name} option fixture")]);
    }
    let authoring = read_fixture(&path)?
        .get(POLICY_ANNOTATION_KEY)
        .and_then(|annotation| annotation.get("authoring"))
        .cloned();
    if authoring.as_ref() == Some(&option.authoring_annotation()) {
        return Ok(Vec::new());
    }
    Ok(vec![format!(
        "chart {chart}: the {name} option fixture records authoring {} instead",
        authoring.unwrap_or(Value::Null)
    )])
}

/// A chart the Rust coalescer cannot compose exactly is not enrolled:
/// packaged dependencies and `import-values` are unmodelled.
fn chart_problems(chart: &str, layout: &Layout) -> eyre::Result<Vec<String>> {
    let dir = layout.chart_dir(chart);
    let mut problems = Vec::new();
    if !dir.is_dir() {
        problems.push(format!("chart {chart}: no directory {}", dir.display()));
        return Ok(problems);
    }
    if !layout.fixture(chart).is_file() {
        problems.push(format!("chart {chart}: no fixture"));
    }
    for relative in chart_files(&dir)? {
        let in_charts = relative
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|parent| parent == "charts");
        if in_charts && relative.extension().is_some_and(|ext| ext == "tgz") {
            problems.push(format!(
                "chart {chart}: packaged dependency {} is not modelled",
                relative.display()
            ));
        }
        let manifest = relative
            .file_name()
            .is_some_and(|name| name == "Chart.yaml" || name == "requirements.yaml");
        if manifest {
            let path = dir.join(&relative);
            let source = std::fs::read_to_string(&path)
                .wrap_err_with(|| format!("read {}", path.display()))?;
            let parsed: Value = serde_yaml::from_str(&source)
                .wrap_err_with(|| format!("parse {}", path.display()))?;
            let imports = parsed
                .get("dependencies")
                .and_then(Value::as_array)
                .is_some_and(|dependencies| {
                    dependencies
                        .iter()
                        .any(|dependency| dependency.get("import-values").is_some())
                });
            if imports {
                problems.push(format!(
                    "chart {chart}: import-values in {} is not modelled",
                    relative.display()
                ));
            }
        }
    }
    Ok(problems)
}

fn override_problems(family: Family, witness: &Witness, layout: &Layout) -> Vec<String> {
    let id = witness.id;
    let mut problems = Vec::new();
    match witness.overrides {
        Overrides::Set(pairs) => {
            for pair in pairs {
                let key_is_plain = pair.key.split('.').all(|segment| {
                    !segment.is_empty()
                        && segment
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                });
                if !key_is_plain {
                    problems.push(format!(
                        "{id}: --set key {:?} is not a plain path",
                        pair.key
                    ));
                }
                if let SetValue::Str(text) = pair.value
                    && !set_string_is_literal(text)
                {
                    problems.push(format!(
                        "{id}: --set {}={text} would not reach Helm as that string",
                        pair.key
                    ));
                }
            }
            if let Err(error) = witness.overrides.overlay(&layout.values_files) {
                problems.push(format!("{id}: {error}"));
            }
        }
        Overrides::ValuesFile(relative) => {
            let relative = Path::new(relative);
            let directory = relative
                .parent()
                .and_then(Path::to_str)
                .map(str::parse::<Family>);
            let named = directory == Some(Ok(family))
                && relative.file_stem().is_some_and(|stem| stem == id)
                && relative.extension().is_some_and(|ext| ext == "yaml");
            if !named {
                problems.push(format!(
                    "{id}: a values file is {family:?}/{id}.yaml, not {}",
                    relative.display()
                ));
            }
            if let Err(error) = witness.overrides.overlay(&layout.values_files) {
                problems.push(format!("{id}: {error:#}"));
            }
        }
    }
    problems
}

/// Whether Helm's `--set` parser keeps `text` as exactly this string: it
/// re-types booleans, `null` and integers, and splits on its own syntax.
fn set_string_is_literal(text: &str) -> bool {
    let retyped = ["true", "false", "null"]
        .iter()
        .any(|word| text.eq_ignore_ascii_case(word))
        || text.parse::<i64>().is_ok();
    let syntax = text.contains([',', '=', '[', ']', '{', '}', '\\']);
    !retyped && !syntax
}

fn expectation_problems(witness: &Witness) -> Vec<String> {
    let id = witness.id;
    let mut problems = Vec::new();
    match witness.oracle {
        OracleExpectation::Aborts { diagnostic } if diagnostic.trim().is_empty() => {
            problems.push(format!("{id}: an abort needs its Helm diagnostic"));
        }
        OracleExpectation::Renders {
            kubernetes: KubernetesExpectation::Invalid { violations: [] },
        } => {
            problems.push(format!("{id}: an invalid render needs its violations"));
        }
        _ => {}
    }
    let desired = witness.oracle.desired();
    let consistent = match witness.schema {
        SchemaExpectation::Fixed(verdict) => verdict == desired,
        // A policy exception's option restores the oracle's acceptance the
        // default policy deliberately withholds.
        SchemaExpectation::KnownFalseRejection | SchemaExpectation::PolicyException { .. } => {
            desired == SchemaVerdict::Accepts
        }
        SchemaExpectation::KnownFalseAcceptance => desired == SchemaVerdict::Rejects,
        SchemaExpectation::PolicyUnresolved { question, .. } => {
            if question.trim().is_empty() {
                problems.push(format!("{id}: an unresolved policy needs its question"));
            }
            true
        }
    };
    if !consistent {
        problems.push(format!(
            "{id}: {:?} contradicts an oracle that calls for {desired:?}",
            witness.schema
        ));
    }
    problems
}

/// The adjudicated inputs of a row, spelled canonically: chart tree,
/// Kubernetes version, overrides with their transport, and the oracle.
fn adjudicated_digest(
    witness: &Witness,
    chart_tree: &str,
    layout: &Layout,
) -> eyre::Result<String> {
    let mut text = String::new();
    writeln!(text, "chart {} {chart_tree}", witness.chart)?;
    writeln!(text, "kubernetes {}", witness.kubernetes_version)?;
    text.push_str(&overrides_text(&witness.overrides, layout)?);
    match witness.oracle {
        OracleExpectation::Aborts { diagnostic } => writeln!(text, "aborts {diagnostic}")?,
        OracleExpectation::Renders { kubernetes } => match kubernetes {
            KubernetesExpectation::NotRelevant => writeln!(text, "renders")?,
            KubernetesExpectation::Valid => writeln!(text, "renders valid")?,
            KubernetesExpectation::Invalid { violations } => {
                writeln!(text, "renders invalid")?;
                for ExpectedViolation {
                    kind,
                    instance_path,
                    message,
                } in violations
                {
                    writeln!(text, "violation {kind} {instance_path} {message}")?;
                }
            }
        },
    }
    let mut digest = sha256_hex(text.as_bytes());
    digest.truncate(16);
    Ok(digest)
}

/// The overrides with their transport: `--set` arguments as Helm receives
/// them, or the digest of the values file's bytes.
fn overrides_text(overrides: &Overrides, layout: &Layout) -> eyre::Result<String> {
    let mut text = String::new();
    match overrides {
        Overrides::Set(pairs) => {
            for pair in *pairs {
                writeln!(text, "--set {}", pair.argument())?;
            }
        }
        Overrides::ValuesFile(relative) => {
            let path = layout.values_files.join(relative);
            let bytes =
                std::fs::read(&path).wrap_err_with(|| format!("read {}", path.display()))?;
            writeln!(text, "-f {}", sha256_hex(&bytes))?;
        }
    }
    Ok(text)
}

/// Digest of every file under a chart directory, by relative path.
fn chart_tree_digest(dir: &Path) -> eyre::Result<String> {
    let mut hasher = Sha256::new();
    for relative in chart_files(dir)? {
        let path = dir.join(&relative);
        let bytes = std::fs::read(&path).wrap_err_with(|| format!("read {}", path.display()))?;
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(u64::try_from(bytes.len())?.to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hex(&hasher.finalize()))
}

/// Every file under `dir`, relative to it, sorted.
fn chart_files(dir: &Path) -> eyre::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let absolute = dir.join(&relative);
        for entry in
            std::fs::read_dir(&absolute).wrap_err_with(|| format!("list {}", absolute.display()))?
        {
            let entry = entry?;
            let child = relative.join(entry.file_name());
            if entry.path().is_dir() {
                pending.push(child);
            } else {
                files.push(child);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// Per-family counts and the campaign metric.
#[derive(Debug, Default, PartialEq)]
struct FamilyTally {
    rows: usize,
    fixed: usize,
    /// Rows whose default rejection is a decided policy exception.
    exceptions: usize,
    known_open: usize,
    failing: usize,
    unfrozen: usize,
    roster: usize,
    conflicts: usize,
}

impl FamilyTally {
    /// Closed: frozen coverage exists, every row is fixed and passing, and
    /// nothing about the family is still unfrozen or contradictory.
    fn closed(&self) -> bool {
        self.rows > 0
            && self.fixed == self.rows
            && self.failing == 0
            && self.unfrozen == 0
            && self.roster == 0
            && self.conflicts == 0
    }

    /// Closed by policy: at least one row is a decided policy exception,
    /// every other row is fixed, every row passes under both of its
    /// evaluations, and nothing is unfrozen or contradictory. Never counted
    /// as `closed`.
    fn closed_by_policy(&self) -> bool {
        self.exceptions > 0
            && self.fixed + self.exceptions == self.rows
            && self.failing == 0
            && self.unfrozen == 0
            && self.roster == 0
            && self.conflicts == 0
    }
}

fn family_tallies(catalog: &[FamilyWitnesses], run: &GateRun) -> BTreeMap<Family, FamilyTally> {
    let mut tallies: BTreeMap<Family, FamilyTally> = BTreeMap::new();
    for entry in catalog {
        tallies.entry(entry.family).or_default().unfrozen += entry.unfrozen.len();
    }
    for row in &run.rows {
        let tally = tallies.entry(row.want.family).or_default();
        tally.rows += 1;
        match row.want.state {
            RowState::Schema(SchemaExpectation::Fixed(_))
            | RowState::FileSize(ObligationExpectation::Met) => tally.fixed += 1,
            RowState::PolicyException { .. } => tally.exceptions += 1,
            _ => tally.known_open += 1,
        }
        if row.have != row.want {
            tally.failing += 1;
        }
    }
    for group in KNOWN_FALSE_ACCEPTANCES {
        tallies.entry(group.family).or_default().roster += group.probes.len();
    }
    for conflict in &run.conflicts {
        for family in &conflict.families {
            tallies.entry(*family).or_default().conflicts += 1;
        }
    }
    tallies
}

/// The deterministic per-family report and the campaign metric.
fn family_report(catalog: &[FamilyWitnesses], run: &GateRun) -> eyre::Result<String> {
    let tallies = family_tallies(catalog, run);
    let mut report = String::new();
    writeln!(
        report,
        "{:<8} {:>5} {:>6} {:>6} {:>10} {:>8} {:>9} {:>7} {:>10}  state",
        "family",
        "rows",
        "fixed",
        "policy",
        "known-open",
        "failing",
        "unfrozen",
        "roster",
        "conflicts"
    )?;
    for (family, tally) in &tallies {
        let state = if !CAMPAIGN_FAMILIES.contains(family) {
            "not counted"
        } else if tally.closed() {
            "CLOSED"
        } else if tally.closed_by_policy() {
            "CLOSED-BY-POLICY"
        } else {
            "open"
        };
        writeln!(
            report,
            "{:<8} {:>5} {:>6} {:>6} {:>10} {:>8} {:>9} {:>7} {:>10}  {state}",
            format!("{family:?}"),
            tally.rows,
            tally.fixed,
            tally.exceptions,
            tally.known_open,
            tally.failing,
            tally.unfrozen,
            tally.roster,
            tally.conflicts,
        )?;
    }
    let mut closed = Vec::new();
    let mut closed_by_policy = Vec::new();
    let mut unfrozen = Vec::new();
    for family in CAMPAIGN_FAMILIES {
        match tallies.get(&family) {
            Some(tally) if tally.closed() => closed.push(format!("{family:?}")),
            Some(tally) if tally.closed_by_policy() => {
                closed_by_policy.push(format!("{family:?}"));
            }
            Some(tally) if tally.rows > 0 => {}
            _ => unfrozen.push(format!("{family:?}")),
        }
    }
    writeln!(
        report,
        "\nfamilies CLOSED: {} / {} (closed: {})",
        closed.len(),
        CAMPAIGN_FAMILIES.len(),
        closed.join(" ")
    )?;
    writeln!(
        report,
        "families CLOSED-BY-POLICY (not in the CLOSED count): {} / {} ({})",
        closed_by_policy.len(),
        CAMPAIGN_FAMILIES.len(),
        closed_by_policy.join(" ")
    )?;
    writeln!(
        report,
        "families with no frozen row ({}): {}",
        unfrozen.len(),
        unfrozen.join(" ")
    )?;
    for entry in catalog {
        for note in entry.unfrozen {
            writeln!(report, "unfrozen {:?}: {note}", entry.family)?;
        }
    }
    for group in KNOWN_FALSE_ACCEPTANCES {
        for probe in group.probes {
            writeln!(
                report,
                "roster {:?}: {}: {} <- {} ({:?}, baseline {:?} at {ROSTER_BASELINE})",
                group.family, group.chart, probe.path, probe.value, group.rejection, group.baseline
            )?;
        }
    }
    for conflict in &run.conflicts {
        writeln!(
            report,
            "conflict {}: {:?} call for acceptance and {:?} for rejection of one composed \
             document ({:?})",
            conflict.chart, conflict.accepts, conflict.rejects, conflict.families
        )?;
    }
    Ok(report)
}

// Failure modes of the gate itself, on a synthetic chart and schemas under
// `tests/fixtures/family_witness_gate`.

const GATE_CHART: &str = "gate";

const DEFAULTS_DIGEST: &str = "b6206bfd7abbc32e";
const ENABLED_XYZ_DIGEST: &str = "e53b03efbea1035b";
const DISABLED_XYZ_DIGEST: &str = "e58d9a4b079f8ab9";
const ITEMS_ZERO_SET_DIGEST: &str = "04bd7f8c3c9662ea";
const ITEMS_ZERO_FILE_DIGEST: &str = "54f1ab80fb17e7b1";
const EDITED_ENABLED_XYZ_DIGEST: &str = "26e70268bd724aa6";

const ENABLED_XYZ: &[SetPair] = &[
    SetPair {
        key: "feature.enabled",
        value: SetValue::Bool(true),
    },
    SetPair {
        key: "feature.items",
        value: SetValue::Str("xyz"),
    },
];

const DISABLED_XYZ: &[SetPair] = &[
    SetPair {
        key: "feature.enabled",
        value: SetValue::Bool(false),
    },
    SetPair {
        key: "feature.items",
        value: SetValue::Str("xyz"),
    },
];

const GATE_RENDERS: OracleExpectation = OracleExpectation::Renders {
    kubernetes: KubernetesExpectation::NotRelevant,
};

const RANGE_ABORTS: OracleExpectation = OracleExpectation::Aborts {
    diagnostic: "range can't iterate over xyz",
};

/// The rows as an accept-everything schema leaves them.
const ACCEPTING_STATE: FamilyWitnesses = FamilyWitnesses {
    family: Family::F0,
    witnesses: &[
        Witness {
            id: "defaults",
            chart: GATE_CHART,
            kubernetes_version: "1.29.0",
            overrides: Overrides::Set(&[]),
            oracle: GATE_RENDERS,
            schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
            adjudicated: DEFAULTS_DIGEST,
        },
        Witness {
            id: "enabled-xyz",
            chart: GATE_CHART,
            kubernetes_version: "1.29.0",
            overrides: Overrides::Set(ENABLED_XYZ),
            oracle: RANGE_ABORTS,
            schema: SchemaExpectation::KnownFalseAcceptance,
            adjudicated: ENABLED_XYZ_DIGEST,
        },
    ],
    size_obligations: &[],
    unfrozen: &[],
};

/// The same rows as a reject-everything schema leaves them.
const REJECTING_STATE: FamilyWitnesses = FamilyWitnesses {
    family: Family::F0,
    witnesses: &[
        Witness {
            id: "defaults",
            chart: GATE_CHART,
            kubernetes_version: "1.29.0",
            overrides: Overrides::Set(&[]),
            oracle: GATE_RENDERS,
            schema: SchemaExpectation::KnownFalseRejection,
            adjudicated: DEFAULTS_DIGEST,
        },
        Witness {
            id: "enabled-xyz",
            chart: GATE_CHART,
            kubernetes_version: "1.29.0",
            overrides: Overrides::Set(ENABLED_XYZ),
            oracle: RANGE_ABORTS,
            schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
            adjudicated: ENABLED_XYZ_DIGEST,
        },
    ],
    size_obligations: &[],
    unfrozen: &[],
};

fn gate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/family_witness_gate")
}

fn gate_layout(schemas: &str) -> Layout {
    gate_layout_with_options(schemas, "accepting")
}

/// A layout whose default fixtures come from `schemas` and whose option
/// fixtures come from `policy_schemas`.
fn gate_layout_with_options(schemas: &str, policy_schemas: &str) -> Layout {
    Layout {
        charts: gate_root().join("charts"),
        schemas: gate_root().join("schemas").join(schemas),
        policy_schemas: gate_root().join("policy-schemas").join(policy_schemas),
        values_files: gate_root().join("values"),
    }
}

/// The rows whose observed state differs from the recorded one, as
/// `(id, have, want)`.
fn state_failures(run: &GateRun) -> Vec<(&'static str, RowState, RowState)> {
    run.rows
        .iter()
        .filter(|row| row.have.state != row.want.state)
        .map(|row| (row.want.id, row.have.state, row.want.state))
        .collect()
}

fn failing_ids(run: &GateRun) -> Vec<&'static str> {
    run.rows
        .iter()
        .filter(|row| row.have != row.want)
        .map(|row| row.want.id)
        .collect()
}

#[test]
fn recorded_states_pass_while_they_hold() -> eyre::Result<()> {
    for (catalog, schemas) in [
        (ACCEPTING_STATE, "accepting"),
        (REJECTING_STATE, "rejecting"),
    ] {
        let run = run_gate(&[catalog], &gate_layout(schemas))?;
        sim_assert_eq!(have: failing_ids(&run), want: Vec::<&str>::new());
    }
    Ok(())
}

#[test]
fn fixed_rows_fail_on_regression_in_both_directions() -> eyre::Result<()> {
    let mut regressions = Vec::new();
    for (catalog, schemas) in [
        (ACCEPTING_STATE, "rejecting"),
        (REJECTING_STATE, "accepting"),
    ] {
        let run = run_gate(&[catalog], &gate_layout(schemas))?;
        for (id, have, want) in state_failures(&run) {
            if matches!(want, RowState::Schema(SchemaExpectation::Fixed(_))) {
                regressions.push((id, have, want));
            }
        }
    }
    sim_assert_eq!(
        have: regressions,
        want: vec![
            (
                "defaults",
                RowState::Schema(SchemaExpectation::KnownFalseRejection),
                RowState::Schema(SchemaExpectation::Fixed(SchemaVerdict::Accepts)),
            ),
            (
                "enabled-xyz",
                RowState::Schema(SchemaExpectation::KnownFalseAcceptance),
                RowState::Schema(SchemaExpectation::Fixed(SchemaVerdict::Rejects)),
            ),
        ]
    );
    Ok(())
}

#[test]
fn known_open_rows_fail_when_fixed_unexpectedly_in_both_directions() -> eyre::Result<()> {
    let mut hints = Vec::new();
    for (catalog, schemas) in [
        (ACCEPTING_STATE, "rejecting"),
        (REJECTING_STATE, "accepting"),
    ] {
        let run = run_gate(&[catalog], &gate_layout(schemas))?;
        for row in &run.rows {
            let known_open = !matches!(
                row.want.state,
                RowState::Schema(SchemaExpectation::Fixed(_))
            );
            if known_open && row.have.state != row.want.state {
                hints.push((row.want.id, failure_hint(&row.have, &row.want)));
            }
        }
    }
    sim_assert_eq!(
        have: hints,
        want: vec![
            (
                "enabled-xyz",
                "unexpected fix: promote the row to Schema(Fixed(Rejects))".to_string(),
            ),
            (
                "defaults",
                "unexpected fix: promote the row to Schema(Fixed(Accepts))".to_string(),
            ),
        ]
    );
    Ok(())
}

#[test]
fn activation_context_keeps_rows_with_one_payload_apart() -> eyre::Result<()> {
    let catalog = FamilyWitnesses {
        family: Family::F0,
        witnesses: &[
            Witness {
                id: "enabled-xyz",
                chart: GATE_CHART,
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(ENABLED_XYZ),
                oracle: RANGE_ABORTS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: ENABLED_XYZ_DIGEST,
            },
            Witness {
                id: "disabled-xyz",
                chart: GATE_CHART,
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(DISABLED_XYZ),
                oracle: GATE_RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: DISABLED_XYZ_DIGEST,
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    };
    let run = run_gate(&[catalog], &gate_layout("toggle"))?;
    sim_assert_eq!(have: failing_ids(&run), want: Vec::<&str>::new());
    let digests: Vec<Option<String>> = run
        .rows
        .iter()
        .map(|row| row.have.adjudicated.clone())
        .collect();
    sim_assert_eq!(
        have: digests,
        want: vec![
            Some(ENABLED_XYZ_DIGEST.to_string()),
            Some(DISABLED_XYZ_DIGEST.to_string()),
        ]
    );

    let repeated = FamilyWitnesses {
        family: Family::F0,
        witnesses: &[
            Witness {
                id: "enabled-xyz",
                chart: GATE_CHART,
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(ENABLED_XYZ),
                oracle: RANGE_ABORTS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: ENABLED_XYZ_DIGEST,
            },
            Witness {
                id: "enabled-xyz-again",
                chart: GATE_CHART,
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(ENABLED_XYZ),
                oracle: RANGE_ABORTS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: ENABLED_XYZ_DIGEST,
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    };
    sim_assert_eq!(
        have: registration_problems(&[repeated], &gate_layout("toggle"))?,
        want: vec!["enabled-xyz-again: the same inputs as enabled-xyz".to_string()]
    );
    Ok(())
}

#[test]
fn overlays_compose_with_null_deletion_and_list_replacement() -> eyre::Result<()> {
    let layout = gate_layout("accepting");
    let chart = layout.chart_dir(GATE_CHART);

    let overlay = Overrides::ValuesFile("F0/null-and-list.yaml").overlay(&layout.values_files)?;
    sim_assert_eq!(
        have: test_util::helm_values::coalesce_chart_values(&chart, overlay)?,
        want: json!({"feature": {"enabled": false, "items": []}, "list": ["c"]})
    );

    let overlay = Overrides::Set(&[SetPair {
        key: "drop",
        value: SetValue::Null,
    }])
    .overlay(&layout.values_files)?;
    sim_assert_eq!(
        have: test_util::helm_values::coalesce_chart_values(&chart, overlay)?,
        want: json!({"feature": {"enabled": false, "items": []}, "list": ["a", "b"]})
    );

    let overlay = Overrides::Set(ENABLED_XYZ).overlay(&layout.values_files)?;
    sim_assert_eq!(
        have: overlay,
        want: json!({"feature": {"enabled": true, "items": "xyz"}})
    );
    let arguments: Vec<String> = ENABLED_XYZ.iter().map(SetPair::argument).collect();
    sim_assert_eq!(
        have: arguments,
        want: vec!["feature.enabled=true".to_string(), "feature.items=xyz".to_string()]
    );
    Ok(())
}

/// The overlay reaches the coalescer sparse, so an override that disables a
/// dependency also drops that dependency's defaults. The expected documents
/// are what Helm v4.2.3 hands the templates (`.Values | toJson`).
#[test]
fn dependency_activation_follows_the_sparse_overlay() -> eyre::Result<()> {
    let layout = gate_layout("accepting");
    let chart = layout.chart_dir("umbrella");

    let overlay = Overrides::Set(&[]).overlay(&layout.values_files)?;
    sim_assert_eq!(
        have: test_util::helm_values::coalesce_chart_values(&chart, overlay)?,
        want: json!({"sub": {"enabled": true, "global": {}, "replicas": 1}})
    );

    let overlay = Overrides::Set(&[SetPair {
        key: "sub.enabled",
        value: SetValue::Bool(false),
    }])
    .overlay(&layout.values_files)?;
    sim_assert_eq!(
        have: test_util::helm_values::coalesce_chart_values(&chart, overlay)?,
        want: json!({"sub": {"enabled": false}})
    );
    Ok(())
}

#[test]
fn transports_composing_alike_with_opposite_oracles_block_closure() -> eyre::Result<()> {
    // `--set feature.items=0` reaches the templates as an int64 and a values
    // file's `0` as a float64; Helm may treat them differently, while both
    // compose to one JSON document.
    let catalog = FamilyWitnesses {
        family: Family::F0,
        witnesses: &[
            Witness {
                id: "items-zero-set",
                chart: GATE_CHART,
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[SetPair {
                    key: "feature.items",
                    value: SetValue::Int(0),
                }]),
                oracle: GATE_RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: ITEMS_ZERO_SET_DIGEST,
            },
            Witness {
                id: "items-zero-file",
                chart: GATE_CHART,
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F0/items-zero-file.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "range can't iterate over 0",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: ITEMS_ZERO_FILE_DIGEST,
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    };
    let catalog = [catalog];
    let run = run_gate(&catalog, &gate_layout("accepting"))?;
    sim_assert_eq!(have: failing_ids(&run), want: Vec::<&str>::new());
    sim_assert_eq!(
        have: &run.conflicts,
        want: &vec![Conflict {
            chart: GATE_CHART,
            accepts: vec!["items-zero-set"],
            rejects: vec!["items-zero-file"],
            families: BTreeSet::from([Family::F0]),
        }]
    );
    let tallies = family_tallies(&catalog, &run);
    sim_assert_eq!(
        have: tallies.get(&Family::F0),
        want: Some(&FamilyTally {
            rows: 2,
            fixed: 1,
            exceptions: 0,
            known_open: 1,
            failing: 0,
            unfrozen: 0,
            roster: 0,
            conflicts: 1,
        })
    );
    Ok(())
}

#[test]
fn a_family_closes_only_when_every_row_is_fixed_and_nothing_is_unfrozen() -> eyre::Result<()> {
    let fixed_only = FamilyWitnesses {
        family: Family::F0,
        witnesses: &[Witness {
            id: "defaults",
            chart: GATE_CHART,
            kubernetes_version: "1.29.0",
            overrides: Overrides::Set(&[]),
            oracle: GATE_RENDERS,
            schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
            adjudicated: DEFAULTS_DIGEST,
        }],
        size_obligations: &[],
        unfrozen: &[],
    };
    let with_note = FamilyWitnesses {
        unfrozen: &["a witness Helm could not check yet"],
        ..fixed_only
    };
    let mut closed = Vec::new();
    for (catalog, schemas) in [
        (fixed_only, "accepting"),
        (with_note, "accepting"),
        (ACCEPTING_STATE, "accepting"),
        (REJECTING_STATE, "accepting"),
    ] {
        let catalog = [catalog];
        let run = run_gate(&catalog, &gate_layout(schemas))?;
        let tallies = family_tallies(&catalog, &run);
        closed.push(tallies.get(&Family::F0).is_some_and(FamilyTally::closed));
    }
    sim_assert_eq!(have: closed, want: vec![true, false, false, false]);
    Ok(())
}

/// Writes a schema Helm reads in exactly `size` bytes. The file itself is
/// pretty-printed and larger, so only a gate that measures the shipped
/// compact form sees `size`.
fn write_schema_shipping_at(path: &Path, size: usize) -> eyre::Result<()> {
    let draft = "http://json-schema.org/draft-07/schema#";
    let base = serde_json::to_vec(&json!({"$schema": draft, "description": ""}))?.len();
    let schema = json!({"$schema": draft, "description": " ".repeat(size - base)});
    std::fs::write(path, serde_json::to_vec_pretty(&schema)?)?;
    sim_assert_eq!(have: shipped_bytes(&schema)?, want: size);
    Ok(())
}

#[test]
fn size_obligation_follows_helms_file_limit() -> eyre::Result<()> {
    let schemas = test_util::scratch::ScratchDir::new("family-witness-size")?;
    let layout = Layout {
        charts: gate_root().join("charts"),
        schemas: schemas.path().to_path_buf(),
        policy_schemas: gate_root().join("policy-schemas").join("accepting"),
        values_files: gate_root().join("values"),
    };
    let met = FamilyWitnesses {
        family: Family::F74,
        witnesses: &[Witness {
            id: "defaults",
            chart: GATE_CHART,
            kubernetes_version: "1.29.0",
            overrides: Overrides::Set(&[]),
            oracle: GATE_RENDERS,
            schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
            adjudicated: DEFAULTS_DIGEST,
        }],
        size_obligations: &[SizeObligation {
            id: "gate-fixture-size",
            chart: GATE_CHART,
            expectation: ObligationExpectation::Met,
        }],
        unfrozen: &[],
    };
    let unmet = FamilyWitnesses {
        witnesses: &[],
        size_obligations: &[SizeObligation {
            id: "gate-fixture-size",
            chart: GATE_CHART,
            expectation: ObligationExpectation::KnownUnmet,
        }],
        ..met
    };

    write_schema_shipping_at(&layout.fixture(GATE_CHART), HELM_MAX_CHART_FILE_BYTES)?;
    let run = run_gate(std::slice::from_ref(&met), &layout)?;
    sim_assert_eq!(have: failing_ids(&run), want: Vec::<&str>::new());
    let run = run_gate(std::slice::from_ref(&unmet), &layout)?;
    sim_assert_eq!(
        have: state_failures(&run),
        want: vec![(
            "gate-fixture-size",
            RowState::FileSize(ObligationExpectation::Met),
            RowState::FileSize(ObligationExpectation::KnownUnmet),
        )]
    );

    // One shipped byte over: Helm refuses the file, so every values document
    // fails.
    write_schema_shipping_at(&layout.fixture(GATE_CHART), HELM_MAX_CHART_FILE_BYTES + 1)?;
    let run = run_gate(std::slice::from_ref(&met), &layout)?;
    sim_assert_eq!(
        have: state_failures(&run),
        want: vec![
            (
                "defaults",
                RowState::FixtureUnusable,
                RowState::Schema(SchemaExpectation::Fixed(SchemaVerdict::Accepts)),
            ),
            (
                "gate-fixture-size",
                RowState::FileSize(ObligationExpectation::KnownUnmet),
                RowState::FileSize(ObligationExpectation::Met),
            ),
        ]
    );
    Ok(())
}

/// A fixture Helm refuses judges nothing: it can neither satisfy a
/// `Fixed(Rejects)` row nor keep a known false rejection quietly in place.
#[test]
fn an_unusable_fixture_fails_every_row_on_its_chart() -> eyre::Result<()> {
    let run = run_gate(&[REJECTING_STATE], &gate_layout("broken"))?;
    sim_assert_eq!(
        have: state_failures(&run),
        want: vec![
            (
                "defaults",
                RowState::FixtureUnusable,
                RowState::Schema(SchemaExpectation::KnownFalseRejection),
            ),
            (
                "enabled-xyz",
                RowState::FixtureUnusable,
                RowState::Schema(SchemaExpectation::Fixed(SchemaVerdict::Rejects)),
            ),
        ]
    );
    let catalog = [REJECTING_STATE];
    let tallies = family_tallies(&catalog, &run);
    sim_assert_eq!(
        have: tallies.get(&Family::F0).map(|tally| (tally.failing, tally.closed())),
        want: Some((2, false))
    );
    Ok(())
}

const UNRESOLVED_DEFAULTS: FamilyWitnesses = FamilyWitnesses {
    family: Family::F0,
    witnesses: &[Witness {
        id: "defaults",
        chart: GATE_CHART,
        kubernetes_version: "1.29.0",
        overrides: Overrides::Set(&[]),
        oracle: GATE_RENDERS,
        schema: SchemaExpectation::PolicyUnresolved {
            current: SchemaVerdict::Rejects,
            question: "may the schema reject a document Helm renders?",
        },
        adjudicated: DEFAULTS_DIGEST,
    }],
    size_obligations: &[],
    unfrozen: &[],
};

/// A render whose schema target is an open policy question is neither a
/// false rejection nor fixed: its current verdict is pinned both ways, and
/// the family stays open.
#[test]
fn an_unresolved_policy_pins_the_verdict_and_blocks_closure() -> eyre::Result<()> {
    let catalog = [UNRESOLVED_DEFAULTS];
    let run = run_gate(&catalog, &gate_layout("rejecting"))?;
    sim_assert_eq!(have: failing_ids(&run), want: Vec::<&str>::new());
    sim_assert_eq!(
        have: family_tallies(&catalog, &run)
            .get(&Family::F0)
            .map(FamilyTally::closed),
        want: Some(false)
    );

    let run = run_gate(&catalog, &gate_layout("accepting"))?;
    let hints: Vec<String> = run
        .rows
        .iter()
        .map(|row| failure_hint(&row.have, &row.want))
        .collect();
    sim_assert_eq!(
        have: state_failures(&run),
        want: vec![(
            "defaults",
            RowState::Schema(SchemaExpectation::PolicyUnresolved {
                current: SchemaVerdict::Accepts,
                question: "may the schema reject a document Helm renders?",
            }),
            RowState::Schema(SchemaExpectation::PolicyUnresolved {
                current: SchemaVerdict::Rejects,
                question: "may the schema reject a document Helm renders?",
            }),
        )]
    );
    sim_assert_eq!(
        have: hints,
        want: vec![
            "the verdict moved on a row whose policy is unresolved: decide the policy, then \
             record the row's state"
                .to_string()
        ]
    );
    Ok(())
}

const EXCEPTION_DEFAULTS: FamilyWitnesses = FamilyWitnesses {
    family: Family::F0,
    witnesses: &[Witness {
        id: "defaults",
        chart: GATE_CHART,
        kubernetes_version: "1.29.0",
        overrides: Overrides::Set(&[]),
        oracle: GATE_RENDERS,
        schema: SchemaExpectation::PolicyException {
            option: PolicyOption::OpenRoot,
        },
        adjudicated: DEFAULTS_DIGEST,
    }],
    size_obligations: &[],
    unfrozen: &[],
};

const EXCEPTION_HOLDS: RowState = RowState::PolicyException {
    option: PolicyOption::OpenRoot,
    default: Some(SchemaVerdict::Rejects),
    with_option: Some(SchemaVerdict::Accepts),
};

/// A decided policy exception passes only while the default fixture rejects
/// and the option fixture accepts the same composed document. It closes its
/// family by policy, never in the CLOSED count, and either evaluation
/// failing fails the row.
#[test]
fn a_policy_exception_needs_both_evaluations() -> eyre::Result<()> {
    let catalog = [EXCEPTION_DEFAULTS];
    let mut outcomes = Vec::new();
    let mut hints = Vec::new();
    for (schemas, policy_schemas) in [
        ("rejecting", "accepting"),
        ("accepting", "accepting"),
        ("rejecting", "rejecting"),
    ] {
        let run = run_gate(&catalog, &gate_layout_with_options(schemas, policy_schemas))?;
        let tally = family_tallies(&catalog, &run)
            .remove(&Family::F0)
            .unwrap_or_default();
        outcomes.push((
            state_failures(&run),
            tally.closed(),
            tally.closed_by_policy(),
        ));
        hints.extend(
            run.rows
                .iter()
                .filter(|row| row.have != row.want)
                .map(|row| failure_hint(&row.have, &row.want)),
        );
    }
    sim_assert_eq!(
        have: outcomes,
        want: vec![
            (Vec::new(), false, true),
            (
                vec![(
                    "defaults",
                    RowState::PolicyException {
                        option: PolicyOption::OpenRoot,
                        default: Some(SchemaVerdict::Accepts),
                        with_option: Some(SchemaVerdict::Accepts),
                    },
                    EXCEPTION_HOLDS,
                )],
                false,
                false,
            ),
            (
                vec![(
                    "defaults",
                    RowState::PolicyException {
                        option: PolicyOption::OpenRoot,
                        default: Some(SchemaVerdict::Rejects),
                        with_option: Some(SchemaVerdict::Rejects),
                    },
                    EXCEPTION_HOLDS,
                )],
                false,
                false,
            ),
        ]
    );
    let expected_hint = "a policy exception must reject under the default fixture and accept \
                         under the fixture generated with its option";
    sim_assert_eq!(
        have: hints,
        want: vec![expected_hint.to_string(), expected_hint.to_string()]
    );
    Ok(())
}

/// An exception is judged only against an option fixture that exists and
/// records, in its generated policy annotation, the option it stands for.
#[test]
fn a_policy_exception_needs_its_labelled_option_fixture() -> eyre::Result<()> {
    let catalog = [EXCEPTION_DEFAULTS];
    let mut problems = Vec::new();
    for policy_schemas in ["missing", "mislabelled"] {
        problems.extend(registration_problems(
            &catalog,
            &gate_layout_with_options("rejecting", policy_schemas),
        )?);
    }
    sim_assert_eq!(
        have: problems,
        want: vec![
            "chart gate: no open-root option fixture".to_string(),
            "chart gate: the open-root option fixture records authoring \
             {\"declared-types\":\"assert\",\"root\":\"closed\"} instead"
                .to_string(),
        ]
    );
    let run = run_gate(&catalog, &gate_layout_with_options("rejecting", "missing"));
    sim_assert_eq!(have: run.is_err(), want: true);
    Ok(())
}

/// A deliberate default rejection of a render is a policy exception, not a
/// fix: registering it as `Fixed(Rejects)` contradicts the oracle, and an
/// exception over an aborting oracle has no acceptance to restore.
#[test]
fn a_policy_exception_is_never_registered_as_a_fixed_rejection() -> eyre::Result<()> {
    let catalog = [FamilyWitnesses {
        family: Family::F0,
        witnesses: &[
            Witness {
                id: "defaults",
                chart: GATE_CHART,
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: GATE_RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: DEFAULTS_DIGEST,
            },
            Witness {
                id: "enabled-xyz",
                chart: GATE_CHART,
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(ENABLED_XYZ),
                oracle: RANGE_ABORTS,
                schema: SchemaExpectation::PolicyException {
                    option: PolicyOption::OpenRoot,
                },
                adjudicated: ENABLED_XYZ_DIGEST,
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    }];
    sim_assert_eq!(
        have: registration_problems(&catalog, &gate_layout("rejecting"))?,
        want: vec![
            "defaults: Fixed(Rejects) contradicts an oracle that calls for Accepts".to_string(),
            "enabled-xyz: PolicyException { option: OpenRoot } contradicts an oracle that calls \
             for Rejects"
                .to_string(),
        ]
    );
    Ok(())
}

#[test]
fn editing_a_witness_invalidates_its_adjudication() -> eyre::Result<()> {
    let edited = FamilyWitnesses {
        family: Family::F0,
        witnesses: &[Witness {
            id: "enabled-xyz",
            chart: GATE_CHART,
            kubernetes_version: "1.29.0",
            overrides: Overrides::Set(ENABLED_XYZ),
            oracle: OracleExpectation::Aborts {
                diagnostic: "range can't iterate over abc",
            },
            schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
            adjudicated: ENABLED_XYZ_DIGEST,
        }],
        size_obligations: &[],
        unfrozen: &[],
    };
    let run = run_gate(&[edited], &gate_layout("rejecting"))?;
    let pins: Vec<_> = run
        .rows
        .iter()
        .map(|row| {
            (
                row.want.id,
                row.have.adjudicated.clone(),
                row.want.adjudicated.clone(),
            )
        })
        .collect();
    sim_assert_eq!(
        have: pins,
        want: vec![(
            "enabled-xyz",
            Some(EDITED_ENABLED_XYZ_DIGEST.to_string()),
            Some(ENABLED_XYZ_DIGEST.to_string()),
        )]
    );
    Ok(())
}

#[test]
fn registration_rejects_malformed_rows() -> eyre::Result<()> {
    let catalog = [
        FamilyWitnesses {
            family: Family::F1,
            witnesses: &[
                Witness {
                    id: "abort-called-false-rejection",
                    chart: GATE_CHART,
                    kubernetes_version: "1.29.0",
                    overrides: Overrides::Set(ENABLED_XYZ),
                    oracle: RANGE_ABORTS,
                    schema: SchemaExpectation::KnownFalseRejection,
                    adjudicated: ENABLED_XYZ_DIGEST,
                },
                Witness {
                    id: "empty-diagnostic",
                    chart: GATE_CHART,
                    kubernetes_version: "1.29.0",
                    overrides: Overrides::Set(&[SetPair {
                        key: "feature.enabled",
                        value: SetValue::Bool(true),
                    }]),
                    oracle: OracleExpectation::Aborts { diagnostic: "" },
                    schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                    adjudicated: "0000000000000000",
                },
                Witness {
                    id: "retyped-string",
                    chart: GATE_CHART,
                    kubernetes_version: "1.30.0",
                    overrides: Overrides::Set(&[SetPair {
                        key: "feature.items",
                        value: SetValue::Str("3"),
                    }]),
                    oracle: GATE_RENDERS,
                    schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                    adjudicated: "",
                },
                Witness {
                    id: "packaged-defaults",
                    chart: "packaged",
                    kubernetes_version: "1.29.0",
                    overrides: Overrides::Set(&[]),
                    oracle: GATE_RENDERS,
                    schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                    adjudicated: "0000000000000000",
                },
            ],
            size_obligations: &[],
            unfrozen: &[],
        },
        FamilyWitnesses {
            family: Family::F0,
            witnesses: &[
                Witness {
                    id: "misfiled",
                    chart: GATE_CHART,
                    kubernetes_version: "1.29.0",
                    overrides: Overrides::ValuesFile("F1/misfiled.yaml"),
                    oracle: GATE_RENDERS,
                    schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                    adjudicated: "0000000000000000",
                },
                Witness {
                    id: "retyped-string",
                    chart: GATE_CHART,
                    kubernetes_version: "1.29.0",
                    overrides: Overrides::Set(&[]),
                    oracle: GATE_RENDERS,
                    schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                    adjudicated: DEFAULTS_DIGEST,
                },
            ],
            size_obligations: &[],
            unfrozen: &[],
        },
    ];
    let problems = registration_problems(&catalog, &gate_layout("accepting"))?;
    sim_assert_eq!(
        have: problems,
        want: vec![
            "families must be listed once, in order: F1 before F0".to_string(),
            "abort-called-false-rejection: KnownFalseRejection contradicts an oracle that calls \
             for Rejects"
                .to_string(),
            "empty-diagnostic: an abort needs its Helm diagnostic".to_string(),
            "retyped-string: adjudicated under Kubernetes 1.30.0, but the chart renders under \
             1.29.0"
                .to_string(),
            "retyped-string: --set feature.items=3 would not reach Helm as that string"
                .to_string(),
            "retyped-string: never adjudicated".to_string(),
            "chart packaged: import-values in Chart.yaml is not modelled".to_string(),
            "chart packaged: packaged dependency charts/dep-0.1.0.tgz is not modelled"
                .to_string(),
            "misfiled: a values file is F0/misfiled.yaml, not F1/misfiled.yaml".to_string(),
            "misfiled: Fixed(Rejects) contradicts an oracle that calls for Accepts".to_string(),
            "retyped-string: duplicate id".to_string(),
        ]
    );
    Ok(())
}

//! The version-1 value-algebra control runner: answers one observation
//! request with this revision's own parse and fragment evaluation.
//!
//! `value_algebra_diff` invokes the ignored [`protocol_v1`] entry point of
//! this crate's debug test executable. The adapter evaluates every rendered
//! template of the frozen chart once and only reads the executed result:
//! guarded fragments and the observed fail captures. It never walks template
//! expressions again, schedules dependency charts or coalesces values.
//!
//! The legacy fragment does not record whether a render completed, so a
//! normal exit and an emitted value are exact only when every rendered file
//! is action-free source the fragment reproduces verbatim. A definite abort
//! is exact when a fail capture executes unconditionally. Everything else is
//! an explicit representation gap.

use std::collections::BTreeMap;

use color_eyre::eyre;
use helm_schema_ast::DefineIndex;
use helm_schema_core::PredicateKind;
use helm_schema_test_support::value_algebra_diff::controls::{
    self, AbortPhase, CacheMode, Capabilities, ControlBlobs, ControlInput, ControlResult,
    EngineMode, ExactCoverage, ExecutionEvidence, InfrastructureCode, Observation,
    ProjectionStatus, Query, Selection, Termination, UnknownReason,
};
use helm_schema_test_support::value_algebra_diff::history::canonical_json;
use indoc::{formatdoc, indoc};
use serde::Deserialize as _;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use crate::SymbolicIrContext;
use crate::eval_effect::CaptureKind;
use crate::fragment_eval::{
    AbstractFragment, EntryKey, EvaluatedDocument, Guarded, StringPart, dump_document,
};

/// The evaluator named in execution evidence.
const EVALUATOR: &str = "helm-schema-ir legacy fragment evaluator, controls v1";

/// The runner entry point; it does nothing without a request.
#[test]
#[ignore = "value_algebra_diff's control runner; it needs HELM_SCHEMA_CONTROL_REQUEST"]
fn protocol_v1() -> eyre::Result<()> {
    controls::serve(controls::executable_sha256()?, &capabilities(), observe)?;
    Ok(())
}

fn capabilities() -> Capabilities {
    Capabilities {
        queries: controls::QUERY_KINDS
            .iter()
            .map(ToString::to_string)
            .collect(),
        modes: vec![EngineMode::Render],
        cache_modes: vec![CacheMode::Default, CacheMode::Disabled],
    }
}

/// What a template file's actions are, as its syntax tree shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Actions {
    /// No action: the file is its own output.
    None,
    /// Only top-level `{{ fail "literal" }}` actions: Go's parser accepts the
    /// file, and executing it aborts unconditionally.
    FailOnly,
    /// Anything else, which the adapter cannot prove Go parses or completes.
    Other,
}

/// One rendered template's execution.
struct FileRun {
    source: String,
    actions: Actions,
    document: EvaluatedDocument,
}

/// The whole chart's execution: rendered files by chart-relative name, the
/// worst actions of any template file, partials included, and why the
/// result cannot prove completion.
struct Execution {
    files: BTreeMap<String, FileRun>,
    partials: Actions,
    gaps: Vec<String>,
    result_sha256: String,
}

/// Classifies `source`'s actions from its syntax tree.
fn actions_of(source: &str) -> Actions {
    use helm_schema_ast::{Literal, TemplateExpr};
    let Some(tree) = helm_schema_syntax::parse_go_template(source) else {
        return Actions::Other;
    };
    let root = tree.root_node();
    if root.has_error() {
        return Actions::Other;
    }
    let mut actions = Actions::None;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "text" | "{{" | "{{-" | "}}" | "-}}" => {}
            "function_call" => {
                let expressions = helm_schema_ast::node_expressions(child, source);
                let fail = matches!(
                    expressions.as_slice(),
                    [TemplateExpr::Call { function, args }]
                        if function == "fail"
                            && matches!(args.as_slice(), [TemplateExpr::Literal(Literal::String(_))])
                );
                if !fail {
                    return Actions::Other;
                }
                actions = Actions::FailOnly;
            }
            _ => return Actions::Other,
        }
    }
    actions
}

/// Answers one query; the protocol already verified every blob.
fn observe(blobs: &ControlBlobs, input: &ControlInput, query: &Query) -> ControlResult {
    let chart = match blobs.chart(input) {
        Ok(chart) => chart,
        Err(error) => {
            return ControlResult::InfrastructureFailure {
                code: InfrastructureCode::InputMismatch,
                diagnostic: error.to_string(),
            };
        }
    };
    let execution = execute(&chart.files);
    let termination = termination(&execution);
    match query {
        Query::TerminationV1 { .. } => match termination {
            Ok(outcome) => exact(Observation::Termination { outcome }),
            Err(gap) => unknown(&execution, gap),
        },
        Query::OutputV1 { file, pointer, .. } => {
            match select(&execution, termination, file, pointer) {
                Ok(Selected::Abort(phase)) => exact(Observation::Output {
                    selection: Selection::NotEmitted { phase },
                }),
                Ok(Selected::Missing) => exact(Observation::Output {
                    selection: Selection::Missing,
                }),
                Ok(Selected::Value(serde_yaml::Value::String(text))) => {
                    exact(Observation::Output {
                        selection: controls::output_value(text.as_bytes()),
                    })
                }
                Ok(Selected::Value(other)) => wrong_type(&execution, &other, |selection| {
                    Observation::Output { selection }
                }),
                Err(gap) => unknown(&execution, gap),
            }
        }
        Query::AliasV1 { file, pointer, .. } => {
            match select(&execution, termination, file, pointer) {
                Ok(Selected::Abort(phase)) => exact(Observation::Alias {
                    selection: Selection::NotEmitted { phase },
                }),
                Ok(Selected::Missing) => exact(Observation::Alias {
                    selection: Selection::Missing,
                }),
                Ok(Selected::Value(serde_yaml::Value::Bool(value))) => exact(Observation::Alias {
                    selection: Selection::Value { value },
                }),
                Ok(Selected::Value(other)) => wrong_type(&execution, &other, |selection| {
                    Observation::Alias { selection }
                }),
                Err(gap) => unknown(&execution, gap),
            }
        }
    }
}

fn exact(observation: Observation) -> ControlResult {
    ControlResult::Exact {
        alternatives: vec![observation],
        coverage: ExactCoverage::Complete,
    }
}

fn unknown(execution: &Execution, gap: String) -> ControlResult {
    let mut gaps = execution.gaps.clone();
    gaps.push(gap);
    gaps.sort();
    gaps.dedup();
    ControlResult::AbstractUnknown {
        reason: UnknownReason::RepresentationGap,
        evidence: ExecutionEvidence {
            evaluator: EVALUATOR.into(),
            result_sha256: execution.result_sha256.clone(),
            gaps,
            projection: ProjectionStatus::ReadOnly,
        },
    }
}

fn wrong_type<T>(
    execution: &Execution,
    value: &serde_yaml::Value,
    observation: impl FnOnce(Selection<T>) -> Observation,
) -> ControlResult {
    match serde_json::to_value(value) {
        Ok(value) => exact(observation(Selection::WrongType { value })),
        Err(error) => unknown(
            execution,
            format!("selected value has no JSON form: {error}"),
        ),
    }
}

/// Evaluates every rendered template once.
fn execute(files: &BTreeMap<String, Vec<u8>>) -> Execution {
    let mut gaps = Vec::new();
    if files.keys().any(|name| name.starts_with("charts/")) {
        gaps.push("dependency charts are not scheduled by the legacy evaluator".to_string());
    }
    if files.contains_key(".helmignore") {
        gaps.push(".helmignore file selection is not modeled by the adapter".to_string());
    }
    let mut index = DefineIndex::new();
    let mut sources = BTreeMap::new();
    let mut partials = Actions::None;
    for (name, bytes) in files {
        if !name.starts_with("templates/") {
            continue;
        }
        let Ok(source) = std::str::from_utf8(bytes) else {
            gaps.push(format!("{name} is not UTF-8"));
            continue;
        };
        index.add_file_source(name, source);
        let partial = name
            .rsplit('/')
            .next()
            .is_some_and(|base| base.starts_with('_'));
        if partial {
            partials = match (partials, actions_of(source)) {
                (Actions::Other, _) | (_, Actions::Other) => Actions::Other,
                (Actions::FailOnly, _) | (_, Actions::FailOnly) => Actions::FailOnly,
                (Actions::None, Actions::None) => Actions::None,
            };
        } else {
            sources.insert(name.clone(), source.to_string());
        }
    }
    let context = SymbolicIrContext::new(&index);
    let mut runs = BTreeMap::new();
    let mut dump = String::new();
    for (name, source) in sources {
        let actions = actions_of(&source);
        let document = context.eval_document_fragment(&source);
        dump.push_str(&["file ", &name, "\n", &dump_document(&document), "\n"].concat());
        runs.insert(
            name,
            FileRun {
                source,
                actions,
                document,
            },
        );
    }
    Execution {
        files: runs,
        partials,
        gaps,
        result_sha256: helm_schema_test_support::source_digest::sha256_hex(dump.as_bytes()),
    }
}

/// The render's exit, or why the executed result cannot prove it.
///
/// Go parses every template file before executing any, so an exit is
/// claimed only when every file's syntax is provably Go-parseable: action-
/// free, or only literal `fail` actions. An abort additionally needs a
/// rendered file's executed fail capture to hold unconditionally; a normal
/// exit needs every rendered file to be action-free YAML whose documents are
/// mappings Helm's manifest splitter accepts.
fn termination(execution: &Execution) -> Result<Termination, String> {
    if let Some(gap) = execution.gaps.first() {
        return Err(gap.clone());
    }
    if execution.partials == Actions::Other {
        return Err("a partial template's actions are not provably Go-parseable".into());
    }
    if let Some((name, _)) = execution
        .files
        .iter()
        .find(|(_, run)| run.actions == Actions::Other)
    {
        return Err(format!(
            "{name} executes template actions; the legacy fragment does not record completion"
        ));
    }
    let unconditional_fail = execution.files.values().any(|run| {
        run.actions == Actions::FailOnly
            && run.document.observed_facts.captures.iter().any(|capture| {
                matches!(capture.kind, CaptureKind::Fail)
                    && capture.conjunction.is_empty()
                    && capture.ranged.iter().next().is_none()
            })
    });
    if unconditional_fail {
        return Ok(Termination::Abort {
            phase: AbortPhase::TemplateEval,
        });
    }
    if let Some((name, _)) = execution
        .files
        .iter()
        .find(|(_, run)| run.actions != Actions::None)
    {
        return Err(format!("{name}: its fail captures do not prove an abort"));
    }
    for (name, run) in &execution.files {
        for document in serde_yaml::Deserializer::from_str(&run.source) {
            let document = serde_yaml::Value::deserialize(document)
                .map_err(|error| format!("{name}: rendered YAML parity is unverified: {error}"))?;
            if let Some(problem) = helm_header_problem(&document) {
                return Err(format!("{name}: {problem}"));
            }
        }
    }
    Ok(Termination::Normal)
}

/// Why Helm 4.2.3's `SimpleHead` decoding of a rendered document is not
/// proven to succeed: `apiVersion` and `kind` must be unambiguous strings,
/// `metadata` a mapping whose `name` is a string and whose `annotations`
/// map strings to strings, and no hook annotation may steer the render.
/// Case-folded field aliases and non-finite numbers remain unverified.
/// An empty document is skipped by Helm. `None` when it provably decodes.
fn helm_header_problem(document: &serde_yaml::Value) -> Option<String> {
    if !yaml_numbers_finite(document) || serde_json::to_value(document).is_err() {
        return Some("the rendered document is outside Helm's JSON loader domain".into());
    }
    let text = |value: Option<&serde_yaml::Value>, optional: bool| match value {
        None | Some(serde_yaml::Value::Null) => optional,
        Some(serde_yaml::Value::String(text)) => !text.is_empty() && !yaml_one_one_ambiguous(text),
        Some(_) => false,
    };
    let header = match document {
        serde_yaml::Value::Null => return None,
        serde_yaml::Value::Mapping(header) => header,
        _ => return Some("a document Helm's manifest head cannot decode".into()),
    };
    if !header_keys_supported(header, &["apiVersion", "kind", "metadata"]) {
        return Some("manifest header field aliases are unverified".into());
    }
    if !text(header.get("apiVersion"), false) || !text(header.get("kind"), false) {
        return Some("apiVersion and kind are not proven strings".into());
    }
    let metadata = match header.get("metadata") {
        None | Some(serde_yaml::Value::Null) => return None,
        Some(serde_yaml::Value::Mapping(metadata)) => metadata,
        Some(_) => return Some("metadata is not a mapping".into()),
    };
    if !header_keys_supported(metadata, &["name", "annotations"]) {
        return Some("metadata field aliases are unverified".into());
    }
    if !text(metadata.get("name"), true) {
        return Some("metadata.name is not a proven string".into());
    }
    match metadata.get("annotations") {
        None | Some(serde_yaml::Value::Null) => None,
        Some(serde_yaml::Value::Mapping(annotations)) => {
            let proven = annotations.iter().all(|(key, value)| {
                matches!(key, serde_yaml::Value::String(key) if !key.starts_with("helm.sh/hook"))
                    && text(Some(value), true)
            });
            (!proven).then(|| "metadata.annotations are not proven string pairs".into())
        }
        Some(_) => Some("metadata.annotations is not a mapping".into()),
    }
}

/// Go folds struct field names, including Unicode aliases of ASCII letters.
/// Only ASCII string keys without aliases of the supported fields are proven.
fn header_keys_supported(mapping: &serde_yaml::Mapping, fields: &[&str]) -> bool {
    mapping.keys().all(|key| {
        let serde_yaml::Value::String(key) = key else {
            return false;
        };
        key.is_ascii()
            && !fields
                .iter()
                .any(|field| key != field && key.eq_ignore_ascii_case(field))
    })
}

/// Rejects non-finite numbers before JSON serialization can replace them with null.
fn yaml_numbers_finite(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Null | serde_yaml::Value::Bool(_) | serde_yaml::Value::String(_) => true,
        serde_yaml::Value::Number(number) => number.as_f64().is_some_and(f64::is_finite),
        serde_yaml::Value::Sequence(values) => values.iter().all(yaml_numbers_finite),
        serde_yaml::Value::Mapping(values) => values
            .iter()
            .all(|(key, value)| yaml_numbers_finite(key) && yaml_numbers_finite(value)),
        serde_yaml::Value::Tagged(tagged) => yaml_numbers_finite(&tagged.value),
    }
}

/// Whether the plain spelling `text` may decode to something other than
/// this string in YAML 1.1.
fn yaml_one_one_ambiguous(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "y" | "yes" | "n" | "no" | "on" | "off" | "true" | "false" | "null" | "~"
    ) || text
        .chars()
        .all(|character| character.is_ascii_digit() || "+-.:_eEoOxX".contains(character))
}

/// What a selection found once the render's exit is known.
enum Selected {
    Abort(AbortPhase),
    Missing,
    Value(serde_yaml::Value),
}

/// Selects `pointer` in `file`'s single executed document.
fn select(
    execution: &Execution,
    termination: Result<Termination, String>,
    file: &str,
    pointer: &str,
) -> Result<Selected, String> {
    match termination? {
        Termination::Abort { phase } => return Ok(Selected::Abort(phase)),
        Termination::Normal => {}
    }
    let Some(run) = execution.files.get(file) else {
        return Ok(Selected::Missing);
    };
    let documents = serde_yaml::Deserializer::from_str(&run.source).count();
    if documents != 1 {
        return Err(format!(
            "{file} has {documents} YAML documents; the legacy fragment merges documents"
        ));
    }
    let mut node = single_always(&run.document.root)?;
    for segment in pointer_segments(pointer)? {
        let next = match node {
            AbstractFragment::Mapping(mapping) => {
                let mut found = None;
                for entry in &mapping.entries {
                    let EntryKey::Literal(raw) = &entry.key else {
                        return Err("a templated key is not decided by the legacy fragment".into());
                    };
                    if decode_plain_or_quoted(raw)? == serde_yaml::Value::String(segment.clone()) {
                        found = Some(&entry.value);
                        break;
                    }
                }
                found
            }
            AbstractFragment::Sequence(sequence) => segment
                .parse::<usize>()
                .ok()
                .and_then(|index| sequence.items.get(index)),
            AbstractFragment::Scalar(_)
            | AbstractFragment::Splice(_)
            | AbstractFragment::Opaque(_) => {
                return Err("the pointer passes through a scalar or unknown fragment".into());
            }
        };
        let Some(next) = next else {
            return Ok(Selected::Missing);
        };
        node = single_always(next)?;
    }
    let AbstractFragment::Scalar(scalar) = node else {
        return Err("only scalar selections are read from the legacy fragment".into());
    };
    if scalar.suppressed {
        return Err("a block scalar's text is not recorded verbatim".into());
    }
    let mut raw = String::new();
    for part in &scalar.parts {
        match part {
            StringPart::Text(alternatives) if alternatives.len() == 1 => {
                raw.extend(alternatives.iter().map(String::as_str));
            }
            StringPart::Text(_) | StringPart::Splice(_) | StringPart::Taint(_) => {
                return Err("the selected scalar is not one literal".into());
            }
        }
    }
    decode_plain_or_quoted(&raw).map(Selected::Value)
}

/// The one unconditional arm of `guarded`.
fn single_always(guarded: &Guarded<AbstractFragment>) -> Result<&AbstractFragment, String> {
    match guarded.arms.as_slice() {
        [(condition, fragment)] if matches!(condition.kind(), PredicateKind::True) => Ok(fragment),
        _ => Err("the selected position is guarded".into()),
    }
}

/// RFC 6901 reference tokens.
fn pointer_segments(pointer: &str) -> Result<Vec<String>, String> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    let Some(rest) = pointer.strip_prefix('/') else {
        return Err(format!("{pointer:?} is not a JSON pointer"));
    };
    Ok(rest
        .split('/')
        .map(|token| token.replace("~1", "/").replace("~0", "~"))
        .collect())
}

/// Decodes one raw scalar the same way YAML 1.1 (Helm) and YAML 1.2
/// (`serde_yaml`) would, or reports the plain spellings where they differ.
fn decode_plain_or_quoted(raw: &str) -> Result<serde_yaml::Value, String> {
    let raw = raw.trim();
    let value = serde_yaml::from_str::<serde_yaml::Value>(raw)
        .map_err(|error| format!("selected scalar is not YAML: {error}"))?;
    if raw.starts_with('"') || raw.starts_with('\'') {
        return Ok(value);
    }
    let ambiguous = match &value {
        serde_yaml::Value::String(text) => yaml_one_one_ambiguous(text),
        serde_yaml::Value::Bool(_) => !matches!(raw, "true" | "false"),
        serde_yaml::Value::Number(_) => {
            let digits = raw.strip_prefix('-').unwrap_or(raw);
            digits.is_empty()
                || !digits.chars().all(|character| character.is_ascii_digit())
                || (digits.len() > 1 && digits.starts_with('0'))
        }
        serde_yaml::Value::Null
        | serde_yaml::Value::Sequence(_)
        | serde_yaml::Value::Mapping(_)
        | serde_yaml::Value::Tagged(_) => true,
    };
    if ambiguous {
        return Err(format!(
            "plain scalar {raw:?} may decode differently in YAML 1.1"
        ));
    }
    Ok(value)
}

/// Runs the adapter on `files` with defaults `{}`.
fn run(files: &[(&str, &str)], query: &Value) -> eyre::Result<ControlResult> {
    let scratch = test_util::scratch::ScratchDir::new("ir-controls")?;
    let mut builder = controls::RegistryBuilder::new(&scratch.path().join("registry"))?;
    let input = builder.render_input(files, "")?;
    let blobs = ControlBlobs::new(scratch.path().join("registry/blobs"));
    let query: Query = serde_json::from_value(query.clone())?;
    Ok(observe(&blobs, &input, &query))
}

fn output_query(pointer: &str) -> Value {
    json!({"kind": "output_v1", "file": "templates/cm.yaml", "document": 0, "pointer": pointer})
}

const CHART: (&str, &str) = (
    "Chart.yaml",
    indoc! {"
        apiVersion: v2
        name: c
        version: 0.1.0
    "},
);

/// Helm 4.2.3 renders the static `ConfigMap` verbatim; `/data/greeting` is
/// the string `hello world`.
#[test]
fn literal_output_control_is_exact() -> eyre::Result<()> {
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: literal
        data:
          greeting: "hello world"
          count: 3
    "#};
    let files = [CHART, ("templates/cm.yaml", template)];
    let have = run(&files, &output_query("/data/greeting"))?;
    sim_assert_eq!(have: have, want: exact(Observation::Output {
        selection: controls::output_value(b"hello world"),
    }));
    let have = run(&files, &output_query("/data/count"))?;
    sim_assert_eq!(have: have, want: exact(Observation::Output {
        selection: Selection::WrongType { value: json!(3) },
    }));
    let have = run(&files, &output_query("/data/absent"))?;
    sim_assert_eq!(have: have, want: exact(Observation::Output {
        selection: Selection::Missing,
    }));
    let have = run(
        &files,
        &json!({"kind": "termination_v1", "scope": "render"}),
    )?;
    sim_assert_eq!(have: have, want: exact(Observation::Termination {
        outcome: Termination::Normal,
    }));
    Ok(())
}

/// Helm 4.2.3 aborts `helm template` in template evaluation with
/// `execution error at (c/templates/fail.yaml:1:3): boom`.
#[test]
fn direct_abort_control_is_exact() -> eyre::Result<()> {
    let files = [
        CHART,
        ("templates/fail.yaml", "{{ fail \"boom\" }}\n"),
        (
            "templates/cm.yaml",
            indoc! {"
                apiVersion: v1
                kind: ConfigMap
                data:
                  a: b
            "},
        ),
    ];
    let have = run(
        &files,
        &json!({"kind": "termination_v1", "scope": "render"}),
    )?;
    sim_assert_eq!(have: have, want: exact(Observation::Termination {
        outcome: Termination::Abort { phase: AbortPhase::TemplateEval },
    }));
    let have = run(&files, &output_query("/data/a"))?;
    sim_assert_eq!(have: have, want: exact(Observation::Output {
        selection: Selection::NotEmitted { phase: AbortPhase::TemplateEval },
    }));
    Ok(())
}

/// A values-guarded fail is not a definite abort of this input: without
/// guard projection the legacy result cannot say, so it reports a gap.
#[test]
fn guarded_abort_is_a_representation_gap() -> eyre::Result<()> {
    let files = [
        CHART,
        (
            "templates/fail.yaml",
            "{{ if .Values.x }}{{ fail \"boom\" }}{{ end }}\n",
        ),
    ];
    let have = run(
        &files,
        &json!({"kind": "termination_v1", "scope": "render"}),
    )?;
    let ControlResult::AbstractUnknown { reason, .. } = have else {
        eyre::bail!("expected abstract_unknown, have {have:?}");
    };
    sim_assert_eq!(have: reason, want: UnknownReason::RepresentationGap);
    Ok(())
}

/// The snapshot chart: Helm 4.2.3 renders `/data/snapshot` as the string
/// `{"x":"old"}`. The legacy fragment folds `toJson` into a structured
/// mapping and cannot prove completion, so the adapter must report a gap,
/// never the structured value as a wrong-type claim.
#[test]
fn snapshot_control_is_an_explicit_representation_gap() -> eyre::Result<()> {
    let template = indoc! {r#"
        {{- $d := dict "x" "old" -}}
        {{- $saved := toJson $d -}}
        {{- $_ := set $d "x" "new" -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: snapshot
        data:
          snapshot: {{ $saved | quote }}
    "#};
    let files = [CHART, ("templates/cm.yaml", template)];
    let have = run(&files, &output_query("/data/snapshot"))?;
    let ControlResult::AbstractUnknown { reason, evidence } = have else {
        eyre::bail!("expected abstract_unknown, have {have:?}");
    };
    sim_assert_eq!(have: reason, want: UnknownReason::RepresentationGap);
    sim_assert_eq!(have: evidence.gaps, want: vec![
        "templates/cm.yaml executes template actions; the legacy fragment does not record completion"
            .to_string(),
    ]);
    Ok(())
}

/// The write-visible alias chart: Helm 4.2.3 renders `/data/visible` as
/// `true`. The mutation/read program executes actions, so the legacy
/// result reports a representation gap.
#[test]
fn alias_control_is_an_explicit_representation_gap() -> eyre::Result<()> {
    let template = indoc! {r#"
        {{- $a := dict "x" "old" -}}
        {{- $b := $a -}}
        {{- $_ := set $a "x" "new" -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: alias
        data:
          visible: {{ eq (get $b "x") "new" }}
    "#};
    let files = [CHART, ("templates/cm.yaml", template)];
    let query = json!({"kind": "alias_v1", "file": "templates/cm.yaml", "document": 0,
        "pointer": "/data/visible", "relation": "write_visible"});
    let have = run(&files, &query)?;
    let ControlResult::AbstractUnknown { reason, .. } = have else {
        eyre::bail!("expected abstract_unknown, have {have:?}");
    };
    sim_assert_eq!(have: reason, want: UnknownReason::RepresentationGap);
    Ok(())
}

/// A static Boolean is read through the alias evaluator exactly.
#[test]
fn static_alias_boolean_is_exact() -> eyre::Result<()> {
    let files = [
        CHART,
        (
            "templates/cm.yaml",
            indoc! {"
                apiVersion: v1
                kind: ConfigMap
                data:
                  visible: true
            "},
        ),
    ];
    let query = json!({"kind": "alias_v1", "file": "templates/cm.yaml", "document": 0,
        "pointer": "/data/visible", "relation": "write_visible"});
    let have = run(&files, &query)?;
    sim_assert_eq!(have: have, want: exact(Observation::Alias {
        selection: Selection::Value { value: true },
    }));
    Ok(())
}

/// Plain scalars YAML 1.1 reads differently are gaps, not guesses.
#[test]
fn yaml_one_one_ambiguous_scalars_are_gaps() -> eyre::Result<()> {
    for scalar in ["yes", "on", "012", "1:30", "True"] {
        let template = format!("apiVersion: v1\nkind: ConfigMap\ndata:\n  v: {scalar}\n");
        let files = [CHART, ("templates/cm.yaml", template.as_str())];
        let have = run(&files, &output_query("/data/v"))?;
        sim_assert_eq!(
            have: matches!(have, ControlResult::AbstractUnknown { .. }),
            want: true,
            "{scalar}: {have:?}"
        );
    }
    Ok(())
}

/// The canonical result bytes are stable: two runs agree byte for byte.
#[test]
fn adapter_answers_are_deterministic() -> eyre::Result<()> {
    let files = [CHART, ("templates/fail.yaml", "{{ fail \"boom\" }}\n")];
    let query = json!({"kind": "termination_v1", "scope": "render"});
    let (first, second) = (run(&files, &query)?, run(&files, &query)?);
    sim_assert_eq!(
        have: canonical_json(&serde_json::to_value(first)?),
        want: canonical_json(&serde_json::to_value(second)?)
    );
    Ok(())
}

/// Fails that Helm never executes are never claimed as aborts.
#[test]
fn unexecuted_fails_are_not_claimed_aborts() -> eyre::Result<()> {
    let sources = [
        "{{ range list }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ range $k, $v := dict }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ with \"\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if false }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if eq 1 2 }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ define \"never\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if .Release.IsUpgrade }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if .Capabilities.APIVersions.Has \"x/v1\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if lookup \"v1\" \"Secret\" \"default\" \"x\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ $x := false }}{{ if $x }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if (empty (list 1)) }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if not (hasKey (dict \"a\" 1) \"a\") }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if ne .Release.Name \"p\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if ne .Chart.Name \"c\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if semverCompare \"<1.0.0\" .Capabilities.KubeVersion.Version }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if .Files.Get \"x\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if (include \"missing.helper\" .) }}{{ fail \"boom\" }}{{ end }}\n",
    ];
    for source in sources {
        let files = [CHART, ("templates/x.yaml", source)];
        let have = run(
            &files,
            &json!({"kind": "termination_v1", "scope": "render"}),
        )?;
        let claimed_abort = have
            == exact(Observation::Termination {
                outcome: Termination::Abort {
                    phase: AbortPhase::TemplateEval,
                },
            });
        sim_assert_eq!(have: claimed_abort, want: false, "{source}");
    }
    Ok(())
}

/// A `.helmignore` selects which files Helm loads, which the adapter does
/// not model, so the chart is a gap even when every template is static.
/// Subcharts and chart defaults are refused before any runner sees them.
#[test]
fn unmodeled_chart_file_selection_is_a_gap() -> eyre::Result<()> {
    let static_map = indoc! {"
        apiVersion: v1
        kind: ConfigMap
        data:
          a: b
    "};
    let files = [
        CHART,
        ("templates/cm.yaml", static_map),
        (".helmignore", "templates/fail.yaml\n"),
    ];
    let have = run(
        &files,
        &json!({"kind": "termination_v1", "scope": "render"}),
    )?;
    let ControlResult::AbstractUnknown { reason, .. } = have else {
        eyre::bail!("expected abstract_unknown, have {have:?}");
    };
    sim_assert_eq!(have: reason, want: UnknownReason::RepresentationGap);
    Ok(())
}

/// Another file Go cannot parse turns the abort into a `template_parse`
/// abort: the adapter must not claim `template_eval`. Helm 4.2.3 reports
/// `parse error at (c/templates/bad.yaml:1): function "nope" not defined`.
#[test]
fn an_unparsable_file_beside_a_fail_is_a_gap() -> eyre::Result<()> {
    let termination = json!({"kind": "termination_v1", "scope": "render"});
    for other in [
        ("templates/bad.yaml", "{{ nope }}\n"),
        ("templates/_helpers.tpl", "{{ end }}\n"),
        (
            "templates/_helpers.tpl",
            "{{/* a comment Go accepts but the adapter does not prove */}}\n",
        ),
    ] {
        let (name, _) = other;
        let files = [
            CHART,
            ("templates/fail.yaml", "{{ fail \"boom\" }}\n"),
            other,
        ];
        let have = run(&files, &termination)?;
        let ControlResult::AbstractUnknown { reason, .. } = have else {
            eyre::bail!("{name}: expected abstract_unknown, have {have:?}");
        };
        sim_assert_eq!(have: reason, want: UnknownReason::RepresentationGap);
    }
    Ok(())
}

/// A normal exit needs every template file, partials included, to be
/// provably parseable and every rendered document's head to decode as
/// Helm 4.2.3's `SimpleHead`, which aborts `list.yaml` with `YAML parse
/// error on c/templates/list.yaml: ... cannot unmarshal array into Go value
/// of type util.SimpleHead` and `meta.yaml` (`metadata: []`) likewise.
#[test]
fn normal_needs_whole_chart_proof() -> eyre::Result<()> {
    let termination = json!({"kind": "termination_v1", "scope": "render"});
    let static_map = indoc! {"
        apiVersion: v1
        kind: ConfigMap
        data:
          a: b
    "};
    for other in [
        (
            "templates/_helpers.tpl",
            "{{ define \"x\" }}{{ nope }}{{ end }}\n",
        ),
        (
            "templates/list.yaml",
            indoc! {"
                - a
                - b
            "},
        ),
        ("templates/scalar.yaml", "just text\n"),
        (
            "templates/meta.yaml",
            indoc! {"
                apiVersion: v1
                kind: ConfigMap
                metadata: []
            "},
        ),
    ] {
        let (name, _) = other;
        let files = [CHART, ("templates/cm.yaml", static_map), other];
        let have = run(&files, &termination)?;
        let ControlResult::AbstractUnknown { reason, .. } = have else {
            eyre::bail!("{name}: expected abstract_unknown, have {have:?}");
        };
        sim_assert_eq!(have: reason, want: UnknownReason::RepresentationGap);
    }
    Ok(())
}

/// Renders with the exact Helm release whose loader the adapter models.
fn render_with_pinned_helm(template: &str) -> eyre::Result<std::process::Output> {
    let helm = || {
        let mut command = std::process::Command::new("mise");
        command.args(["exec", "helm@4.2.3", "--", "helm"]);
        command
    };
    let version = helm()
        .args(["version", "--template", "{{.Version}}"])
        .output()?;
    assert!(version.status.success(), "{version:?}");
    sim_assert_eq!(have: String::from_utf8(version.stdout)?.trim(), want: "v4.2.3");
    let chart = test_util::scratch::ScratchDir::new("ir-controls-helm-parity")?;
    std::fs::create_dir_all(chart.path().join("templates"))?;
    let (name, contents) = CHART;
    std::fs::write(chart.path().join(name), contents)?;
    std::fs::write(chart.path().join("templates/cm.yaml"), template)?;
    Ok(helm()
        .args(["template", "p"])
        .arg(chart.path())
        .args(["--kube-version", "1.29.0"])
        .output()?)
}

/// The adapter must leave Helm's render-YAML failures unknown rather than claim Normal.
fn assert_helm_yaml_gap(template: &str) -> eyre::Result<()> {
    let have = run(
        &[CHART, ("templates/cm.yaml", template)],
        &json!({"kind": "termination_v1", "scope": "render"}),
    )?;
    let ControlResult::AbstractUnknown { reason, .. } = have else {
        eyre::bail!("expected an unknown render exit for {template:?}, have {have:?}");
    };
    sim_assert_eq!(have: reason, want: UnknownReason::RepresentationGap);
    let rendered = render_with_pinned_helm(template)?;
    let stderr = String::from_utf8(rendered.stderr)?;
    sim_assert_eq!(have: rendered.status.code(), want: Some(1), "{template}: {stderr}");
    assert!(stderr.contains("YAML parse error"), "{template}: {stderr}");
    Ok(())
}

/// Go's folded struct lookup can reach typed fields through aliases Rust does not look up.
#[test]
fn case_folded_headers_do_not_prove_normal() -> eyre::Result<()> {
    for alias in [
        "Metadata: []",
        "APIVERSION: []",
        "KIND: []",
        "\u{212a}ind: []",
        indoc! {"
            metadata:
              Name: []
        "},
        indoc! {"
            metadata:
              Annotations: []
        "},
        indoc! {"
            metadata:
              annotationſ: []
        "},
    ] {
        assert_helm_yaml_gap(&formatdoc! {"
            apiVersion: v1
            kind: ConfigMap
            {alias}
        "})?;
    }
    Ok(())
}

/// Non-finite values are outside Helm's JSON domain, including nested mappings and sequences.
#[test]
fn non_finite_yaml_does_not_prove_normal() -> eyre::Result<()> {
    for scalar in [".nan", ".inf", "-.inf"] {
        for body in [
            format!("data: {scalar}\n"),
            formatdoc! {"
                data:
                  nested:
                    - values: [{scalar}]
            "},
        ] {
            assert_helm_yaml_gap(&formatdoc! {"
                apiVersion: v1
                kind: ConfigMap
                {body}
            "})?;
        }
    }
    // Finite numbers and quoted spellings remain valid loader inputs.
    let finite = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          nested: [1.5, 0, -1, ".nan", ".inf"]
    "#};
    sim_assert_eq!(have: run(&[CHART, ("templates/cm.yaml", finite)],
        &json!({"kind": "termination_v1", "scope": "render"}))?,
        want: exact(Observation::Termination { outcome: Termination::Normal }));
    let rendered = render_with_pinned_helm(finite)?;
    assert!(rendered.status.success(), "{rendered:?}");
    Ok(())
}

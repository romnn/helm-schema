//! Independent census of heap events emitted by canonical whole-chart generation.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::ValuesPath;
use helm_schema_test_support::generate::chart_schema;
use helm_schema_test_support::registry::{ArtifactId, ChartId, GenerationRecipe};
use serde::Deserialize;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use tracing::field::{Field, Visit};
use tracing_subscriber::prelude::*;

use crate::analysis_db::IrAnalysisDb;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Empty,
    Registration,
    Install,
    Write,
    WriteSlot,
    UnknownWrite,
    Join,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
enum Install {
    Baseline,
    Mutation,
}

#[derive(Debug, Deserialize)]
struct HeapEvent {
    arena: String,
    version: u64,
    parents: Vec<u64>,
    kind: Kind,
    object: Option<u64>,
    key: Option<String>,
    install: Option<Install>,
    objects: Option<Vec<u64>>,
    paths: Vec<(u64, ValuesPath)>,
    writes: BTreeSet<ValuesPath>,
    registration_matches: Option<bool>,
    changes_inputs: bool,
}

#[derive(Deserialize)]
struct CertificateEvent {
    source: String,
    local_templates: BTreeSet<String>,
    required: bool,
}

#[derive(Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Observation {
    Heap(HeapEvent),
    Certificate(CertificateEvent),
}

type Paths = BTreeMap<u64, Vec<ValuesPath>>;

struct Record {
    event: HeapEvent,
    paths: Arc<Paths>,
    expected: BTreeSet<ValuesPath>,
    ancestry: Ancestry,
}

#[derive(Default)]
struct Ancestry {
    parent: Option<u64>,
    depth: usize,
    inputs: bool,
}

#[derive(Default)]
struct History {
    records: BTreeMap<u64, Record>,
}

impl History {
    fn record(&mut self, event: HeapEvent) -> eyre::Result<bool> {
        eyre::ensure!(
            !self.records.contains_key(&event.version),
            "duplicate heap observation"
        );
        let mut paths = event.parents.first().map_or_else(
            || Ok(Arc::new(Paths::new())),
            |parent| self.get(*parent).map(|parent| Arc::clone(&parent.paths)),
        )?;
        for (object, path) in &event.paths {
            let paths = Arc::make_mut(&mut paths).entry(*object).or_default();
            if !paths.contains(path) {
                paths.push(path.clone());
            }
        }
        let mut rows = Vec::new();
        for parent in &event.parents {
            rows.extend(self.rows_at(*parent, &paths)?);
        }
        let object = match event.kind {
            Kind::Install => (event.install.ok_or_eyre("install lacks kind")? == Install::Mutation)
                .then_some(event.object.ok_or_eyre("install lacks object")?),
            Kind::Write | Kind::WriteSlot => Some(event.object.ok_or_eyre("write lacks object")?),
            Kind::Empty | Kind::Registration | Kind::Join | Kind::UnknownWrite => None,
        };
        if let Some(object) = object {
            for original in paths.get(&object).into_iter().flatten() {
                let mut path = original.clone();
                if let Some(key) = &event.key {
                    path.push(key);
                }
                rows.push(path);
            }
        }
        if event.kind == Kind::UnknownWrite {
            match &event.objects {
                None => rows.push(ValuesPath::default()),
                Some(objects) => {
                    for object in objects {
                        rows.extend(paths.get(object).into_iter().flatten().cloned());
                    }
                }
            }
        }
        let expected = normalized(rows);
        let ancestry = self.ancestry(&event, &paths)?;
        let matches = expected == event.writes
            && event.registration_matches != Some(false)
            && ancestry.inputs == event.changes_inputs;
        if !matches {
            eprintln!(
                "stage0 mismatch version={} kind={:?} indexed={:?} expected={expected:?}",
                event.version, event.kind, event.writes
            );
        }
        self.records.insert(
            event.version,
            Record {
                event,
                paths,
                expected,
                ancestry,
            },
        );
        Ok(matches)
    }

    fn ancestry(&self, event: &HeapEvent, paths: &Paths) -> eyre::Result<Ancestry> {
        if event.kind == Kind::Empty {
            return Ok(Ancestry::default());
        }
        if event.kind != Kind::Join {
            let parent = event
                .parents
                .first()
                .copied()
                .ok_or_eyre("event lacks parent")?;
            let inputs = match event.kind {
                Kind::Empty | Kind::Registration | Kind::Join => false,
                Kind::UnknownWrite => true,
                Kind::Install => {
                    event.install != Some(Install::Baseline)
                        && event
                            .object
                            .is_some_and(|object| paths.contains_key(&object))
                }
                Kind::Write | Kind::WriteSlot => event
                    .object
                    .is_some_and(|object| paths.contains_key(&object)),
            };
            return Ok(Ancestry {
                parent: Some(parent),
                depth: self.get(parent)?.ancestry.depth + 1,
                inputs,
            });
        }
        let &[mut left, mut right] = event.parents.as_slice() else {
            eyre::bail!("join lacks two parents");
        };
        let mut inputs = false;
        while left != right {
            let left_record = &self.get(left)?.ancestry;
            let right_record = &self.get(right)?.ancestry;
            if left_record.depth >= right_record.depth {
                inputs |= left_record.inputs;
                let Some(parent) = left_record.parent else {
                    return Ok(Ancestry {
                        inputs: true,
                        ..Ancestry::default()
                    });
                };
                left = parent;
            } else {
                inputs |= right_record.inputs;
                let Some(parent) = right_record.parent else {
                    return Ok(Ancestry {
                        inputs: true,
                        ..Ancestry::default()
                    });
                };
                right = parent;
            }
        }
        Ok(Ancestry {
            parent: Some(left),
            depth: self.get(left)?.ancestry.depth + 1,
            inputs,
        })
    }

    fn get(&self, version: u64) -> eyre::Result<&Record> {
        self.records
            .get(&version)
            .ok_or_eyre("missing parent heap observation")
    }

    fn rows_at(&self, version: u64, paths: &Paths) -> eyre::Result<Vec<ValuesPath>> {
        let parent = self.get(version)?;
        let mut rows: Vec<_> = parent.expected.iter().cloned().collect();
        if rows.iter().any(|path| path.segments().next().is_none()) {
            return Ok(rows);
        }
        for (object, current) in paths {
            let previous = parent.paths.get(object).map_or(&[][..], Vec::as_slice);
            for new in current.iter().filter(|path| !previous.contains(path)) {
                if previous.is_empty() {
                    if self.prior_mutation(version, *object)? {
                        rows.push(new.clone());
                    }
                    continue;
                }
                for old in previous {
                    let old_segments: Vec<_> = old.segments().collect();
                    for written in &parent.expected {
                        let written_segments: Vec<_> = written.segments().collect();
                        if old_segments.starts_with(&written_segments) {
                            rows.push(new.clone());
                        } else if written_segments.starts_with(&old_segments) {
                            rows.push(ValuesPath::from_segments(
                                new.segments()
                                    .chain(written.segments().skip(old_segments.len())),
                            ));
                        }
                    }
                }
            }
        }
        Ok(rows)
    }

    fn prior_mutation(&self, version: u64, object: u64) -> eyre::Result<bool> {
        let mut pending = vec![version];
        let mut seen = BTreeSet::new();
        while let Some(version) = pending.pop() {
            if !seen.insert(version) {
                continue;
            }
            let event = &self.get(version)?.event;
            match event.kind {
                Kind::Write | Kind::WriteSlot if event.object == Some(object) => return Ok(true),
                Kind::Install if event.object == Some(object) => {
                    if event.install == Some(Install::Mutation) {
                        return Ok(true);
                    }
                    let parent = event
                        .parents
                        .first()
                        .copied()
                        .ok_or_eyre("install lacks parent")?;
                    if !self.has_allocation(parent, object)? {
                        continue;
                    }
                }
                Kind::UnknownWrite => {
                    if event
                        .objects
                        .as_ref()
                        .is_none_or(|objects| objects.contains(&object))
                    {
                        return Ok(true);
                    }
                }
                Kind::Empty
                | Kind::Registration
                | Kind::Join
                | Kind::Write
                | Kind::WriteSlot
                | Kind::Install => {}
            }
            pending.extend(&event.parents);
        }
        Ok(false)
    }

    fn has_allocation(&self, version: u64, object: u64) -> eyre::Result<bool> {
        let mut pending = vec![version];
        let mut seen = BTreeSet::new();
        while let Some(version) = pending.pop() {
            if !seen.insert(version) {
                continue;
            }
            let event = &self.get(version)?.event;
            if event.kind == Kind::Install && event.object == Some(object) {
                return Ok(true);
            }
            pending.extend(&event.parents);
        }
        Ok(false)
    }
}

// Normalize independent event rows without invoking the production index's updates.
fn normalized(rows: Vec<ValuesPath>) -> BTreeSet<ValuesPath> {
    let mut result = BTreeSet::<ValuesPath>::new();
    for row in rows {
        let row =
            ValuesPath::from_segments(row.segments().take_while(|part| part.literal().is_some()));
        let row_segments: Vec<_> = row.segments().collect();
        if result
            .iter()
            .any(|prefix| row_segments.starts_with(&prefix.segments().collect::<Vec<_>>()))
        {
            continue;
        }
        result.retain(|child| {
            !child
                .segments()
                .collect::<Vec<_>>()
                .starts_with(&row_segments)
        });
        result.insert(row);
        if result.len() > 64 {
            return BTreeSet::from([ValuesPath::default()]);
        }
    }
    result
}

#[derive(Default)]
struct Census {
    histories: BTreeMap<String, History>,
    certificates: BTreeMap<CertificateKey, bool>,
    versions: usize,
    index_mismatches: usize,
    sessions: usize,
    comparisons: usize,
    required: usize,
    read_only: usize,
    certificate_mismatches: usize,
    errors: Vec<String>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct CertificateKey {
    source: String,
    local_templates: BTreeSet<String>,
}

impl Census {
    fn observe(&mut self, observation: Observation) -> eyre::Result<()> {
        match observation {
            Observation::Heap(event) => {
                if event.version == 0 {
                    self.histories
                        .insert(event.arena.clone(), History::default());
                    self.sessions += 1;
                }
                let history = self.histories.entry(event.arena.clone()).or_default();
                self.index_mismatches += usize::from(!history.record(event)?);
                self.versions += 1;
            }
            Observation::Certificate(CertificateEvent {
                source,
                local_templates,
                required,
            }) => {
                let key = CertificateKey {
                    source,
                    local_templates,
                };
                let may_write = *self.certificates.entry(key).or_insert_with_key(|key| {
                    let db = IrAnalysisDb::new(&DefineIndex::new());
                    let parsed = db.parsed_source(&key.source, None, "stage0-certificate");
                    parsed.root.syntax().is_none_or(|syntax| {
                        super::write_certificate::may_write(
                            &key.source,
                            syntax,
                            &key.local_templates,
                        )
                    })
                });
                self.comparisons += 1;
                self.required += usize::from(required);
                self.read_only += usize::from(!may_write);
                self.certificate_mismatches += usize::from(required && !may_write);
            }
        }
        Ok(())
    }

    fn report(&self, chart: &str) -> Value {
        json!({"chart": chart, "sessions": self.sessions,
            "index_versions": self.versions, "index_mismatches": self.index_mismatches,
            "certificate_comparisons": self.comparisons, "certificate_required": self.required,
            "certificate_read_only": self.read_only,
            "certificate_mismatches": self.certificate_mismatches, "errors": self.errors})
    }
}

#[derive(Clone)]
struct CensusLayer(Arc<Mutex<Census>>);

#[derive(Default)]
struct Payload {
    json: String,
}

impl Visit for Payload {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "payload" {
            self.json = format!("{value:?}");
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "payload" {
            self.json = value.to_owned();
        }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CensusLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if event.metadata().target() != "helm_schema_ir::stage0" {
            return;
        }
        let Self(census) = self;
        let mut payload = Payload::default();
        event.record(&mut payload);
        let mut census = census.lock().unwrap_or_else(PoisonError::into_inner);
        let observation = serde_json::from_str(&payload.json).map_err(eyre::Report::from);
        if let Err(err) = observation.and_then(|observation| census.observe(observation)) {
            census.errors.push(format!("{err:#}"));
        }
    }
}

#[test]
#[ignore = "canonical eight-chart Stage 0 census; run one chart per process when memory is bounded"]
fn stage0_chart_census() -> eyre::Result<()> {
    let census = Arc::new(Mutex::new(Census::default()));
    let subscriber = tracing_subscriber::registry().with(
        CensusLayer(Arc::clone(&census)).with_filter(tracing_subscriber::filter::filter_fn(
            |metadata| metadata.target() == "helm_schema_ir::stage0",
        )),
    );
    tracing::subscriber::set_global_default(subscriber)?;
    let selected = std::env::var("STAGE0_CENSUS_CHART").ok();
    let mut reports = Vec::new();
    for chart in [
        ChartId::Airflow,
        ChartId::ArgoCd,
        ChartId::BitnamiRedis,
        ChartId::Cilium,
        ChartId::Datadog,
        ChartId::KubePrometheusStack,
        ChartId::PostgresqlHa,
        ChartId::Vault,
    ] {
        let GenerationRecipe::Chart(recipe) = ArtifactId::Chart(chart).spec().recipe else {
            eyre::bail!("census chart lacks a canonical whole-chart recipe");
        };
        if selected
            .as_ref()
            .is_some_and(|selected| selected != recipe.chart)
        {
            continue;
        }
        *census.lock().unwrap_or_else(PoisonError::into_inner) = Census::default();
        chart_schema(&recipe).wrap_err_with(|| format!("generate {}", recipe.chart))?;
        let census = census.lock().unwrap_or_else(PoisonError::into_inner);
        let report = census.report(recipe.chart);
        eprintln!("stage0 {report}");
        reports.push(report);
        if let Some(output) = std::env::var_os("STAGE0_CENSUS_OUT") {
            std::fs::write(
                output,
                serde_json::to_vec_pretty(&json!({"charts": &reports}))?,
            )?;
        }
        eyre::ensure!(
            census.sessions > 0 && census.versions > 0,
            "census observed no heaps"
        );
        eyre::ensure!(
            census.comparisons > 0,
            "census observed no certificate comparisons"
        );
        sim_assert_eq!(have: census.index_mismatches, want: 0);
        sim_assert_eq!(have: census.certificate_mismatches, want: 0);
        sim_assert_eq!(have: &census.errors, want: &Vec::<String>::new());
    }
    eyre::ensure!(!reports.is_empty(), "no canonical census chart selected");
    if let Some(output) = std::env::var_os("STAGE0_CENSUS_OUT") {
        std::fs::write(
            output,
            serde_json::to_vec_pretty(&json!({"charts": reports}))?,
        )?;
    }
    Ok(())
}

#[test]
fn index_equals_history_recomputation_for_writes_havocs_and_joins() -> eyre::Result<()> {
    use std::rc::Rc;

    use helm_schema_core::Predicate;

    use crate::value::{Coverage, Field, Multiplicity, ObjectData, Remainder, State};

    let census = Arc::new(Mutex::new(Census::default()));
    let subscriber = tracing_subscriber::registry().with(
        CensusLayer(Arc::clone(&census)).with_filter(tracing_subscriber::filter::filter_fn(
            |metadata| metadata.target() == "helm_schema_ir::stage0",
        )),
    );
    tracing::subscriber::set_global_default(subscriber)?;
    let mut state = State::new(crate::value::Value::nil());
    let object = state.heap.allocate(
        ObjectData::Map {
            fields: Rc::default(),
            remainder: Remainder::Input,
        },
        Multiplicity::Singleton,
    );
    state
        .heap
        .register_input(object.clone(), ValuesPath::parse("config"));
    assert!(
        state
            .heap
            .write(&object, "first", Field::Present(crate::value::Value::nil()))
    );
    let mut other = state.heap.fork();
    other.register_input(object.clone(), ValuesPath::parse("alias"));
    assert!(other.write(
        &object,
        "second",
        Field::Present(crate::value::Value::nil())
    ));
    let original = state.heap.fork();
    let predicate = Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    state.heap.join(&decision, &original, &other)?;
    state.heap.unknown_write(Rc::default());
    let census = census.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(census.versions > 5);
    sim_assert_eq!(have: census.index_mismatches, want: 0);
    sim_assert_eq!(have: &census.errors, want: &Vec::<String>::new());
    Ok(())
}

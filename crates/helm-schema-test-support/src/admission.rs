//! The W1 admission suite under `testdata/w1-admission`: `cell_matrix` cells that record the
//! pinned Helm 4.2.3 class and main's verdict, and `obligations.tsv`, the cells main gets wrong.
//!
//! The admission gate (`crates/helm-schema-cli/tests/w1_admission.rs`) regenerates every cell's
//! schema with the code under test and [`classify`]s the verdict against the recorded one. The
//! suite records the current main exactly: a cell that moves away from Helm is a new failure, and a
//! cell that moves toward Helm must be re-recorded ([`refresh`]) before the gate passes again.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use serde_json::Value;
use test_util::helm_values::{ValuesError, ValuesOptions, coalesce_chart_values};

use crate::cell_matrix::generation_recipe;
use crate::helm::adjudication::PinnedHelmChart;
use crate::helm::invocation::HelmRunner;
use crate::registry::BUNDLE_K8S_VERSION;
use crate::source_digest::sha256_hex;

/// The suite's directory under the workspace `testdata/`.
pub const SUITE_DIR: &str = "w1-admission";

/// The cells files and the declared-types mode each was measured under.
pub const CELL_FILES: [(&str, DeclaredTypes); 2] = [
    ("cells.tsv", DeclaredTypes::Annotate),
    ("cells-assert.tsv", DeclaredTypes::Assert),
];

/// The frozen roster of cell ids and modes.
pub const ROSTER: &str = "roster.tsv";
const ROSTER_HEADER: &str = "cell_id\tdeclared_types";
const COALESCED_TABLE: &str = "coalesced.tsv";
const COALESCED_HEADER: &str = "cell_id\tinputs_sha256\tdocument_sha256";
/// Snapshots of Helm's coalesced document, for the cells the Rust values port does not reproduce.
const COALESCED_DIR: &str = "coalesced";
const CELLS_HEADER: &str = "cell_id\tchart\tvalues\tkube_version\texpect";
const OBLIGATIONS_HEADER: &str = "cell_id\tlane\tdeclared_types\thelm_class\tmain\terror\towner";
/// The owner a newly recorded obligation gets; ADR 0042 policy rows are classified by hand.
const DEFAULT_OWNER: &str = "R1-R3";
const OWNERS: [&str; 2] = [DEFAULT_OWNER, "ADR-0042"];

/// `--declared-types` of a cells file's generations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeclaredTypes {
    /// A declared default documents a value.
    Annotate,
    /// A declared default asserts its type.
    Assert,
}

impl DeclaredTypes {
    /// The CLI spelling.
    #[must_use]
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Annotate => "annotate",
            Self::Assert => "assert",
        }
    }
}

/// Pinned Helm's class of a cell's render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelmClass {
    /// Helm renders the cell.
    Pass,
    /// Helm aborts.
    Reject,
}

/// A schema's verdict on a cell's coalesced values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The schema accepts the values.
    Accept,
    /// The schema rejects the values.
    Reject,
}

impl Verdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Reject => "reject",
        }
    }

    /// Whether this verdict is the one Helm's class calls for.
    #[must_use]
    pub fn agrees_with(self, helm: HelmClass) -> bool {
        matches!(
            (self, helm),
            (Self::Accept, HelmClass::Pass) | (Self::Reject, HelmClass::Reject)
        )
    }
}

impl HelmClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Reject => "reject",
        }
    }
}

/// One recorded cell.
#[derive(Clone, Debug)]
pub struct Cell {
    /// `<lane>.<name>.<case>`.
    pub id: String,
    /// The chart directory, relative to the suite.
    pub chart: String,
    /// The values file bytes Helm and the schema read.
    pub values: Vec<u8>,
    /// The Kubernetes version Helm renders the cell at.
    pub kube_version: String,
    /// The mode its file was measured under.
    pub declared_types: DeclaredTypes,
    /// Pinned Helm's class.
    pub helm: HelmClass,
    /// Main's recorded verdict.
    pub main: Verdict,
}

impl Cell {
    /// The lane the cell came from.
    #[must_use]
    pub fn lane(&self) -> &str {
        self.id.split('.').next().unwrap_or_default()
    }

    /// Whether main gets the cell wrong, which makes it an obligation.
    #[must_use]
    pub fn is_obligation(&self) -> bool {
        !self.main.agrees_with(self.helm)
    }

    fn obligation_row(&self, owner: &str) -> String {
        let error = match self.main {
            Verdict::Accept => "false-accept",
            Verdict::Reject => "false-reject",
        };
        [
            self.id.as_str(),
            self.lane(),
            self.declared_types.as_arg(),
            self.helm.as_str(),
            self.main.as_str(),
            error,
            owner,
        ]
        .join("\t")
    }
}

/// The suite as committed.
#[derive(Debug)]
pub struct Suite {
    /// The suite directory.
    pub root: PathBuf,
    /// Every cell of both cells files.
    pub cells: Vec<Cell>,
    /// `obligations.tsv` rows by cell id, as written.
    pub obligations: BTreeMap<String, String>,
    /// `roster.tsv`: the frozen cell ids with their modes, edited only by hand.
    pub roster: Vec<(String, DeclaredTypes)>,
    /// `coalesced.tsv`: each cell's inputs hash and the hash of the document pinned Helm
    /// coalesced from them.
    pub coalesced: BTreeMap<String, CoalescedRecord>,
}

/// One `coalesced.tsv` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoalescedRecord {
    /// [`inputs_sha256`] when Helm's document was recorded.
    pub inputs_sha256: String,
    /// [`document_sha256`] of Helm's coalesced document.
    pub document_sha256: String,
}

/// How a cell's verdict under the code under test compares with the recorded one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// The recorded verdict holds.
    Kept,
    /// The verdict moved away from Helm: a new failure.
    New,
    /// The verdict moved toward Helm: the obligation closed and must be re-recorded.
    Closed,
}

/// Compares `observed` with the cell's recorded verdict.
#[must_use]
pub fn classify(cell: &Cell, observed: Verdict) -> Admission {
    if observed == cell.main {
        Admission::Kept
    } else if observed.agrees_with(cell.helm) {
        Admission::Closed
    } else {
        Admission::New
    }
}

/// The generation arguments of a cell in `mode`, exactly as `main=` was measured: `cell_matrix`'s
/// recipe at the bundled Kubernetes version under the `full` profile, with the mode pinned.
#[must_use]
pub fn generation_arguments(mode: DeclaredTypes) -> Vec<String> {
    generation_recipe(BUNDLE_K8S_VERSION, "full", Some(mode.as_arg()))
}

/// Reads both cells files and `obligations.tsv` under `root`.
///
/// # Errors
///
/// Returns an error when a file is missing or a row is malformed.
pub fn read_suite(root: &Path) -> eyre::Result<Suite> {
    let mut cells = Vec::new();
    for (name, declared_types) in CELL_FILES {
        let path = root.join(name);
        let text =
            fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?;
        let mut lines = text.lines();
        eyre::ensure!(
            lines.next() == Some(CELLS_HEADER),
            "{name}: unexpected header"
        );
        for (number, line) in lines.enumerate() {
            let row = number + 2;
            cells.push(
                parse_cell(root, line, declared_types).wrap_err_with(|| format!("{name}:{row}"))?,
            );
        }
    }
    let path = root.join("obligations.tsv");
    let text = fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?;
    let mut lines = text.lines();
    eyre::ensure!(
        lines.next() == Some(OBLIGATIONS_HEADER),
        "obligations.tsv: unexpected header"
    );
    let mut obligations = BTreeMap::new();
    for line in lines {
        let id = line.split('\t').next().unwrap_or_default().to_string();
        eyre::ensure!(
            obligations.insert(id.clone(), line.to_string()).is_none(),
            "obligations.tsv: duplicate {id}"
        );
    }
    let roster = read_roster(root)?;
    let mut coalesced = BTreeMap::new();
    if let Ok(text) = fs::read_to_string(root.join(COALESCED_TABLE)) {
        let mut lines = text.lines();
        eyre::ensure!(
            lines.next() == Some(COALESCED_HEADER),
            "coalesced.tsv: unexpected header"
        );
        for line in lines {
            let [id, inputs, document] = line.split('\t').collect::<Vec<_>>()[..] else {
                eyre::bail!("coalesced.tsv: malformed row {line:?}");
            };
            coalesced.insert(
                id.to_string(),
                CoalescedRecord {
                    inputs_sha256: inputs.to_string(),
                    document_sha256: document.to_string(),
                },
            );
        }
    }
    Ok(Suite {
        root: root.to_path_buf(),
        cells,
        obligations,
        roster,
        coalesced,
    })
}

fn parse_cell(root: &Path, line: &str, declared_types: DeclaredTypes) -> eyre::Result<Cell> {
    let fields: Vec<&str> = line.split('\t').collect();
    let [id, chart, values, kube_version, expect] = fields.as_slice() else {
        eyre::bail!("needs five tab-separated fields");
    };
    eyre::ensure!(
        root.join(chart).join("Chart.yaml").is_file(),
        "{id}: no chart at {chart}"
    );
    let values = match values.strip_prefix('@') {
        Some(file) => {
            fs::read(root.join(file)).wrap_err_with(|| format!("{id}: values file {file}"))?
        }
        None => values.as_bytes().to_vec(),
    };
    let (helm, main) = parse_expect(expect)
        .wrap_err_with(|| id.to_string())?
        .ok_or_else(|| eyre::eyre!("{id}: not measured yet (`-`); run admission_refresh"))?;
    Ok(Cell {
        id: (*id).to_string(),
        chart: (*chart).to_string(),
        values,
        kube_version: (*kube_version).to_string(),
        declared_types,
        helm,
        main,
    })
}

/// A cell's recorded expectations, or `None` for a new cell not measured yet (`-`).
fn parse_expect(expect: &str) -> eyre::Result<Option<(HelmClass, Verdict)>> {
    if expect == "-" {
        return Ok(None);
    }
    let mut helm = None;
    let mut main = None;
    for token in expect.split(' ') {
        match token {
            "helm_class=pass" => helm = Some(HelmClass::Pass),
            "helm_class=reject" => helm = Some(HelmClass::Reject),
            "main=accept" => main = Some(Verdict::Accept),
            "main=reject" => main = Some(Verdict::Reject),
            other => eyre::bail!("unexpected expectation {other:?}"),
        }
    }
    Ok(Some((
        helm.ok_or_eyre("no helm_class expectation")?,
        main.ok_or_eyre("no main expectation")?,
    )))
}

impl Suite {
    /// Every disagreement between `obligations.tsv` and the cells: its ids must be exactly the
    /// cells main gets wrong, each row must restate its cell, and each owner must be known.
    #[must_use]
    pub fn obligation_mismatches(&self) -> Vec<String> {
        let mut mismatches = Vec::new();
        let mut ids = BTreeSet::new();
        for cell in &self.cells {
            if !ids.insert(cell.id.as_str()) {
                mismatches.push(format!("{}: duplicate cell id", cell.id));
            }
            match (cell.is_obligation(), self.obligations.get(&cell.id)) {
                (false, None) => {}
                (false, Some(_)) => mismatches.push(format!(
                    "{}: listed in obligations.tsv, but main agrees with Helm",
                    cell.id
                )),
                (true, None) => mismatches.push(format!(
                    "{}: main disagrees with Helm, but obligations.tsv omits it",
                    cell.id
                )),
                (true, Some(row)) => {
                    let owner = row.rsplit('\t').next().unwrap_or_default();
                    if !OWNERS.contains(&owner) || *row != cell.obligation_row(owner) {
                        mismatches.push(format!(
                            "{}: obligations.tsv row {row:?} does not restate the cell",
                            cell.id
                        ));
                    }
                }
            }
        }
        for id in self.obligations.keys() {
            if !ids.contains(id.as_str()) {
                mismatches.push(format!("{id}: obligations.tsv names no cell"));
            }
        }
        mismatches
    }

    /// Every difference between the cells files and the frozen `roster.tsv`: a cell may be added
    /// or removed only by editing the roster, and an empty suite is refused.
    #[must_use]
    pub fn roster_mismatches(&self) -> Vec<String> {
        let mut mismatches = Vec::new();
        if self.roster.is_empty() || self.cells.is_empty() {
            mismatches.push("the suite holds no cells".to_string());
        }
        for (name, mode) in CELL_FILES {
            if !self.cells.iter().any(|cell| cell.declared_types == mode) {
                mismatches.push(format!("{name} holds no cells"));
            }
        }
        let roster: BTreeSet<(&str, DeclaredTypes)> = self
            .roster
            .iter()
            .map(|(id, mode)| (id.as_str(), *mode))
            .collect();
        if roster.len() != self.roster.len() {
            mismatches.push("roster.tsv lists a cell twice".to_string());
        }
        let cells: BTreeSet<(&str, DeclaredTypes)> = self
            .cells
            .iter()
            .map(|cell| (cell.id.as_str(), cell.declared_types))
            .collect();
        for (id, mode) in roster.difference(&cells) {
            mismatches.push(format!(
                "{id} ({}): in roster.tsv, missing from the cells files",
                mode.as_arg()
            ));
        }
        for (id, mode) in cells.difference(&roster) {
            mismatches.push(format!(
                "{id} ({}): in the cells files, missing from roster.tsv",
                mode.as_arg()
            ));
        }
        mismatches
    }

    /// Every difference between `coalesced.tsv` (and its snapshots) and the cells.
    ///
    /// # Errors
    ///
    /// Returns an error when the snapshot directory cannot be read.
    pub fn coalesced_mismatches(&self) -> eyre::Result<Vec<String>> {
        let ids: BTreeSet<&str> = self.cells.iter().map(|cell| cell.id.as_str()).collect();
        let mut mismatches = Vec::new();
        for id in &ids {
            if !self.coalesced.contains_key(*id) {
                mismatches.push(format!("{id}: no coalesced.tsv row"));
            }
        }
        for id in self.coalesced.keys() {
            if !ids.contains(id.as_str()) {
                mismatches.push(format!("{id}: coalesced.tsv names no cell"));
            }
        }
        let dir = self.root.join(COALESCED_DIR);
        if dir.is_dir() {
            for entry in fs::read_dir(&dir)? {
                let name = entry?.file_name().to_string_lossy().to_string();
                let id = name.strip_suffix(".json").unwrap_or(&name);
                if !ids.contains(id) {
                    mismatches.push(format!("coalesced/{name}: names no cell"));
                }
            }
        }
        Ok(mismatches)
    }
}

/// The sha256 of a cell's inputs: its values bytes and every file of its chart.
///
/// # Errors
///
/// Returns an error when the chart cannot be read.
pub fn inputs_sha256(root: &Path, cell: &Cell) -> eyre::Result<String> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<u8>) -> eyre::Result<()> {
        let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<_, _>>()?;
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, base, out)?;
            } else {
                let bytes = fs::read(&path)?;
                let relative = path
                    .strip_prefix(base)?
                    .to_string_lossy()
                    .replace('\\', "/");
                out.extend(format!("file {relative} {}\n", bytes.len()).as_bytes());
                out.extend(bytes);
            }
        }
        Ok(())
    }
    let mut bytes = format!("values {}\n", cell.values.len()).into_bytes();
    bytes.extend(&cell.values);
    let chart = root.join(&cell.chart);
    walk(&chart, &chart, &mut bytes)?;
    Ok(sha256_hex(&bytes))
}

/// The sha256 of a values document's JSON (object keys sorted).
///
/// # Errors
///
/// Returns an error when the document cannot be serialized.
pub fn document_sha256(document: &Value) -> eyre::Result<String> {
    Ok(sha256_hex(&serde_json::to_vec(document)?))
}

/// The values document a cell's schema validates, checked against the document pinned Helm
/// coalesced from the same inputs (`coalesced.tsv`): the Rust port's composition must hash to
/// it, and where the port does not reproduce it, the recorded snapshot must.
///
/// # Errors
///
/// Returns an error when the inputs changed since Helm's document was recorded, or neither the
/// port nor a snapshot yields Helm's document.
pub fn compose(suite: &Suite, cell: &Cell, scratch: &Path) -> eyre::Result<Value> {
    let record = suite
        .coalesced
        .get(&cell.id)
        .ok_or_else(|| eyre::eyre!("no coalesced.tsv row; run `admission_refresh --coalesced`"))?;
    eyre::ensure!(
        inputs_sha256(&suite.root, cell)? == record.inputs_sha256,
        "the chart or values changed since Helm's document was recorded; run \
         `admission_refresh --coalesced`"
    );
    let snapshot = suite
        .root
        .join(COALESCED_DIR)
        .join(format!("{}.json", cell.id));
    if snapshot.is_file() {
        let document: Value = serde_json::from_slice(&fs::read(&snapshot)?)?;
        eyre::ensure!(
            document_sha256(&document)? == record.document_sha256,
            "coalesced/{}.json is not the document Helm coalesced",
            cell.id
        );
        return Ok(document);
    }
    let document = port_composed(&suite.root, cell, scratch)?
        .map_err(|reason| eyre::eyre!("the Rust values port refuses the cell: {reason}"))?;
    eyre::ensure!(
        document_sha256(&document)? == record.document_sha256,
        "the Rust values port composes another document than Helm recorded; run \
         `admission_refresh --coalesced` to snapshot Helm's"
    );
    Ok(document)
}

fn port_composed(root: &Path, cell: &Cell, scratch: &Path) -> eyre::Result<Result<Value, String>> {
    let file = scratch.join(format!("{}.values", cell.id));
    fs::write(&file, &cell.values)?;
    let options = ValuesOptions {
        value_files: vec![file],
        ..ValuesOptions::default()
    };
    match options
        .merge_values()
        .and_then(|overrides| coalesce_chart_values(&root.join(&cell.chart), overrides))
    {
        Ok(instance) => Ok(Ok(instance)),
        Err(ValuesError::NotValidated(reason) | ValuesError::Unmodelled(reason)) => Ok(Err(reason)),
        Err(error) => Err(error.into()),
    }
}

/// Re-records `coalesced.tsv` from the documents `helm` coalesces (as `cell_matrix` reads them),
/// and a snapshot for every cell whose Rust-port composition differs or is refused. Returns the
/// snapshotted cell ids.
///
/// # Errors
///
/// Returns an error when the cells differ from the roster, or Helm fails or coalesces nothing.
pub fn record_helm_coalesced(
    root: &Path,
    helm: &Path,
    scratch: &Path,
) -> eyre::Result<Vec<String>> {
    let suite = read_suite(root)?;
    let roster = suite.roster_mismatches();
    eyre::ensure!(
        roster.is_empty(),
        "the cells differ from roster.tsv:\n{}",
        roster.join("\n")
    );
    let runner: &'static HelmRunner = Box::leak(Box::new(HelmRunner::with_program_within(
        &scratch.join("helm"),
        false,
        helm.to_path_buf(),
        Duration::from_secs(300),
    )?));
    let mut charts = BTreeMap::new();
    let mut table = format!("{COALESCED_HEADER}\n");
    let mut snapshots = Vec::new();
    let mut ids: Vec<&Cell> = suite.cells.iter().collect();
    ids.sort_by(|a, b| a.id.cmp(&b.id));
    for cell in ids {
        if !charts.contains_key(&cell.chart) {
            let chart =
                PinnedHelmChart::prepare_for_explicit_versions(runner, &root.join(&cell.chart))?;
            charts.insert(cell.chart.clone(), chart);
        }
        let chart = charts.get(&cell.chart).ok_or_eyre("unprepared chart")?;
        let probe = chart.adjudicate_file(&cell.values, &cell.kube_version)?;
        let document = probe
            .values
            .ok_or_else(|| eyre::eyre!("{}: Helm coalesces no document", cell.id))?
            .as_json()
            .clone();
        let document_hash = document_sha256(&document)?;
        let port = port_composed(root, cell, scratch)?;
        let reproduced = match &port {
            Ok(composed) => document_sha256(composed)? == document_hash,
            Err(_) => false,
        };
        if !reproduced {
            snapshots.push((cell.id.clone(), document));
        }
        writeln!(
            table,
            "{}\t{}\t{document_hash}",
            cell.id,
            inputs_sha256(root, cell)?
        )?;
    }
    let dir = root.join(COALESCED_DIR);
    if dir.is_dir() {
        fs::remove_dir_all(&dir)?;
    }
    let mut recorded = Vec::new();
    for (id, document) in snapshots {
        fs::create_dir_all(&dir)?;
        let mut bytes = serde_json::to_vec_pretty(&document)?;
        bytes.push(b'\n');
        fs::write(dir.join(format!("{id}.json")), bytes)?;
        recorded.push(id);
    }
    fs::write(root.join(COALESCED_TABLE), table)?;
    Ok(recorded)
}

/// Re-records the suite under `root` from `cell_matrix` tables (`matrix.tsv`) that measured every
/// cell with a binary labelled `main`: each cell's `helm_class=` and `main=` expectation, and
/// `obligations.tsv`, whose existing owners are kept.
///
/// Only closures re-record an existing cell: a verdict moving away from Helm on a cell main got
/// right, or a changed Helm class, is refused. A new cell (expectation `-`, listed in
/// `roster.tsv`) takes whatever was measured.
///
/// # Errors
///
/// Returns an error, writing nothing, when a table misses a cell or holds a class other than pass
/// or reject, the cells differ from the roster, a re-recording is refused, or a file cannot be
/// read or written.
pub fn refresh(root: &Path, matrices: &[PathBuf]) -> eyre::Result<()> {
    let measured = read_measured(matrices)?;
    let owners = read_owners(root)?;
    let roster: BTreeSet<(String, DeclaredTypes)> = read_roster(root)?.into_iter().collect();
    let mut listed = BTreeSet::new();
    let mut refusals = Vec::new();
    let mut rewritten = Vec::new();
    for (name, mode) in CELL_FILES {
        let path = root.join(name);
        let text = fs::read_to_string(&path)?;
        let mut out = format!("{CELLS_HEADER}\n");
        for line in text.lines().skip(1) {
            let mut fields: Vec<&str> = line.split('\t').collect();
            let id = fields.first().copied().unwrap_or_default();
            listed.insert((id.to_string(), mode));
            let &(helm, main) = measured
                .get(id)
                .ok_or_else(|| eyre::eyre!("no table measured {id}"))?;
            let recorded = parse_expect(fields.last().copied().unwrap_or_default())
                .wrap_err_with(|| format!("{name}: {id}"))?;
            if let Some((recorded_helm, recorded_main)) = recorded {
                if recorded_helm != helm {
                    refusals.push(format!(
                        "{id}: Helm's class changed from {} to {}",
                        recorded_helm.as_str(),
                        helm.as_str()
                    ));
                } else if recorded_main.agrees_with(recorded_helm) && !main.agrees_with(helm) {
                    refusals.push(format!(
                        "{id}: NEW failure: main={} agreed with Helm, the table says {}",
                        recorded_main.as_str(),
                        main.as_str()
                    ));
                }
            }
            let expect = format!("helm_class={} main={}", helm.as_str(), main.as_str());
            if let Some(field) = fields.last_mut() {
                *field = &expect;
            }
            writeln!(out, "{}", fields.join("\t"))?;
        }
        rewritten.push((path, out));
    }
    eyre::ensure!(
        listed == roster,
        "the cells files differ from roster.tsv; add or remove a cell in both"
    );
    eyre::ensure!(
        refusals.is_empty(),
        "refusing to re-record:\n{}",
        refusals.join("\n")
    );
    // Written only once every cell was measured and admitted, so a refusal changes nothing.
    for (path, out) in rewritten {
        fs::write(path, out)?;
    }
    let suite = read_suite(root)?;
    let mut rows: Vec<(String, String)> = suite
        .cells
        .iter()
        .filter(|cell| cell.is_obligation())
        .map(|cell| {
            let owner = owners.get(&cell.id).map_or(DEFAULT_OWNER, String::as_str);
            (cell.id.clone(), cell.obligation_row(owner))
        })
        .collect();
    rows.sort();
    let mut out = format!("{OBLIGATIONS_HEADER}\n");
    for (_, row) in rows {
        writeln!(out, "{row}")?;
    }
    fs::write(root.join("obligations.tsv"), out)?;
    Ok(())
}

/// Every cell's measured Helm class and `main` verdict in `cell_matrix` tables; a later table wins.
fn read_measured(matrices: &[PathBuf]) -> eyre::Result<BTreeMap<String, (HelmClass, Verdict)>> {
    let mut measured = BTreeMap::new();
    for matrix in matrices {
        let text =
            fs::read_to_string(matrix).wrap_err_with(|| format!("read {}", matrix.display()))?;
        let mut lines = text.lines().filter(|line| !line.starts_with('#'));
        let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').collect();
        let column = |name: &str| {
            header
                .iter()
                .position(|field| *field == name)
                .ok_or_else(|| eyre::eyre!("{}: no {name} column", matrix.display()))
        };
        let (id_column, main_column, helm_column) =
            (column("cell_id")?, column("main")?, column("helm_class")?);
        for line in lines {
            let fields: Vec<&str> = line.split('\t').collect();
            let field = |index: usize| fields.get(index).copied().unwrap_or_default();
            let main = match field(main_column) {
                "accept" => Verdict::Accept,
                "reject" => Verdict::Reject,
                other => eyre::bail!("{}: main is {other:?}", field(id_column)),
            };
            let helm = match field(helm_column) {
                "pass" => HelmClass::Pass,
                "reject" => HelmClass::Reject,
                other => eyre::bail!("{}: helm_class is {other:?}", field(id_column)),
            };
            measured.insert(field(id_column).to_string(), (helm, main));
        }
    }
    Ok(measured)
}

fn read_owners(root: &Path) -> eyre::Result<BTreeMap<String, String>> {
    let text = fs::read_to_string(root.join("obligations.tsv")).wrap_err("read obligations.tsv")?;
    Ok(text
        .lines()
        .skip(1)
        .map(|row| {
            let id = row.split('\t').next().unwrap_or_default().to_string();
            let owner = row.rsplit('\t').next().unwrap_or_default().to_string();
            (id, owner)
        })
        .collect())
}

fn read_roster(root: &Path) -> eyre::Result<Vec<(String, DeclaredTypes)>> {
    let text = fs::read_to_string(root.join(ROSTER)).wrap_err("read roster.tsv")?;
    let mut lines = text.lines();
    eyre::ensure!(
        lines.next() == Some(ROSTER_HEADER),
        "roster.tsv: unexpected header"
    );
    let mut roster = Vec::new();
    for line in lines {
        let (id, mode) = line.split_once('\t').unwrap_or((line, ""));
        let mode = match mode {
            "annotate" => DeclaredTypes::Annotate,
            "assert" => DeclaredTypes::Assert,
            other => eyre::bail!("roster.tsv: {id} has mode {other:?}"),
        };
        roster.push((id.to_string(), mode));
    }
    Ok(roster)
}

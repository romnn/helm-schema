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

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use serde_json::Value;
use test_util::helm_values::{ValuesError, ValuesOptions, coalesce_chart_values};

use crate::cell_matrix::generation_recipe;
use crate::registry::BUNDLE_K8S_VERSION;

/// The suite's directory under the workspace `testdata/`.
pub const SUITE_DIR: &str = "w1-admission";

/// The cells files and the declared-types mode each was measured under.
pub const CELL_FILES: [(&str, DeclaredTypes); 2] = [
    ("cells.tsv", DeclaredTypes::Annotate),
    ("cells-assert.tsv", DeclaredTypes::Assert),
];

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
    Ok(Suite {
        root: root.to_path_buf(),
        cells,
        obligations,
    })
}

fn parse_cell(root: &Path, line: &str, declared_types: DeclaredTypes) -> eyre::Result<Cell> {
    let fields: Vec<&str> = line.split('\t').collect();
    let [id, chart, values, _kube_version, expect] = fields.as_slice() else {
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
    let mut helm = None;
    let mut main = None;
    for token in expect.split(' ') {
        match token {
            "helm_class=pass" => helm = Some(HelmClass::Pass),
            "helm_class=reject" => helm = Some(HelmClass::Reject),
            "main=accept" => main = Some(Verdict::Accept),
            "main=reject" => main = Some(Verdict::Reject),
            other => eyre::bail!("{id}: unexpected expectation {other:?}"),
        }
    }
    Ok(Cell {
        id: (*id).to_string(),
        chart: (*chart).to_string(),
        values,
        declared_types,
        helm: helm.ok_or_eyre("no helm_class expectation")?,
        main: main.ok_or_eyre("no main expectation")?,
    })
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
}

/// The directory of Helm-coalesced documents recorded for the cells the Rust values port does not
/// model (a YAML 1.1 boolean in `values.yaml`, for example).
const COALESCED_DIR: &str = "coalesced";

/// The values document a cell's schema validates: the cell's values composed over its chart's
/// defaults by the Rust port of Helm's coalescing, or, where the port does not model the input,
/// the document pinned Helm coalesced, recorded under `coalesced/`.
///
/// # Errors
///
/// Returns an error when neither yields a document.
pub fn compose(root: &Path, cell: &Cell, scratch: &Path) -> eyre::Result<Value> {
    let recorded = root.join(COALESCED_DIR).join(format!("{}.json", cell.id));
    if recorded.is_file() {
        return Ok(serde_json::from_slice(&fs::read(&recorded)?)?);
    }
    match port_composed(root, cell, scratch)? {
        Ok(instance) => Ok(instance),
        Err(reason) => Err(eyre::eyre!(
            "{reason}; record Helm's document with `admission_refresh --coalesced <helm>`"
        )),
    }
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

/// Records, under `coalesced/`, the document `helm` coalesces for every cell the Rust port does not
/// model, and removes every other recorded document. Returns the recorded cell ids.
///
/// # Errors
///
/// Returns an error when Helm fails or its output holds no document.
pub fn record_helm_coalesced(
    root: &Path,
    helm: &Path,
    scratch: &Path,
) -> eyre::Result<Vec<String>> {
    let suite = read_suite(root)?;
    let dir = root.join(COALESCED_DIR);
    if dir.is_dir() {
        fs::remove_dir_all(&dir)?;
    }
    let mut recorded = Vec::new();
    for cell in &suite.cells {
        if port_composed(root, cell, scratch)?.is_ok() {
            continue;
        }
        let chart = scratch.join(&cell.id);
        test_util::scratch::copy_tree(&root.join(&cell.chart), &chart)?;
        fs::remove_dir_all(chart.join("templates"))?;
        fs::create_dir_all(chart.join("templates"))?;
        fs::write(
            chart.join("templates/values.json"),
            "{{ toJson .Values }}\n",
        )?;
        let values = scratch.join(format!("{}.values", cell.id));
        let output = std::process::Command::new(helm)
            .args(["template", "probe"])
            .arg(&chart)
            .arg("-f")
            .arg(&values)
            .args(["--show-only", "templates/values.json"])
            .output()?;
        eyre::ensure!(
            output.status.success(),
            "{}: helm failed: {}",
            cell.id,
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout)?;
        let document = text
            .lines()
            .find(|line| line.starts_with('{'))
            .ok_or_else(|| eyre::eyre!("{}: no document in {text:?}", cell.id))?;
        let document: Value = serde_json::from_str(document)?;
        fs::create_dir_all(&dir)?;
        let mut bytes = serde_json::to_vec_pretty(&document)?;
        bytes.push(b'\n');
        fs::write(dir.join(format!("{}.json", cell.id)), bytes)?;
        recorded.push(cell.id.clone());
    }
    Ok(recorded)
}

/// Re-records the suite under `root` from `cell_matrix` tables (`matrix.tsv`) that measured every
/// cell with a binary labelled `main`: each cell's `helm_class=` and `main=` expectation, and
/// `obligations.tsv`, whose existing owners are kept.
///
/// # Errors
///
/// Returns an error when a table misses a cell, holds a class other than pass or reject, or a file
/// cannot be read or written.
pub fn refresh(root: &Path, matrices: &[PathBuf]) -> eyre::Result<()> {
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
                "accept" | "reject" => field(main_column),
                other => eyre::bail!("{}: main is {other:?}", field(id_column)),
            };
            let helm = match field(helm_column) {
                "pass" | "reject" => field(helm_column),
                other => eyre::bail!("{}: helm_class is {other:?}", field(id_column)),
            };
            measured.insert(
                field(id_column).to_string(),
                format!("helm_class={helm} main={main}"),
            );
        }
    }
    let owners: BTreeMap<String, String> = read_suite(root)?
        .obligations
        .into_iter()
        .map(|(id, row)| {
            let owner = row.rsplit('\t').next().unwrap_or_default().to_string();
            (id, owner)
        })
        .collect();
    let mut rewritten = Vec::new();
    for (name, _) in CELL_FILES {
        let path = root.join(name);
        let text = fs::read_to_string(&path)?;
        let mut out = format!("{CELLS_HEADER}\n");
        for line in text.lines().skip(1) {
            let mut fields: Vec<&str> = line.split('\t').collect();
            let id = fields.first().copied().unwrap_or_default();
            let expect = measured
                .get(id)
                .ok_or_else(|| eyre::eyre!("no table measured {id}"))?;
            if let Some(field) = fields.last_mut() {
                *field = expect;
            }
            writeln!(out, "{}", fields.join("\t"))?;
        }
        rewritten.push((path, out));
    }
    // Written only once every cell was measured, so a refused table changes nothing.
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

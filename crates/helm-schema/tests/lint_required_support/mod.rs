//! Chart construction and schema-enabled helmsweep calls for the dependency matrix.

mod chart;
mod engine;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema::AnalysisSession;
use helm_schema::generation::{AuthoringPolicy, DeclaredTypes, RootPolicy, SchemaProfile};
use helm_schema_test_support::generate::generate_options_at;
use helm_schema_test_support::registry::ChartRecipe;
use serde::{Deserialize as _, Serialize};
use serde_json::{Value, json};
use test_util::scratch::ScratchDir;

#[derive(Clone, Copy, Debug)]
pub enum DefaultValue {
    Absent,
    Null,
    Set,
}

impl DefaultValue {
    pub const ALL: [Self; 3] = [Self::Absent, Self::Null, Self::Set];

    fn value(self) -> Option<Value> {
        match self {
            Self::Absent => None,
            Self::Null => Some(Value::Null),
            Self::Set => Some(json!(false)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Feature {
    Plain,
    Alias,
    Condition,
    Tags,
    Global,
    ImportChild,
    ImportParent,
    Library,
    Required,
    Fail,
}

#[derive(Clone, Copy, Debug)]
pub enum Transport {
    File,
    Chart,
}

#[derive(Clone, Copy, Debug)]
pub enum Override {
    Absent,
    Partial,
    Delete,
    Set,
}

#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub depth: usize,
    pub child: DefaultValue,
    pub parent: DefaultValue,
    pub feature: Feature,
    pub guarded: bool,
    pub transport: Transport,
    pub overlay: Override,
}

#[derive(Debug, Serialize)]
struct Verdicts {
    cell: String,
    control: engine::Operations,
    generated: engine::Operations,
}

pub struct Matrix {
    scratch: ScratchDir,
    engine: engine::Engine,
    generator: Option<PathBuf>,
    report: Option<PathBuf>,
    rows: Vec<Verdicts>,
    failures: Vec<String>,
}

impl Matrix {
    pub fn new() -> eyre::Result<Self> {
        let scratch = ScratchDir::new("lint-required-matrix")?;
        let engine = engine::Engine::new(scratch.path())?;
        Ok(Self {
            scratch,
            engine,
            // A frozen CLI makes the same permanent test usable for pre-fix evidence.
            generator: std::env::var_os("HELM_SCHEMA_MATRIX_GENERATOR").map(PathBuf::from),
            report: std::env::var_os("HELM_SCHEMA_MATRIX_REPORT").map(PathBuf::from),
            rows: Vec::new(),
            failures: Vec::new(),
        })
    }

    pub fn check_configuration(&mut self, cell: Cell) -> eyre::Result<()> {
        for transport in [Transport::File, Transport::Chart] {
            for overlay in [
                Override::Absent,
                Override::Partial,
                Override::Delete,
                Override::Set,
            ] {
                self.check(Cell {
                    transport,
                    overlay,
                    ..cell
                })?;
            }
        }
        Ok(())
    }

    pub fn check_public_chart(&mut self, source: &Path) -> eyre::Result<()> {
        let directory = self.scratch.path().join("public-chart");
        let chart = directory.join("chart");
        chart::copy_public(source, &chart)?;
        write_json(&directory.join("override.yaml"), &json!({}))?;
        let schema = generate(&chart, false, self.generator.as_deref())?;
        let control = self.engine.run(&directory, &chart, "control")?;
        write_json(&chart.join("values.schema.json"), &schema)?;
        let generated = self.engine.run(&directory, &chart, "generated")?;
        if control.lint != 0
            || control.template != 0
            || generated.lint != 0
            || generated.template != 0
        {
            self.failures.push(format!(
                "public umbrella: schema-free={control:?}, generated={generated:?}"
            ));
        }
        self.rows.push(Verdicts {
            cell: "public daskhub 2024.1.1, provider v1.35.0".into(),
            control,
            generated,
        });
        Ok(())
    }

    pub fn check(&mut self, cell: Cell) -> eyre::Result<()> {
        let directory = self
            .scratch
            .path()
            .join(format!("cell-{}", self.rows.len()));
        let chart = chart::write(&directory, cell)?;
        let schema = generate(&chart, cell.guarded, self.generator.as_deref())?;
        let control = self.engine.run(&directory, &chart, "control")?;
        if control.lint != 0 || control.template != 0 {
            self.failures.push(format!(
                "{cell:?}: unusable schema-free control: {control:?}"
            ));
        }
        write_json(&chart.join("values.schema.json"), &schema)?;
        let generated = self.engine.run(&directory, &chart, "generated")?;
        self.check_rendered_documents(&directory, cell, &control, &generated)?;
        for operation in ["lint", "template"] {
            if control.verdict(operation)? != generated.verdict(operation)? {
                self.failures.push(format!(
                    "{cell:?}: {operation}: schema-free={control:?}, generated={generated:?}"
                ));
            }
        }
        self.rows.push(Verdicts {
            cell: format!("{cell:?}"),
            control,
            generated,
        });
        Ok(())
    }

    fn check_rendered_documents(
        &mut self,
        directory: &Path,
        cell: Cell,
        control: &engine::Operations,
        generated: &engine::Operations,
    ) -> eyre::Result<()> {
        if control.template != 0 {
            return Ok(());
        }
        let rendered = fs::read(directory.join("control-template.stdout"))?;
        let mut documents = Vec::new();
        for document in serde_yaml::Deserializer::from_slice(&rendered) {
            let document = serde_yaml::Value::deserialize(document)?;
            if !document.is_null() {
                documents.push(document);
            }
        }
        let expected = match cell.feature {
            Feature::Condition | Feature::Tags => 0,
            Feature::ImportChild | Feature::ImportParent => 2,
            Feature::Plain
            | Feature::Alias
            | Feature::Global
            | Feature::Library
            | Feature::Required
            | Feature::Fail => 1,
        };
        if documents.len() != expected {
            self.failures.push(format!(
                "{cell:?}: rendered {} documents, expected {expected}",
                documents.len()
            ));
        }
        if generated.template == 0
            && rendered != fs::read(directory.join("generated-template.stdout"))?
        {
            self.failures
                .push(format!("{cell:?}: schema changed rendered output"));
        }
        Ok(())
    }

    pub fn check_required(&mut self, cell: Cell) -> eyre::Result<()> {
        self.check(cell)?;
        for token in [None, Some(Value::Null), Some(json!(""))] {
            self.check_missing_token(cell, token)?;
        }
        // Helm's required accepts false; an explicit `if not ... | fail` rejects it.
        if matches!(cell.feature, Feature::Fail) {
            self.check_missing_token(cell, Some(json!(false)))?;
        }
        Ok(())
    }

    fn check_missing_token(&mut self, cell: Cell, token: Option<Value>) -> eyre::Result<()> {
        let directory = self
            .scratch
            .path()
            .join(format!("required-{}", self.rows.len()));
        let chart = chart::write(&directory, cell)?;
        let schema = generate(&chart, true, self.generator.as_deref())?;
        let validator = jsonschema::validator_for(&schema)?;
        let mut missing = json!({"kid": {"flag": true, "enableServiceLinks": false}});
        if let Some(token) = token {
            missing
                .pointer_mut("/kid")
                .and_then(Value::as_object_mut)
                .ok_or_eyre("dependency values absent")?
                .insert("token".into(), token);
        }
        if validator.is_valid(&missing) {
            self.failures.push(format!(
                "{cell:?}: schema accepts invalid required token: {missing}"
            ));
        }
        // Helm's LintMode suppresses required/fail, whereas template must still abort.
        match cell.transport {
            Transport::File => {
                write_json(&chart.join("values.yaml"), &json!({}))?;
                write_json(&directory.join("override.yaml"), &missing)?;
            }
            Transport::Chart => {
                write_json(&chart.join("values.yaml"), &missing)?;
                write_json(&directory.join("override.yaml"), &json!({}))?;
            }
        }
        let control = self.engine.run(&directory, &chart, "missing-control")?;
        write_json(&chart.join("values.schema.json"), &schema)?;
        let generated = self.engine.run(&directory, &chart, "missing-generated")?;
        if control.lint != 0
            || control.template != 1
            || generated.lint != 1
            || generated.template != 1
        {
            self.failures.push(format!(
                "{cell:?}: missing token: {control:?}, {generated:?}"
            ));
        }
        self.rows.push(Verdicts {
            cell: format!("invalid token {missing}: {cell:?}"),
            control,
            generated,
        });
        Ok(())
    }

    pub fn finish(self) -> eyre::Result<Vec<String>> {
        eprintln!(
            "dependency matrix: {} cells, {} mismatches",
            self.rows.len(),
            self.failures.len()
        );
        if let Some(path) = self.report {
            write_json(
                &path,
                &json!({"rows": self.rows, "failures": self.failures}),
            )?;
        }
        if !self.failures.is_empty() {
            eprintln!(
                "matrix evidence: {}",
                test_util::scratch::preserve(self.scratch.path())?.display()
            );
        }
        Ok(self.failures)
    }
}

fn generate(chart: &Path, guarded: bool, frozen: Option<&Path>) -> eyre::Result<Value> {
    let recipe = ChartRecipe {
        k8s_version: "v1.35.0",
        // Equivalence tests recovered facts; closed-root and declared-type assertions
        // deliberately accept fewer documents than Helm and have their own coverage.
        authoring: if guarded {
            AuthoringPolicy {
                root: RootPolicy::Open,
                declared_types: DeclaredTypes::Annotate,
            }
        } else {
            AuthoringPolicy::default()
        },
        ..ChartRecipe::corpus("matrix", SchemaProfile::Full)
    };
    if let Some(program) = frozen {
        let bundle = test_util::workspace_testdata().join("provider-bundle");
        let mut command = Command::new(program);
        command
            .arg(chart)
            .args([
                "--offline",
                "--exclude-tests",
                "--k8s-version",
                recipe.k8s_version,
            ])
            .arg("--k8s-schema-cache-dir")
            .arg(bundle.join("kubernetes-json-schema-cache"))
            .arg("--crd-catalog-cache-dir")
            .arg(bundle.join("crds-catalog-cache"));
        if guarded {
            command.args(["--open-root", "--declared-types", "annotate"]);
        }
        let output = command.output()?;
        eyre::ensure!(
            output.status.success(),
            "frozen generator: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(serde_json::from_slice(&output.stdout)?);
    }
    Ok(AnalysisSession::new(generate_options_at(chart, &recipe))
        .generated_schema()?
        .schema)
}

fn write_json(path: &Path, value: &Value) -> eyre::Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)
        .wrap_err_with(|| format!("write {}", path.display()))
}

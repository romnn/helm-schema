use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::helm::invocation::{HelmRunner, find_helmsweep};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Serialize)]
pub(super) struct Operations {
    pub lint: i32,
    pub template: i32,
}

impl Operations {
    pub fn verdict(&self, operation: &str) -> eyre::Result<i32> {
        match operation {
            "lint" => Ok(self.lint),
            "template" => Ok(self.template),
            other => eyre::bail!("unknown operation {other}"),
        }
    }
}

#[derive(Deserialize)]
struct Response {
    id: u64,
    exit_code: Option<i32>,
    error: Option<String>,
}

pub(super) struct Engine {
    program: PathBuf,
    home: PathBuf,
}

impl Engine {
    pub fn new(root: &Path) -> eyre::Result<Self> {
        let program = find_helmsweep()?;
        // The shared runner checks both the module pin and Helm release build identity.
        HelmRunner::with_helmsweep(
            &root.join("pin"),
            false,
            program.clone(),
            Duration::from_secs(120),
        )?;
        let home = root.join("home");
        fs::create_dir(&home)?;
        Ok(Self { program, home })
    }

    pub fn run(&self, directory: &Path, chart: &Path, stage: &str) -> eyre::Result<Operations> {
        let mut requests = Vec::new();
        for (id, operation) in [(1, "lint"), (2, "template")] {
            serde_json::to_writer(
                &mut requests,
                &json!({
                    "id": id, "op": operation,
                    "release": if operation == "template" { "matrix" } else { "" },
                    "chart": chart, "kube_version": "1.35.0",
                    "values": [directory.join("override.yaml")],
                    // Unlike the battery's schema-free render, this exercises Helm's validator.
                    "skip_schema_validation": false,
                    "stdout_path": directory.join(format!("{stage}-{operation}.stdout")),
                    "stderr_path": directory.join(format!("{stage}-{operation}.stderr"))
                }),
            )?;
            requests.push(b'\n');
        }
        let input = directory.join(format!("{stage}.requests.jsonl"));
        fs::write(&input, requests)?;
        let mut command = Command::new(&self.program);
        command
            .arg("serve")
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join("config"))
            .env("XDG_CACHE_HOME", self.home.join("cache"))
            .env("XDG_DATA_HOME", self.home.join("data"))
            .env("GOMAXPROCS", "1")
            .stdin(Stdio::from(fs::File::open(input)?));
        let output = command.output()?;
        eyre::ensure!(
            output.status.success(),
            "helmsweep: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut operations = Operations {
            lint: -1,
            template: -1,
        };
        let mut count = 0;
        for response in serde_json::Deserializer::from_slice(&output.stdout).into_iter::<Response>()
        {
            let response = response?;
            eyre::ensure!(response.error.is_none(), "helmsweep: {:?}", response.error);
            let code = response
                .exit_code
                .ok_or_eyre("helmsweep omitted exit code")?;
            eyre::ensure!([0, 1].contains(&code), "abnormal Helm exit {code}");
            match response.id {
                1 => operations.lint = code,
                2 => operations.template = code,
                other => eyre::bail!("unexpected response {other}"),
            }
            count += 1;
        }
        eyre::ensure!(count == 2, "expected two Helm responses, got {count}");
        Ok(operations)
    }
}

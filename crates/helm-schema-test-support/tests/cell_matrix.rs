//! End-to-end tests of the `cell_matrix` binary: tiny charts, fake `helm-schema` binaries
//! that write a fixed schema (and one real `helm-schema` built from this workspace), the
//! pinned Helm v4.2.3 and helmsweep (integration profile).
//!
//! POSIX only: the fake binaries and fake Helm are shell scripts.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::helm::invocation::{find_helm, find_helmsweep};
use helm_schema_test_support::registry::PROVIDER_BUNDLE;
use indoc::{formatdoc, indoc};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::{ScratchDir, copy_tree, target_dir};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// A fake `helm-schema` running the shell `generate` with `$out` (the `--output` path) and
/// `$cache` (the `--k8s-schema-cache-dir` path) set, logging its arguments to `<name>.args`.
fn fake_bin(dir: &Path, name: &str, generate: &str) -> eyre::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;
    let path = dir.join(name);
    let script = formatdoc! {r#"
        #!/bin/sh
        echo "$@" >> '{log}'
        out=; cache=; prev=
        for a in "$@"; do
          [ "$prev" = --output ] && out=$a
          [ "$prev" = --k8s-schema-cache-dir ] && cache=$a
          prev=$a
        done
        {generate}
        "#,
        log = dir.join(format!("{name}.args")).display(),
    };
    fs::write(&path, script)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// The shell that writes `schema` to `$out`, or crashes without one.
fn writes(schema: Option<&str>) -> String {
    schema.map_or_else(
        || "echo 'generation crashed' >&2; exit 1".to_string(),
        |schema| format!("printf '%s' '{schema}' > \"$out\""),
    )
}

fn write(path: &Path, text: &str) -> eyre::Result<()> {
    fs::create_dir_all(path.parent().ok_or_else(|| eyre::eyre!("no parent"))?)?;
    fs::write(path, text)?;
    Ok(())
}

/// A chart `name` under `root` with `values.yaml` and one `ConfigMap` template.
fn chart(root: &Path, name: &str, values: &str, template: &str) -> eyre::Result<()> {
    write(
        &root.join(name).join("Chart.yaml"),
        &formatdoc! {"
            apiVersion: v2
            name: {name}
            version: 1.0.0
        "},
    )?;
    write(&root.join(name).join("values.yaml"), values)?;
    write(&root.join(name).join("templates/main.yaml"), template)
}

struct Run {
    root: ScratchDir,
    bins: Vec<String>,
}

impl Run {
    fn new() -> eyre::Result<Self> {
        Ok(Self {
            root: ScratchDir::new("cell_matrix")?,
            bins: Vec::new(),
        })
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn bin(&mut self, label: &str, schema: Option<&str>) -> eyre::Result<()> {
        self.script_bin(label, &writes(schema))
    }

    fn script_bin(&mut self, label: &str, generate: &str) -> eyre::Result<()> {
        let path = fake_bin(self.path(), label, generate)?;
        self.bins
            .push(format!("{label}={}@{COMMIT}", path.display()));
        Ok(())
    }

    fn cells(&self, rows: &[&str]) -> eyre::Result<()> {
        let mut text = String::from("cell_id\tchart\tvalues\tkube_version\texpect\n");
        for row in rows {
            text.push_str(row);
            text.push('\n');
        }
        write(&self.path().join("cells.tsv"), &text)
    }

    /// Runs `cell_matrix` in the run's directory, with the cargo target directory named.
    fn run(&self, extra: &[&str], env: &[(&str, &str)]) -> eyre::Result<Output> {
        self.run_with(&find_helm()?, &find_helmsweep()?, extra, env)
    }

    fn run_with(
        &self,
        helm: &Path,
        helmsweep: &Path,
        extra: &[&str],
        env: &[(&str, &str)],
    ) -> eyre::Result<Output> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cell_matrix"));
        command
            .current_dir(self.path())
            .env("CARGO_TARGET_DIR", target_dir())
            .arg("--cells")
            .arg(self.path().join("cells.tsv"))
            .arg("--helm")
            .arg(helm)
            .arg("--helmsweep")
            .arg(helmsweep)
            .arg("--out")
            .arg(self.path().join("out"));
        for bin in &self.bins {
            command.arg("--bin").arg(bin);
        }
        command.args(extra).envs(env.iter().copied());
        Ok(command.output()?)
    }

    fn file(&self, name: &str) -> eyre::Result<String> {
        Ok(fs::read_to_string(self.path().join("out").join(name))?)
    }

    /// matrix.tsv's rows, without the header lines that name local executables.
    fn rows(&self) -> eyre::Result<Vec<String>> {
        Ok(self
            .file("matrix.tsv")?
            .lines()
            .filter(|line| !line.starts_with('#'))
            .map(|line| {
                // The prepared chart's content hash is checked by the manifest test.
                let mut fields: Vec<&str> = line.split('\t').collect();
                if let Some(field) = fields.get_mut(1) {
                    *field = if *field == "prep_sha256" {
                        "prep_sha256"
                    } else {
                        "<sha>"
                    };
                }
                fields.join("\t")
            })
            .collect())
    }
}

fn code(output: &Output) -> Option<i32> {
    output.status.code()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

const APP_VALUES: &str = indoc! {"
    replicas: 1
    name: base
"};
const ACCEPT_ALL: &str = "{}";
const INTEGER_REPLICAS: &str = r#"{"type":"object","properties":{"replicas":{"type":"integer"}}}"#;
const CONFIGMAP: &str = indoc! {r"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: app
    data:
      replicas: {{ .Values.replicas | quote }}
      name: {{ .Values.name | quote }}
"};

/// Verdicts per column over Helm's own coalesced values: null deletion, the original
/// override bytes (a YAML anchor), a prerelease Kubernetes version, the prepared chart
/// (a shipped values.schema.json and templates/tests are removed on both sides), and
/// `--check` reproducing the bytes without writing.
#[test]
fn a_matrix_reproduces_and_measures_the_prepared_chart() -> eyre::Result<()> {
    let mut run = Run::new()?;
    chart(run.path(), "app", APP_VALUES, CONFIGMAP)?;
    write(&run.path().join("app/values.schema.json"), r#"{"not": {}}"#)?;
    write(
        &run.path().join("app/templates/tests/fail.yaml"),
        "{{ fail \"tests ran\" }}",
    )?;
    write(
        &run.path().join("app/helm-schema.yaml"),
        "not: valid: policy\n",
    )?;
    write(
        &run.path().join("values/anchor.yaml"),
        indoc! {"
            base: &n anchored
            name: *n
        "},
    )?;
    run.bin("A", Some(ACCEPT_ALL))?;
    run.bin("B", Some(INTEGER_REPLICAS))?;
    run.cells(&[
        "c1\tapp\t{}\t1.29.0\tA=accept B=accept helm_rc=0 helm_class=pass k8s=valid",
        "c2\tapp\t{replicas: two}\t1.29.0\tA=accept B=reject helm_class=pass",
        "c3\tapp\t{replicas: null}\t1.29.0-rc.1\tB=accept helm_rc=0",
        "c4\tapp\t@values/anchor.yaml\t1.29.0\t-",
    ])?;
    let output = run.run(&[], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(0), "{}", stderr(&output));
    sim_assert_eq!(
        have: run.rows()?,
        want: vec![
            "cell_id\tprep_sha256\tvalues_src\tA\tB\thelm_rc\thelm_class\thelm_abort\tk8s\texpect_ok",
            "c1\t<sha>\thelm\taccept\taccept\t0\tpass\t-\tvalid\tyes",
            "c2\t<sha>\thelm\taccept\treject\t0\tpass\t-\tvalid\tyes",
            "c3\t<sha>\thelm\taccept\taccept\t0\tpass\t-\tvalid\tyes",
            "c4\t<sha>\thelm\taccept\taccept\t0\tpass\t-\tvalid\t-",
        ],
    );
    sim_assert_eq!(
        have: run.file("diagnostics.tsv")?,
        want: "cell_id\tcolumn\tinstance_ptr\tkeyword\tdetail\tschema_ptr\nc2\tB\t/replicas\ttype\t\"two\"\t/properties/replicas/type\n",
    );
    let local = run.path().to_string_lossy().to_string();
    for name in ["matrix.tsv", "diagnostics.tsv", "manifest.json"] {
        eyre::ensure!(
            !run.file(name)?.contains(&local),
            "{name} names a local path"
        );
    }
    let args = fs::read_to_string(run.path().join("A.args"))?;
    eyre::ensure!(
        args.contains("--no-config") && args.contains("--exclude-tests"),
        "recipe: {args}"
    );

    let before: Vec<Vec<u8>> = ["matrix.tsv", "diagnostics.tsv", "manifest.json"]
        .iter()
        .map(|name| fs::read(run.path().join("out").join(name)))
        .collect::<Result<_, _>>()?;
    let check = run.run(&["--check", "--require-expected"], &[])?;
    sim_assert_eq!(have: code(&check), want: Some(2), "c4 carries no expectation: {}", stderr(&check));
    let check = run.run(&["--check"], &[])?;
    sim_assert_eq!(have: code(&check), want: Some(0), "{}", stderr(&check));
    let tampered = run.file("matrix.tsv")?.replace("\treject\t", "\taccept\t");
    fs::write(run.path().join("out/matrix.tsv"), &tampered)?;
    let check = run.run(&["--check"], &[])?;
    sim_assert_eq!(have: code(&check), want: Some(1), "{}", stderr(&check));
    sim_assert_eq!(have: run.file("matrix.tsv")?, want: tampered, "--check never writes");
    fs::write(
        run.path().join("out/matrix.tsv"),
        before.first().ok_or_else(|| eyre::eyre!("no bytes"))?,
    )?;
    Ok(())
}

/// No coalesced document (Helm aborts on a non-map dependency scope and the Rust port does
/// not validate one) reads `unavailable`, never accept or reject; an empty render and a
/// resource without a pinned schema are told apart.
#[test]
fn unavailable_instances_empty_renders_and_undecided_resources() -> eyre::Result<()> {
    let mut run = Run::new()?;
    chart(run.path(), "parent", "child: {}\n", "")?;
    write(
        &run.path().join("parent/Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: parent
            version: 1.0.0
            dependencies:
              - name: child
                version: 1.0.0
        "},
    )?;
    chart(&run.path().join("parent/charts"), "child", "a: 1\n", "")?;
    chart(
        run.path(),
        "custom",
        "{}\n",
        indoc! {"
            apiVersion: example.com/v1
            kind: Widget
            metadata:
              name: w
        "},
    )?;
    run.bin("A", Some(ACCEPT_ALL))?;
    run.cells(&[
        "u1\tparent\t{child: 5}\t1.29.0\tA=unavailable helm_rc=1 k8s=-",
        "u2\tparent\t{}\t1.29.0\tA=accept k8s=empty",
        "u3\tcustom\t{}\t1.29.0\tk8s=uncertain",
    ])?;
    let output = run.run(&["--require-expected"], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(0), "{}", stderr(&output));
    sim_assert_eq!(
        have: run.rows()?,
        want: vec![
            "cell_id\tprep_sha256\tvalues_src\tA\thelm_rc\thelm_class\thelm_abort\tk8s\texpect_ok",
            "u1\t<sha>\t-\tunavailable\t1\tunresolved:unknown-template-failure\tError: chart dependencies processing failed: type mismatch on child: %!t(float64=5)\t-\tyes",
            "u2\t<sha>\thelm\taccept\t0\tpass\t-\tempty\tyes",
            "u3\t<sha>\thelm\taccept\t0\tpass\t-\tuncertain\tyes",
        ],
    );
    sim_assert_eq!(
        have: run.file("diagnostics.tsv")?,
        want: "cell_id\tcolumn\tinstance_ptr\tkeyword\tdetail\tschema_ptr\n\
               u1\tvalues\t\tunavailable\tHelm aborts: type mismatch on child\t\n\
               u3\tk8s\t\tuncertain\tdocument 0: example.com/v1/Widget w: pinned CRD schema not found\t\n",
    );
    Ok(())
}

/// Refusals (exit 2, nothing written) and harness failures (exit 3, never a reject).
#[test]
fn refusals_and_harness_failures_write_nothing() -> eyre::Result<()> {
    let mut run = Run::new()?;
    chart(run.path(), "app", APP_VALUES, CONFIGMAP)?;
    run.bin("A", Some(ACCEPT_ALL))?;
    for (rows, flags, reason) in [
        (vec![], vec![], "holds no cells"),
        (
            vec!["c1\tapp\t{}\t1.29.0\t-", "c1\tapp\t{}\t1.29.0\t-"],
            vec![],
            "duplicate cell id",
        ),
        (
            vec!["c1\tapp\t{}\t1.29.0\tZ=accept"],
            vec![],
            "unknown column",
        ),
        (
            vec!["c1\tapp\t{}\t1.29.0\thelm_class=unresolved:loader"],
            vec![],
            "cannot be expected",
        ),
        (
            vec!["c1\tapp\t{}\t1.29.0\t-"],
            vec!["--require-expected"],
            "carries no expectation",
        ),
        (vec!["c1\tapp\t{}\tlatest\t-"], vec![], "bad kube version"),
        (
            vec!["c1\tapp\t{}\t1.29.0\t-"],
            vec!["--validate-release", "v1.29.0-standalone-strict/../v1.35.0"],
            "is not a release directory of the bundle",
        ),
        (
            vec!["c1\tapp\t{}\t1.29.0\t-"],
            vec!["--validate-release", "v9.9.9"],
            "is not a release directory of the bundle",
        ),
        (
            vec!["c1\tapp\t{}\t1.29.0\t-"],
            vec!["--gen-k8s-version", "v1.29.0/../../../x"],
            "is not a release directory of the bundle",
        ),
        (
            vec!["c1\tmissing\t{}\t1.29.0\t-"],
            vec![],
            "no chart directory",
        ),
    ] {
        run.cells(&rows)?;
        let output = run.run(&flags, &[])?;
        sim_assert_eq!(have: code(&output), want: Some(2), "{reason}: {}", stderr(&output));
        eyre::ensure!(
            stderr(&output).contains(reason),
            "{reason}: {}",
            stderr(&output)
        );
        eyre::ensure!(!run.path().join("out").exists(), "{reason}: wrote outputs");
    }
    run.cells(&["c1\tapp\t{}\t1.29.0\t-"])?;
    let bundle = run.path().join("bundle");
    fs::create_dir_all(bundle.join("kubernetes-json-schema-cache"))?;
    fs::create_dir_all(bundle.join("crds-catalog-cache"))?;
    let output = run.run(&["--bundle", &bundle.to_string_lossy()], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(2), "a wrong but unchanging bundle: {}", stderr(&output));
    for (label, schema) in [
        ("crash", None),
        ("not-json", Some("{")),
        ("uncompilable", Some(r#"{"type": 5}"#)),
    ] {
        let mut failing = Run::new()?;
        chart(failing.path(), "app", APP_VALUES, CONFIGMAP)?;
        failing.bin(label, schema)?;
        failing.cells(&["c1\tapp\t{}\t1.29.0\t-"])?;
        let output = failing.run(&[], &[])?;
        sim_assert_eq!(have: code(&output), want: Some(3), "{label}: {}", stderr(&output));
        eyre::ensure!(
            !failing.path().join("out").exists(),
            "{label}: wrote outputs"
        );
    }
    Ok(())
}

/// Only `--helm` renders: an unusable ambient Helm and engine selection change nothing; a
/// chart whose renders are not replayable is outside `--check`'s guarantee.
#[test]
fn only_the_named_helm_runs_and_nondeterministic_charts_refuse_check() -> eyre::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut run = Run::new()?;
    chart(run.path(), "app", APP_VALUES, CONFIGMAP)?;
    chart(
        run.path(),
        "clock",
        "{}\n",
        indoc! {"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: c
            data:
              at: {{ now | quote }}
        "},
    )?;
    let ambient = run.path().join("ambient");
    write(
        &ambient.join("helm"),
        indoc! {"
            #!/bin/sh
            echo v3.0.0
            exit 1
        "},
    )?;
    fs::set_permissions(ambient.join("helm"), fs::Permissions::from_mode(0o755))?;
    run.bin("A", Some(ACCEPT_ALL))?;
    run.cells(&["c1\tapp\t{}\t1.29.0\tA=accept"])?;
    let path = format!("{}:{}", ambient.display(), std::env::var("PATH")?);
    let output = run.run(&[], &[("PATH", &path), ("SCHEMA_HELM_ENGINE", "bogus")])?;
    sim_assert_eq!(have: code(&output), want: Some(0), "{}", stderr(&output));
    run.cells(&["c1\tapp\t{}\t1.29.0\tA=accept", "c2\tclock\t{}\t1.29.0\t-"])?;
    let output = run.run(&[], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(0), "{}", stderr(&output));
    let output = run.run(&["--check"], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(2), "{}", stderr(&output));
    eyre::ensure!(
        stderr(&output).contains("nondeterministically"),
        "{}",
        stderr(&output)
    );
    Ok(())
}

/// The committed provider bundle is the one `cell_matrix` pins: a bundle update must update the digests.
#[test]
fn the_committed_bundle_matches_its_pinned_digests() -> eyre::Result<()> {
    use helm_schema_test_support::cell_matrix::{CRD_BUNDLE_SHA256, KUBERNETES_BUNDLE_SHA256};
    use helm_schema_test_support::helm::invocation::tree_sha256;
    use helm_schema_test_support::registry::PROVIDER_BUNDLE;
    let bundle = test_util::workspace_testdata().join(PROVIDER_BUNDLE);
    sim_assert_eq!(
        have: [
            tree_sha256(&bundle.join("kubernetes-json-schema-cache"))?,
            tree_sha256(&bundle.join("crds-catalog-cache"))?,
        ],
        want: [KUBERNETES_BUNDLE_SHA256.to_string(), CRD_BUNDLE_SHA256.to_string()],
    );
    Ok(())
}

/// Builds this workspace's `helm-schema` binary and returns its path.
fn build_helm_schema() -> eyre::Result<PathBuf> {
    let output = Command::new(env!("CARGO"))
        .current_dir(test_util::workspace_root())
        .args([
            "build",
            "--offline",
            "-p",
            "helm-schema-cli",
            "--bin",
            "helm-schema",
            "--message-format",
            "json",
        ])
        .output()?;
    eyre::ensure!(
        output.status.success(),
        "cargo build: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for line in String::from_utf8(output.stdout)?.lines() {
        let message: serde_json::Value = serde_json::from_str(line)?;
        let text = |pointer: &str| message.pointer(pointer).and_then(serde_json::Value::as_str);
        if text("/reason") == Some("compiler-artifact")
            && text("/target/name") == Some("helm-schema")
            && let Some(executable) = text("/executable")
        {
            return Ok(PathBuf::from(executable));
        }
    }
    eyre::bail!("cargo build named no helm-schema executable")
}

/// The real CLI generates with the recipe, from a binary and a bundle named by paths relative
/// to the invocation directory, and `--check --require-expected` reproduces the committed outputs.
#[test]
fn the_real_cli_generates_from_relative_paths_and_reproduces() -> eyre::Result<()> {
    use std::os::unix::fs::symlink;
    let mut run = Run::new()?;
    copy_tree(
        &test_util::workspace_testdata().join("charts/dict-config"),
        &run.path().join("dict-config"),
    )?;
    symlink(build_helm_schema()?, run.path().join("helm-schema"))?;
    symlink(
        test_util::workspace_testdata().join(PROVIDER_BUNDLE),
        run.path().join("bundle"),
    )?;
    run.bins.push(format!("real=./helm-schema@{COMMIT}"));
    run.cells(&[
        "s1\tdict-config\t{}\t1.29.0\treal=accept helm_class=pass k8s=empty",
        "s2\tdict-config\t{podDisruptionBudget: {enabled: true}}\t1.29.0\treal=accept helm_class=pass k8s=valid",
        "s3\tdict-config\t{podDisruptionBudget: {enabled: true, minAvailable: \"50%\"}}\t1.29.0\treal=accept k8s=valid",
        "s4\tdict-config\t{podDisruptionBudget: {enabled: true, minAvailable: [1]}}\t1.29.0\treal=reject helm_class=pass k8s=invalid",
        "s5\tdict-config\t{ingress: {enabled: true, className: 5}}\t1.29.0\treal=reject helm_class=pass k8s=invalid",
    ])?;
    let output = run.run(&["--bundle", "./bundle", "--require-expected"], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(0), "{}", stderr(&output));
    sim_assert_eq!(
        have: run.rows()?,
        want: vec![
            "cell_id\tprep_sha256\tvalues_src\treal\thelm_rc\thelm_class\thelm_abort\tk8s\texpect_ok",
            "s1\t<sha>\thelm\taccept\t0\tpass\t-\tempty\tyes",
            "s2\t<sha>\thelm\taccept\t0\tpass\t-\tvalid\tyes",
            "s3\t<sha>\thelm\taccept\t0\tpass\t-\tvalid\tyes",
            "s4\t<sha>\thelm\treject\t0\tpass\t-\tinvalid\tyes",
            "s5\t<sha>\thelm\treject\t0\tpass\t-\tinvalid\tyes",
        ],
    );
    sim_assert_eq!(
        have: run.file("diagnostics.tsv")?,
        want: "cell_id\tcolumn\tinstance_ptr\tkeyword\tdetail\tschema_ptr\n\
               s4\tk8s\tpolicy/v1/PodDisruptionBudget /spec/minAvailable\toneOf\tarray\t/properties/spec/properties/minAvailable/oneOf\n\
               s4\treal\t/podDisruptionBudget/minAvailable\toneOf\tarray\t/$defs/values~1podDisruptionBudget~1oneOf~1anyOf~1integer+null+string;pattern-0-1-9+string;pattern-a-za-z0-9-t+2more+1more@2/oneOf\n\
               s5\tk8s\tnetworking.k8s.io/v1/Ingress /spec/ingressClassName\ttype\t5\t/properties/spec/properties/ingressClassName/type\n\
               s5\treal\t/ingress/className\tanyOf\t5\t/properties/ingress/allOf/0/then/properties/className/anyOf\n",
    );
    let check = run.run(
        &["--bundle", "./bundle", "--check", "--require-expected"],
        &[],
    )?;
    sim_assert_eq!(have: code(&check), want: Some(0), "{}", stderr(&check));
    Ok(())
}

/// A hung generation and a hung Helm render are harness failures (exit 3), never verdicts.
#[test]
fn slow_generations_and_slow_helm_renders_time_out() -> eyre::Result<()> {
    let helm = find_helm()?;
    let helmsweep = find_helmsweep()?;
    // Each fake leaves a background descendant, whose pid it records: `hang` never exits; `pipes_kept`
    // exits normally but its descendant keeps the output pipes open.
    let hang = "sleep 60 & echo $! > '@DESCENDANT_FILE@'; exec sleep 60";
    let pipes_kept = |real: &Path| {
        format!(
            "out=$('{}' \"$@\"); rc=$?; sleep 60 & echo $! > '@DESCENDANT_FILE@'; printf '%s' \"$out\"; exit $rc",
            real.display()
        )
    };
    let wrapper = |on: &str, body: &str, real: &Path| {
        formatdoc! {r#"
            #!/bin/sh
            if [ "$1" = {on} ]; then {body}; fi
            exec '{real}' "$@"
            "#,
            real = real.display(),
        }
    };
    let generation_held =
        "sleep 60 & echo $! > '@DESCENDANT_FILE@'; printf '%s' '{}' > \"$out\"; exit 0";
    // (case, generation, fake helm, fake helmsweep, what the failure must name)
    let cases = [
        (
            "generation hangs",
            Some(hang.to_string()),
            None,
            None,
            vec!["generating chart app (cells c1, c2)", "--bin A timed out"],
        ),
        (
            "generation holds its pipes",
            Some(generation_held.to_string()),
            None,
            None,
            vec!["generating chart app (cells c1, c2)", "--bin A timed out"],
        ),
        (
            "helm version hangs",
            None,
            Some(wrapper("version", hang, &helm)),
            None,
            vec!["[\"version\"", "timed out"],
        ),
        (
            "helm version holds its pipes",
            None,
            Some(wrapper("version", &pipes_kept(&helm), &helm)),
            None,
            vec!["[\"version\"", "timed out"],
        ),
        (
            "helm template hangs",
            None,
            Some(wrapper("template", hang, &helm)),
            None,
            vec!["cell c1", "Helm", "timed out"],
        ),
        (
            "helmsweep version holds its pipes",
            None,
            None,
            Some(wrapper("version", &pipes_kept(&helmsweep), &helmsweep)),
            vec!["version timed out"],
        ),
        (
            "helmsweep classify hangs",
            None,
            None,
            Some(wrapper("classify", hang, &helmsweep)),
            vec!["cell c1", "classify timed out"],
        ),
        (
            "helmsweep classify holds its pipes",
            None,
            None,
            Some(wrapper("classify", &pipes_kept(&helmsweep), &helmsweep)),
            vec!["cell c1", "classify timed out"],
        ),
    ];
    for (case, generation, fake_helm, fake_helmsweep, named) in cases {
        expect_timeout(
            case,
            generation.as_deref(),
            fake_helm.as_deref(),
            fake_helmsweep.as_deref(),
            &named,
        )?;
    }
    Ok(())
}

/// Runs one matrix whose `generation` (a fake binary's shell), Helm or helmsweep (fake scripts, else
/// the pinned programs) outlives `--timeout 5`: exit 3 naming `named`, no outputs, and the fake's whole
/// process group killed.
fn expect_timeout(
    case: &str,
    generation: Option<&str>,
    fake_helm: Option<&str>,
    fake_helmsweep: Option<&str>,
    named: &[&str],
) -> eyre::Result<()> {
    let mut run = Run::new()?;
    chart(run.path(), "app", APP_VALUES, CONFIGMAP)?;
    // The children run under a cleared environment: the fakes learn the file from their text.
    let descendant_file = run.path().join("descendant");
    let descendant_path = descendant_file.to_string_lossy().to_string();
    let generation = generation.map(|body| body.replace("@DESCENDANT_FILE@", &descendant_path));
    let fake_helm = fake_helm.map(|script| script.replace("@DESCENDANT_FILE@", &descendant_path));
    let fake_helmsweep =
        fake_helmsweep.map(|script| script.replace("@DESCENDANT_FILE@", &descendant_path));
    match generation.as_deref() {
        Some(body) => run.script_bin("A", body)?,
        None => run.bin("A", Some(ACCEPT_ALL))?,
    }
    run.cells(&["c1\tapp\t{}\t1.29.0\t-", "c2\tapp\t{}\t1.29.0\t-"])?;
    let helm = match fake_helm.as_deref() {
        Some(script) => executable(&run.path().join("fake-helm"), script)?,
        None => find_helm()?,
    };
    let helmsweep = match fake_helmsweep.as_deref() {
        Some(script) => executable(&run.path().join("fake-helmsweep"), script)?,
        None => find_helmsweep()?,
    };
    let started = std::time::Instant::now();
    let output = run.run_with(&helm, &helmsweep, &["--timeout", "5"], &[])?;
    let elapsed = started.elapsed();
    sim_assert_eq!(have: code(&output), want: Some(3), "{case}: {}", stderr(&output));
    for name in named {
        eyre::ensure!(
            stderr(&output).contains(name),
            "{case}: no {name:?} in {}",
            stderr(&output)
        );
    }
    eyre::ensure!(!run.path().join("out").exists(), "{case}: wrote outputs");
    eyre::ensure!(
        elapsed < std::time::Duration::from_secs(40),
        "{case}: took {elapsed:?}"
    );
    let descendant = fs::read_to_string(&descendant_file)?.trim().to_string();
    eyre::ensure!(!descendant.is_empty(), "{case}: the fake never ran");
    eyre::ensure!(
        process_is_gone(&descendant)?,
        "{case}: the fake's background descendant {descendant} survived the timeout"
    );
    Ok(())
}

/// Whether process `pid` is gone (waiting up to two seconds for the kill to land).
fn process_is_gone(pid: &str) -> eyre::Result<bool> {
    for _ in 0..20 {
        let alive = Command::new("kill")
            .args(["-0", pid])
            .stderr(std::process::Stdio::null())
            .status()?;
        if !alive.success() {
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(false)
}

/// Writes the executable `text` to `path`.
fn executable(path: &Path, text: &str) -> eyre::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;
    write(path, text)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(path.to_path_buf())
}

/// A render of nothing but separators and comments, or of null documents, is `empty`.
#[test]
fn comment_only_and_null_renders_are_empty() -> eyre::Result<()> {
    let mut run = Run::new()?;
    chart(
        run.path(),
        "comments",
        "{}\n",
        indoc! {"
            ---
            # a comment
            #
        "},
    )?;
    chart(
        run.path(),
        "nulls",
        "{}\n",
        indoc! {"
            ---
            ~
            ---
            null
        "},
    )?;
    run.bin("A", Some(ACCEPT_ALL))?;
    run.cells(&[
        "e1\tcomments\t{}\t1.29.0\tk8s=empty",
        "e2\tnulls\t{}\t1.29.0\tk8s=empty",
    ])?;
    let output = run.run(&["--require-expected"], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(0), "{}", stderr(&output));
    sim_assert_eq!(
        have: run.rows()?,
        want: vec![
            "cell_id\tprep_sha256\tvalues_src\tA\thelm_rc\thelm_class\thelm_abort\tk8s\texpect_ok",
            "e1\t<sha>\thelm\taccept\t0\tpass\t-\tempty\tyes",
            "e2\t<sha>\thelm\taccept\t0\tpass\t-\tempty\tyes",
        ],
    );
    Ok(())
}

/// The bundle the validator reads must be the committed one after every generation ran.
#[test]
fn a_bundle_changed_during_generation_is_refused() -> eyre::Result<()> {
    let mut run = Run::new()?;
    chart(run.path(), "app", APP_VALUES, CONFIGMAP)?;
    copy_tree(
        &test_util::workspace_testdata().join(PROVIDER_BUNDLE),
        &run.path().join("bundle"),
    )?;
    run.script_bin(
        "tamper",
        "echo '{}' > \"$cache/added.json\"; printf '%s' '{}' > \"$out\"",
    )?;
    run.cells(&["c1\tapp\t{}\t1.29.0\t-"])?;
    let output = run.run(&["--bundle", "bundle"], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(2), "{}", stderr(&output));
    eyre::ensure!(
        stderr(&output).contains("bundle kubernetes-json-schema-cache is"),
        "{}",
        stderr(&output)
    );
    eyre::ensure!(
        run.path().join("tamper.args").exists(),
        "the generation ran"
    );
    eyre::ensure!(!run.path().join("out").exists(), "wrote outputs");
    Ok(())
}

/// `--check` refuses (exit 2) other binaries than the recorded ones, or no recorded manifest,
/// before anything runs.
#[test]
fn check_refuses_unrecorded_binaries_before_running() -> eyre::Result<()> {
    let mut run = Run::new()?;
    chart(run.path(), "app", APP_VALUES, CONFIGMAP)?;
    run.bin("A", Some(ACCEPT_ALL))?;
    run.cells(&["c1\tapp\t{}\t1.29.0\tA=accept"])?;
    let output = run.run(&["--check"], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(2), "{}", stderr(&output));
    eyre::ensure!(
        stderr(&output).contains("needs the recorded"),
        "{}",
        stderr(&output)
    );
    let output = run.run(&[], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(0), "{}", stderr(&output));

    run.bins.clear();
    run.bin("A", Some(INTEGER_REPLICAS))?;
    fs::remove_file(run.path().join("A.args"))?;
    let output = run.run(&["--check"], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(2), "{}", stderr(&output));
    eyre::ensure!(
        stderr(&output).contains("not the recorded"),
        "{}",
        stderr(&output)
    );
    eyre::ensure!(
        !run.path().join("A.args").exists(),
        "the changed binary ran"
    );
    Ok(())
}

/// Scratch must live in the cargo target directory named by `CARGO_TARGET_DIR`.
#[test]
fn scratch_outside_the_target_directory_is_refused() -> eyre::Result<()> {
    let mut run = Run::new()?;
    chart(run.path(), "app", APP_VALUES, CONFIGMAP)?;
    run.bin("A", Some(ACCEPT_ALL))?;
    run.cells(&["c1\tapp\t{}\t1.29.0\t-"])?;
    let climbing = target_dir().join("../cell-matrix-outside-target");
    let climbing = climbing.to_string_lossy();
    // A symlink inside the target that leads out of it (rework 2, C12).
    std::os::unix::fs::symlink("/", run.path().join("escape"))?;
    let escaping = run.path().join("escape/cell-matrix-scratch");
    let escaping = escaping.to_string_lossy();
    for (env, reason) in [
        (("CARGO_TARGET_DIR", ""), "CARGO_TARGET_DIR must name"),
        (("HELM_SCHEMA_SCRATCH_ROOT", &*climbing), "climbs with `..`"),
        (
            ("HELM_SCHEMA_SCRATCH_ROOT", &*escaping),
            "outside the target directory",
        ),
        (
            ("HELM_SCHEMA_SCRATCH_ROOT", "/"),
            "outside the target directory",
        ),
    ] {
        let output = run.run(&[], &[env])?;
        sim_assert_eq!(have: code(&output), want: Some(2), "{reason}: {}", stderr(&output));
        eyre::ensure!(
            stderr(&output).contains(reason),
            "{reason}: {}",
            stderr(&output)
        );
        eyre::ensure!(!run.path().join("A.args").exists(), "{reason}: generated");
    }
    Ok(())
}

/// `--check --scratch <dir>` reproduces the outputs writing nothing under `--out` or the target, so
/// a reviewer can run it from a read-only checkout (rework 2, C15); a scratch under `--out` is refused.
#[test]
fn check_with_a_private_scratch_writes_only_there() -> eyre::Result<()> {
    let mut run = Run::new()?;
    chart(run.path(), "app", APP_VALUES, CONFIGMAP)?;
    run.bin("A", Some(ACCEPT_ALL))?;
    run.cells(&["c1\tapp\t{}\t1.29.0\tA=accept helm_class=pass k8s=valid"])?;
    let output = run.run(&[], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(0), "{}", stderr(&output));
    let listing = |dir: &Path| -> eyre::Result<Vec<(PathBuf, Vec<u8>)>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            let bytes = if path.is_file() {
                fs::read(&path)?
            } else {
                Vec::new()
            };
            files.push((path, bytes));
        }
        files.sort();
        Ok(files)
    };
    let recorded = listing(&run.path().join("out"))?;
    let private = run.path().join("private");
    fs::create_dir(&private)?;
    let unused_target = run.path().join("unused-target");
    let private_arg = private.to_string_lossy().to_string();
    let target_arg = unused_target.to_string_lossy().to_string();
    // An ambient replay store beneath --out is ignored under --scratch (rework 3, C17).
    let store = run
        .path()
        .join("out/replay-store")
        .to_string_lossy()
        .to_string();
    let check = run.run(
        &["--check", "--require-expected", "--scratch", &private_arg],
        &[
            ("CARGO_TARGET_DIR", &target_arg),
            ("SCHEMA_HELM_INVOCATION_CACHE", &store),
        ],
    )?;
    sim_assert_eq!(have: code(&check), want: Some(0), "{}", stderr(&check));
    sim_assert_eq!(have: listing(&run.path().join("out"))?, want: recorded, "--out is untouched");
    eyre::ensure!(!unused_target.exists(), "wrote under the target");
    eyre::ensure!(
        fs::read_dir(&private)?.next().is_some(),
        "the scratch went elsewhere"
    );
    let under_out = run.path().join("out").to_string_lossy().to_string();
    let refused = run.run(&["--check", "--scratch", &under_out], &[])?;
    sim_assert_eq!(have: code(&refused), want: Some(2), "{}", stderr(&refused));
    eyre::ensure!(
        stderr(&refused).contains("lies under --out"),
        "{}",
        stderr(&refused)
    );
    sim_assert_eq!(have: listing(&run.path().join("out"))?, want: recorded, "--out is untouched");
    // A scratch directory is never created by the tool, and the refusal says what to name (rework 3, C18).
    let missing = run.path().join("missing").to_string_lossy().to_string();
    let refused = run.run(&["--check", "--scratch", &missing], &[])?;
    sim_assert_eq!(have: code(&refused), want: Some(2), "{}", stderr(&refused));
    eyre::ensure!(
        stderr(&refused).contains("must be an existing writable directory"),
        "{}",
        stderr(&refused)
    );
    eyre::ensure!(!run.path().join("missing").exists(), "created the scratch");
    Ok(())
}

/// A cell's explicit Kubernetes version reaches Helm even when the chart's `kubeVersion`
/// excludes the battery's versions; a `k8s` verdict at another minor than the validation
/// release's is marked with it, and the header names all three versions.
#[test]
fn explicit_versions_reach_helm_and_foreign_minors_are_marked() -> eyre::Result<()> {
    let mut run = Run::new()?;
    chart(run.path(), "modern", APP_VALUES, CONFIGMAP)?;
    write(
        &run.path().join("modern/Chart.yaml"),
        indoc! {r#"
            apiVersion: v2
            name: modern
            version: 1.0.0
            kubeVersion: ">=1.34.0-0"
        "#},
    )?;
    run.bin("A", Some(ACCEPT_ALL))?;
    run.cells(&[
        "v1\tmodern\t{}\t1.34.0\tA=accept helm_class=pass k8s=valid@1.29",
        "v2\tmodern\t{}\t1.29.0\t-",
    ])?;
    let output = run.run(&[], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(0), "{}", stderr(&output));
    sim_assert_eq!(
        have: run.rows()?,
        want: vec![
            "cell_id\tprep_sha256\tvalues_src\tA\thelm_rc\thelm_class\thelm_abort\tk8s\texpect_ok",
            "v1\t<sha>\thelm\taccept\t0\tpass\t-\tvalid@1.29\tyes",
            "v2\t<sha>\trust\taccept\t1\tunresolved:kube-version-incompatible\tError: chart requires kubeVersion: >=1.34.0-0 which is incompatible with Kubernetes v1.29.0\t-\t-",
        ],
    );
    let versions = run
        .file("matrix.tsv")?
        .lines()
        .find(|line| line.starts_with("# versions:"))
        .ok_or_eyre("no versions header")?
        .to_string();
    sim_assert_eq!(
        have: versions,
        want: "# versions: helm renders each cell at its kube_version; every schema is generated at \
               v1.29.0-standalone-strict; k8s validates against v1.29.0-standalone-strict (a verdict for \
               a cell at another minor is marked @1.29)",
    );
    Ok(())
}

/// A parent chart with one child under the aliases `first` (a condition) and `second` (a tag),
/// a library dependency, and globals read by both.
fn parent_chart(parent: &Path) -> eyre::Result<()> {
    write(
        &parent.join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: parent
            version: 1.0.0
            dependencies:
              - name: child
                version: 1.0.0
                alias: first
                condition: first.enabled
              - name: child
                version: 1.0.0
                alias: second
                tags: [extra]
              - name: lib
                version: 1.0.0
        "},
    )?;
    write(
        &parent.join("values.yaml"),
        indoc! {"
            global:
              team: core
            first:
              enabled: true
              size: 1
            second:
              size: 2
        "},
    )?;
    write(
        &parent.join("templates/main.yaml"),
        r#"{{ include "lib.configmap" (dict "name" "parent" "team" .Values.global.team) }}"#,
    )?;
    write(
        &parent.join("charts/lib/Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: lib
            version: 1.0.0
            type: library
        "},
    )?;
    write(
        &parent.join("charts/lib/templates/_configmap.tpl"),
        indoc! {r#"
            {{- define "lib.configmap" -}}
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: {{ .name }}
            data:
              team: {{ .team | quote }}
            {{- end }}
        "#},
    )?;
    chart(
        &parent.join("charts"),
        "child",
        "size: 0\n",
        indoc! {r"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: {{ .Chart.Name }}
            data:
              size: {{ .Values.size | quote }}
              team: {{ .Values.global.team | quote }}
        "},
    )?;
    Ok(())
}

/// Helm coalesces dependency values: an aliased child twice, a condition and a tag disabling
/// one copy each, a library dependency, and globals reaching both copies.
#[test]
fn dependency_aliases_conditions_tags_libraries_and_globals() -> eyre::Result<()> {
    let mut run = Run::new()?;
    parent_chart(&run.path().join("parent"))?;
    run.bin("A", Some(ACCEPT_ALL))?;
    run.bin(
        "B",
        Some(
            r#"{"type":"object","properties":{"global":{"properties":{"team":{"type":"string"}}},"first":{"properties":{"size":{"type":"integer"},"global":{"properties":{"team":{"type":"string"}}}}},"second":{"properties":{"size":{"type":"integer"}}}}}"#,
        ),
    )?;
    run.cells(&[
        "g1\tparent\t{}\t1.29.0\t-",
        "g2\tparent\t{first: {enabled: false, size: big}}\t1.29.0\t-",
        "g3\tparent\t{tags: {extra: false}, second: {size: big}}\t1.29.0\t-",
        "g4\tparent\t{global: {team: 5}}\t1.29.0\t-",
        "g5\tparent\t{first: {size: big}}\t1.29.0\t-",
        "g6\tparent\t{second: {size: big}}\t1.29.0\t-",
    ])?;
    let output = run.run(&[], &[])?;
    sim_assert_eq!(have: code(&output), want: Some(0), "{}", stderr(&output));
    sim_assert_eq!(
        have: run.rows()?,
        want: vec![
            "cell_id\tprep_sha256\tvalues_src\tA\tB\thelm_rc\thelm_class\thelm_abort\tk8s\texpect_ok",
            "g1\t<sha>\thelm\taccept\taccept\t0\tpass\t-\tvalid\t-",
            "g2\t<sha>\thelm\taccept\treject\t0\tpass\t-\tvalid\t-",
            "g3\t<sha>\thelm\taccept\treject\t0\tpass\t-\tvalid\t-",
            "g4\t<sha>\thelm\taccept\treject\t0\tpass\t-\tvalid\t-",
            "g5\t<sha>\thelm\taccept\treject\t0\tpass\t-\tvalid\t-",
            "g6\t<sha>\thelm\taccept\treject\t0\tpass\t-\tvalid\t-",
        ],
    );
    sim_assert_eq!(
        have: run.file("diagnostics.tsv")?,
        want: "cell_id\tcolumn\tinstance_ptr\tkeyword\tdetail\tschema_ptr\n\
               g2\tB\t/first/size\ttype\t\"big\"\t/properties/first/properties/size/type\n\
               g3\tB\t/second/size\ttype\t\"big\"\t/properties/second/properties/size/type\n\
               g4\tB\t/first/global/team\ttype\t5\t/properties/first/properties/global/properties/team/type\n\
               g4\tB\t/global/team\ttype\t5\t/properties/global/properties/team/type\n\
               g5\tB\t/first/size\ttype\t\"big\"\t/properties/first/properties/size/type\n\
               g6\tB\t/second/size\ttype\t\"big\"\t/properties/second/properties/size/type\n",
    );
    Ok(())
}

//! `helm-schema lint` and `helm-schema template`: the real Helm on a chart
//! copy whose root schema has short `$defs` keys.
//!
//! Needs Helm 4.2.3 on `PATH` (`mise install`).

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use color_eyre::eyre::{self, WrapErr as _};
use serde_json::{Map, Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

const HELM_SCHEMA_BIN: &str = env!("CARGO_BIN_EXE_helm-schema");

/// A fresh copy of the fixture chart under a directory whose name has a
/// space, and an empty `TMPDIR` for the wrapper.
struct Case {
    dir: ScratchDir,
}

impl Case {
    fn new() -> eyre::Result<Self> {
        let version = Command::new("helm")
            .args(["version", "--short"])
            .output()
            .wrap_err("these tests run the real Helm: `mise install` puts Helm 4.2.3 on PATH")?;
        let version = String::from_utf8(version.stdout)?;
        eyre::ensure!(
            version.starts_with("v4.2.3"),
            "these tests need Helm v4.2.3 on PATH, found {version:?}"
        );
        let dir = ScratchDir::new("helm wrapper")?;
        copy_dir(&fixture("chart"), &dir.path().join("wrapped"))?;
        std::fs::create_dir(dir.path().join("tmp"))?;
        Ok(Self { dir })
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    fn chart(&self) -> PathBuf {
        self.path("wrapped")
    }

    /// `helm-schema <command> <chart>`, with `HELM` unset.
    fn wrapper(&self, command: &str) -> Command {
        let mut wrapper = Command::new(HELM_SCHEMA_BIN);
        wrapper
            .args([OsStr::new(command), self.chart().as_os_str()])
            .env("TMPDIR", self.path("tmp"))
            .env_remove("HELM");
        wrapper
    }

    /// `helm <command> <chart>` on the original chart.
    fn helm(&self, command: &str) -> Command {
        let mut helm = Command::new("helm");
        helm.args([OsStr::new(command), self.chart().as_os_str()]);
        helm
    }

    fn edit_schema(&self, edit: impl FnOnce(&mut Value)) -> eyre::Result<()> {
        let path = self.chart().join("values.schema.json");
        let mut schema: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        edit(&mut schema);
        std::fs::write(&path, serde_json::to_vec_pretty(&schema)?)?;
        Ok(())
    }

    /// Every scratch copy is gone.
    fn assert_clean(&self) -> eyre::Result<()> {
        sim_assert_eq!(have: std::fs::read_dir(self.path("tmp"))?.count(), want: 0);
        Ok(())
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/helm-wrapper")
        .join(name)
}

fn copy_dir(from: &Path, to: &Path) -> eyre::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn run(command: &mut Command) -> eyre::Result<Output> {
    command.output().wrap_err_with(|| format!("{command:?}"))
}

fn text(bytes: &[u8]) -> eyre::Result<String> {
    Ok(String::from_utf8(bytes.to_vec())?)
}

#[test]
fn lint_and_template_run_on_a_shortened_copy() -> eyre::Result<()> {
    let case = Case::new()?;
    std::fs::create_dir(case.chart().join("charts"))?;
    let chart_yaml = case.chart().join("Chart.yaml");
    let declared = std::fs::read_to_string(&chart_yaml)?
        + indoc::indoc! {"
            dependencies:
              - name: dep
                version: 0.1.0
        "};
    std::fs::write(&chart_yaml, declared)?;
    let packaged = run(Command::new("helm")
        .arg("package")
        .arg(fixture("dep"))
        .arg("--destination")
        .arg(case.chart().join("charts")))?;
    eyre::ensure!(packaged.status.success(), "{packaged:?}");
    let archive = case.chart().join("charts/dep-0.1.0.tgz");
    let schema = case.chart().join("values.schema.json");
    let before = (std::fs::read(&archive)?, std::fs::read(&schema)?);

    let lint = run(&mut case.wrapper("lint"))?;
    sim_assert_eq!(have: lint.status.code(), want: Some(0));
    assert!(text(&lint.stdout)?.contains("1 chart(s) linted, 0 chart(s) failed"));

    // `.helmignore`, `templates/tests` and the packaged dependency are
    // copied: the rendering matches Helm's on the original chart.
    let template = run(&mut case.wrapper("template"))?;
    let direct = run(&mut case.helm("template"))?;
    sim_assert_eq!(have: template.status.code(), want: Some(0));
    sim_assert_eq!(have: text(&template.stdout)?, want: text(&direct.stdout)?);
    for rendered in [
        "message: \"hello\"",
        "release-name-test",
        "release-name-dep",
    ] {
        assert!(text(&template.stdout)?.contains(rendered), "{rendered}");
    }

    let after = (std::fs::read(&archive)?, std::fs::read(&schema)?);
    assert!(before == after, "the original chart is unchanged");
    case.assert_clean()
}

#[test]
fn rendered_manifests_are_relayed_byte_for_byte() -> eyre::Result<()> {
    let case = Case::new()?;
    // A value that spells a short definition reference is data, not a
    // diagnostic: `template` output keeps it.
    let literal = ["--set-literal", "message=#/$defs/1"];
    for command in ["lint", "template"] {
        let wrapped = run(case.wrapper(command).args(literal))?;
        let direct = run(case.helm(command).args(literal))?;
        sim_assert_eq!(have: wrapped.status.code(), want: Some(0), "{command}: {wrapped:?}");
        if command == "template" {
            assert!(text(&direct.stdout)?.contains("message: \"#/$defs/1\""));
            sim_assert_eq!(have: text(&wrapped.stdout)?, want: text(&direct.stdout)?);
        }
    }
    case.assert_clean()
}

#[test]
fn a_read_only_schema_is_replaced_in_the_copy() -> eyre::Result<()> {
    let case = Case::new()?;
    let schema = case.chart().join("values.schema.json");
    let mut read_only = std::fs::metadata(&schema)?.permissions();
    read_only.set_readonly(true);
    std::fs::set_permissions(&schema, read_only.clone())?;
    let bytes = std::fs::read(&schema)?;

    let lint = run(&mut case.wrapper("lint"))?;
    sim_assert_eq!(have: lint.status.code(), want: Some(0), "{lint:?}");
    sim_assert_eq!(have: std::fs::metadata(&schema)?.permissions(), want: read_only);
    assert!(
        std::fs::read(&schema)? == bytes,
        "the original schema is unchanged"
    );
    case.assert_clean()
}

#[test]
fn bad_values_end_with_helms_status() -> eyre::Result<()> {
    let case = Case::new()?;
    for command in ["lint", "template"] {
        let wrapped = run(case.wrapper(command).args(["--set", "message=5"]))?;
        let direct = run(case.helm(command).args(["--set", "message=5"]))?;
        assert!(!direct.status.success(), "{command}: {direct:?}");
        sim_assert_eq!(have: wrapped.status.code(), want: direct.status.code(), "{command}");
        if command == "template" {
            sim_assert_eq!(have: text(&wrapped.stderr)?, want: text(&direct.stderr)?);
        }
    }
    case.assert_clean()
}

#[test]
fn schema_errors_name_readable_definitions() -> eyre::Result<()> {
    let case = Case::new()?;
    // Helm 4.2.3 reports value errors by instance path; a schema that does
    // not compile is reported by definition.
    case.edit_schema(|schema| {
        schema["$defs"]["values/message"]["pattern"] = json!("^(?=x)");
    })?;
    let shortened = case.path("shortened");
    copy_dir(&case.chart(), &shortened)?;
    let shorten = run(Command::new(HELM_SCHEMA_BIN)
        .arg("shorten")
        .arg(case.chart().join("values.schema.json"))
        .arg(shortened.join("values.schema.json")))?;
    eyre::ensure!(shorten.status.success(), "{shorten:?}");

    for command in ["lint", "template"] {
        let direct = run(Command::new("helm").arg(command).arg(&shortened))?;
        let direct_out = text(&direct.stdout)? + &text(&direct.stderr)?;
        assert!(
            direct_out.contains("#/$defs/1\""),
            "{command}: {direct_out}"
        );

        let wrapped = run(&mut case.wrapper(command))?;
        let wrapped_out = text(&wrapped.stdout)? + &text(&wrapped.stderr)?;
        assert!(
            wrapped_out.contains("#/$defs/values~1message\"") && !wrapped_out.contains("$defs/1\""),
            "{command}: {wrapped_out}"
        );
        sim_assert_eq!(have: wrapped.status.code(), want: direct.status.code(), "{command}");
        if command == "template" {
            sim_assert_eq!(
                have: text(&wrapped.stderr)?,
                want: text(&direct.stderr)?.replace("$defs/1\"", "$defs/values~1message\"")
            );
        }
    }
    case.assert_clean()
}

#[test]
fn oversized_schemas_pass_only_through_the_wrapper() -> eyre::Result<()> {
    let case = Case::new()?;
    case.edit_schema(|schema| {
        let mut definitions = Map::new();
        let mut properties = Map::new();
        for index in 0..800 {
            let name = format!("values/{index}/{}", "x".repeat(4000));
            properties.insert(
                format!("p{index}"),
                json!({ "$ref": format!("#/$defs/{}", name.replace('/', "~1")) }),
            );
            definitions.insert(name, json!({ "type": "string" }));
        }
        schema["$defs"] = Value::Object(definitions);
        schema["properties"] = Value::Object(properties);
    })?;
    let size = std::fs::metadata(case.chart().join("values.schema.json"))?.len();
    assert!(size > 5 * 1024 * 1024, "{size} bytes");

    let direct = run(&mut case.helm("lint"))?;
    assert!(!direct.status.success(), "{direct:?}");
    let wrapped = run(&mut case.wrapper("lint"))?;
    sim_assert_eq!(have: wrapped.status.code(), want: Some(0), "{wrapped:?}");
    case.assert_clean()
}

#[test]
fn charts_without_a_schema_run_as_given() -> eyre::Result<()> {
    let case = Case::new()?;
    std::fs::remove_file(case.chart().join("values.schema.json"))?;
    let lint = run(&mut case.wrapper("lint"))?;
    sim_assert_eq!(have: lint.status.code(), want: Some(0));
    let linting = format!("==> Linting {}", case.chart().display());
    assert!(text(&lint.stdout)?.contains(&linting), "{lint:?}");
    case.assert_clean()
}

#[test]
fn invalid_schema_json_is_refused_before_helm_runs() -> eyre::Result<()> {
    let case = Case::new()?;
    std::fs::write(case.chart().join("values.schema.json"), "{")?;
    let lint = run(&mut case.wrapper("lint"))?;
    assert!(!lint.status.success());
    sim_assert_eq!(have: text(&lint.stdout)?, want: String::new());
    assert!(text(&lint.stderr)?.contains("ParseSchema"), "{lint:?}");
    case.assert_clean()
}

#[test]
fn a_scratch_root_inside_the_chart_is_refused() -> eyre::Result<()> {
    let case = Case::new()?;
    let inside = case.chart().join("tmp");
    std::fs::create_dir(&inside)?;
    let lint = run(case.wrapper("lint").env("TMPDIR", &inside))?;
    assert!(!lint.status.success());
    assert!(
        text(&lint.stderr)?.contains("ScratchInsideChart"),
        "{lint:?}"
    );
    sim_assert_eq!(have: std::fs::read_dir(&inside)?.count(), want: 0);
    Ok(())
}

#[test]
fn helm_arguments_pass_verbatim_from_the_callers_directory() -> eyre::Result<()> {
    let case = Case::new()?;
    std::fs::write(case.path("override values.yaml"), "message: from-values\n")?;
    std::fs::write(case.path("message.txt"), "from-file")?;
    for (args, rendered) in [
        (
            ["--", "-f", "override values.yaml"],
            "message: \"from-values\"",
        ),
        (
            ["--set-file", "message=message.txt", "--"],
            "message: \"from-file\"",
        ),
    ] {
        let template = run(case
            .wrapper("template")
            .args(args)
            .current_dir(case.path("")))?;
        sim_assert_eq!(have: template.status.code(), want: Some(0), "{template:?}");
        assert!(text(&template.stdout)?.contains(rendered), "{template:?}");
    }

    let help = run(case.wrapper("lint").args(["--help"]))?;
    sim_assert_eq!(have: help.status.code(), want: Some(0));
    assert!(text(&help.stdout)?.contains("helm lint PATH"), "{help:?}");

    let unknown = run(case.wrapper("lint").args(["--", "--no-such-flag"]))?;
    let direct = run(case.helm("lint").args(["--no-such-flag"]))?;
    sim_assert_eq!(have: unknown.status.code(), want: direct.status.code());
    sim_assert_eq!(have: text(&unknown.stderr)?, want: text(&direct.stderr)?);
    case.assert_clean()
}

#[cfg(unix)]
mod unix {
    use std::io::{BufRead as _, BufReader};
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    use std::os::unix::process::ExitStatusExt as _;
    use std::path::PathBuf;
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::time::{Duration, Instant};

    use color_eyre::eyre::{self, OptionExt as _};
    use test_util::prelude::sim_assert_eq;

    use super::{Case, run, text};

    fn fake_helm(case: &Case, name: &str, script: &str) -> eyre::Result<PathBuf> {
        let path = case.path(name);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n"))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        Ok(path)
    }

    #[test]
    fn helm_is_found_by_flag_then_environment_then_path() -> eyre::Result<()> {
        let case = Case::new()?;
        let flag = fake_helm(&case, "flag-helm", "echo flag; exit 42")?;
        let env = fake_helm(&case, "env-helm", "echo env; exit 43")?;

        let by_flag = run(Command::new(super::HELM_SCHEMA_BIN)
            .arg("lint")
            .arg("--helm")
            .arg(&flag)
            .arg(case.chart())
            .env("TMPDIR", case.path("tmp"))
            .env("HELM", &env))?;
        sim_assert_eq!(have: (by_flag.status.code(), text(&by_flag.stdout)?), want: (Some(42), "flag\n".to_string()));

        let by_env = run(case.wrapper("lint").env("HELM", &env))?;
        sim_assert_eq!(have: (by_env.status.code(), text(&by_env.stdout)?), want: (Some(43), "env\n".to_string()));

        let missing = run(case.wrapper("lint").env("HELM", case.path("missing")))?;
        assert!(!missing.status.success());
        assert!(text(&missing.stderr)?.contains("Spawn"), "{missing:?}");
        case.assert_clean()
    }

    #[test]
    fn busy_streams_relay_byte_exact() -> eyre::Result<()> {
        let case = Case::new()?;
        let helm = fake_helm(
            &case,
            "busy-helm",
            indoc::indoc! {r#"
                i=0
                while [ $i -lt 10000 ]; do
                  echo "out $i #/\$defs/1"
                  echo "err $i #/\$defs/1" >&2
                  i=$((i+1))
                done
                printf 'crlf #/\044defs/1\r\n\377 last #/\044defs/1'
                exit 7
            "#},
        )?;
        let output = run(case.wrapper("lint").env("HELM", helm))?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        for index in 0..10000 {
            stdout.extend(format!("out {index} #/$defs/values~1message\n").bytes());
            stderr.extend(format!("err {index} #/$defs/values~1message\n").bytes());
        }
        stdout.extend(b"crlf #/$defs/values~1message\r\n\xff last #/$defs/values~1message");
        sim_assert_eq!(have: output.status.code(), want: Some(7));
        assert!(output.stdout == stdout, "stdout differs");
        assert!(output.stderr == stderr, "stderr differs");
        case.assert_clean()
    }

    #[test]
    fn a_signal_stops_helm_removes_the_copy_and_ends_the_wrapper() -> eyre::Result<()> {
        let case = Case::new()?;
        let helm = fake_helm(&case, "slow-helm", "echo started; exec sleep 60")?;
        let mut wrapper = case
            .wrapper("lint")
            .env("HELM", helm)
            .stdout(Stdio::piped())
            .spawn()?;
        let stdout = wrapper.stdout.take().ok_or_eyre("stdout is piped")?;
        let mut started = String::new();
        BufReader::new(stdout).read_line(&mut started)?;
        sim_assert_eq!(have: started, want: "started\n".to_string());

        let kill = run(Command::new("kill").args(["-TERM", &wrapper.id().to_string()]))?;
        eyre::ensure!(kill.status.success(), "{kill:?}");
        sim_assert_eq!(have: wrapper.wait()?.signal(), want: Some(15));
        case.assert_clean()
    }

    #[test]
    fn a_signal_ends_the_wrapper_while_its_output_is_blocked() -> eyre::Result<()> {
        let case = Case::new()?;
        let big = case.path("big.yaml");
        std::fs::write(&big, format!("message: {}\n", "x".repeat(1_000_000)))?;
        let mut wrapper = case
            .wrapper("template")
            .arg("-f")
            .arg(&big)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        // Nothing reads stdout, so relaying blocks once the pipe is full.
        std::thread::sleep(Duration::from_secs(2));
        let kill = run(Command::new("kill").args(["-TERM", &wrapper.id().to_string()]))?;
        eyre::ensure!(kill.status.success(), "{kill:?}");
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = wrapper.try_wait()? {
                break Some(status);
            }
            if Instant::now() > deadline {
                wrapper.kill()?;
                wrapper.wait()?;
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        sim_assert_eq!(have: status.and_then(|status| status.signal()), want: Some(15));
        case.assert_clean()
    }

    #[test]
    fn a_signal_ends_the_wrapper_after_helm_closes_its_streams() -> eyre::Result<()> {
        let case = Case::new()?;
        let helm = fake_helm(
            &case,
            "silent-helm",
            "echo closing; exec >&- 2>&-; sleep 60",
        )?;
        let mut wrapper = case
            .wrapper("lint")
            .env("HELM", helm)
            .stdout(Stdio::piped())
            .spawn()?;
        let mut stdout = BufReader::new(wrapper.stdout.take().ok_or_eyre("stdout is piped")?);
        stdout.read_line(&mut String::new())?;
        // Helm has closed both streams but keeps running.
        std::thread::sleep(Duration::from_millis(500));
        run(Command::new("kill").args(["-TERM", &wrapper.id().to_string()]))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = wrapper.try_wait()? {
                break Some(status);
            }
            if Instant::now() > deadline {
                wrapper.kill()?;
                wrapper.wait()?;
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        sim_assert_eq!(have: status.and_then(|status| status.signal()), want: Some(15));
        case.assert_clean()
    }

    #[test]
    fn helm_status_survives_a_fast_exit_and_a_failed_cleanup() -> eyre::Result<()> {
        let case = Case::new()?;
        // Helm ends at once; a signal right after it races its exit.
        let fast = fake_helm(&case, "fast-helm", "echo started; exit 5")?;
        let mut wrapper = case
            .wrapper("lint")
            .env("HELM", fast)
            .stdout(Stdio::piped())
            .spawn()?;
        let stdout = wrapper.stdout.take().ok_or_eyre("stdout is piped")?;
        BufReader::new(stdout).read_line(&mut String::new())?;
        run(Command::new("kill").args(["-TERM", &wrapper.id().to_string()]))?;
        let status = wrapper.wait()?;
        assert!(
            status.signal() == Some(15) || status.code() == Some(5),
            "{status:?}"
        );
        case.assert_clean()?;

        // Helm leaves a directory in the copy that cannot be emptied.
        let locking = fake_helm(&case, "locking-helm", r#"chmod 555 "$2/templates"; exit 3"#)?;
        let locked = run(case.wrapper("lint").env("HELM", &locking))?;
        unlock_scratch(&case)?;
        sim_assert_eq!(have: locked.status.code(), want: Some(3));
        assert!(
            text(&locked.stderr)?.contains("failed to remove the scratch directory"),
            "{locked:?}"
        );

        // The report is best effort: a closed stderr does not replace the status.
        let mut closed = case
            .wrapper("lint")
            .env("HELM", &locking)
            .stderr(Stdio::piped())
            .spawn()?;
        drop(closed.stderr.take());
        let closed = closed.wait()?;
        unlock_scratch(&case)?;
        sim_assert_eq!(have: closed.code(), want: Some(3));

        // Nor does a signal wait for a stderr no one reads.
        let flooding = fake_helm(
            &case,
            "flooding-helm",
            indoc::indoc! {r#"
                chmod 555 "$2/templates"
                i=0
                while [ $i -lt 100000 ]; do echo "flooding stderr $i" >&2; i=$((i+1)); done
                sleep 60
            "#},
        )?;
        let flooded = case
            .wrapper("lint")
            .env("HELM", flooding)
            .stderr(Stdio::piped())
            .spawn()?;
        std::thread::sleep(Duration::from_secs(2));
        let status = terminate(flooded)?;
        unlock_scratch(&case)?;
        sim_assert_eq!(have: status.and_then(|status| status.signal()), want: Some(15));
        Ok(())
    }

    /// Makes the copies a locking Helm left behind removable again.
    fn unlock_scratch(case: &Case) -> eyre::Result<()> {
        for entry in std::fs::read_dir(case.path("tmp"))? {
            let templates = entry?.path().join("wrapped/templates");
            std::fs::set_permissions(&templates, std::fs::Permissions::from_mode(0o755))?;
        }
        Ok(())
    }

    /// Sends SIGTERM to `wrapper`; its status if it ends within 10 seconds.
    fn terminate(mut wrapper: Child) -> eyre::Result<Option<ExitStatus>> {
        run(Command::new("kill").args(["-TERM", &wrapper.id().to_string()]))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = wrapper.try_wait()? {
                return Ok(Some(status));
            }
            if Instant::now() > deadline {
                wrapper.kill()?;
                wrapper.wait()?;
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn links_are_copied_as_their_targets_and_cycles_are_refused() -> eyre::Result<()> {
        let case = Case::new()?;
        let outside = case.path("outside.yaml");
        std::fs::write(
            &outside,
            indoc::indoc! {"
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: linked
            "},
        )?;
        let link = case.chart().join("templates/linked.yaml");
        symlink(&outside, &link)?;
        let template = run(&mut case.wrapper("template"))?;
        sim_assert_eq!(have: template.status.code(), want: Some(0), "{template:?}");
        assert!(text(&template.stdout)?.contains("name: linked"));
        assert!(std::fs::symlink_metadata(&link)?.file_type().is_symlink());

        symlink("..", case.chart().join("templates/loop"))?;
        let cycle = run(&mut case.wrapper("template"))?;
        assert!(!cycle.status.success());
        assert!(text(&cycle.stderr)?.contains("LinkCycle"), "{cycle:?}");
        std::fs::remove_file(case.chart().join("templates/loop"))?;

        symlink("missing.yaml", case.chart().join("templates/dangling.yaml"))?;
        let dangling = run(&mut case.wrapper("template"))?;
        assert!(!dangling.status.success());
        assert!(text(&dangling.stderr)?.contains("Copy"), "{dangling:?}");
        case.assert_clean()
    }
}

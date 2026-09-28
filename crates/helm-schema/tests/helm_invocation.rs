//! Checks the exact Helm invocation key, its store, and replay.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use color_eyre::eyre;
use flate2::Compression;
use flate2::write::GzEncoder;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

#[path = "common/helm_cache_policy.rs"]
mod helm_cache_policy;

#[path = "common/helm_invocation.rs"]
mod helm_invocation;

#[path = "common/helm_pool.rs"]
mod helm_pool;

use helm_cache_policy::render_cacheability;
use helm_invocation::{
    Cacheability, HelmRunner, InvocationRequest, Outcome, PreparedTree, RENDER_TIMEOUT,
    TemplateRequest, find_helm, find_helmsweep, stage_totals, tree_sha256,
};
use helm_pool::{Ordinal, PoolLimits, run_ordered};

fn base_request() -> InvocationRequest {
    InvocationRequest {
        format: "helm-schema/helm-invocation/v3".to_string(),
        platform: "macos-aarch64".to_string(),
        engine: "helmsweep".to_string(),
        program_sha256: "helmsweep".to_string(),
        program_version: indoc! {"
            build b
            helm.sh/helm/v4 v4.2.3 => ./third_party/helm-v4.2.3
        "}
        .to_string(),
        working_directory: "/store".to_string(),
        environment: vec![("HOME".to_string(), "/store/home".to_string())],
        arguments: [
            "template",
            "adjudication",
            "/store/trees/chart",
            "--kube-version",
            "1.29.0",
            "--skip-schema-validation",
            "-f",
            "/store/inputs/values.json",
        ]
        .map(str::to_string)
        .to_vec(),
        inputs: vec![
            ("/store/trees/chart".to_string(), "chart".to_string()),
            (
                "/store/inputs/values.json".to_string(),
                "values".to_string(),
            ),
            ("/store/home".to_string(), "home".to_string()),
        ],
        client_only: true,
    }
}

/// Every identity field of a request decides its key on its own.
#[test]
fn each_identity_field_changes_the_key() -> eyre::Result<()> {
    let base = base_request();
    let mut variants = vec![base.clone()];
    let mut edit = |change: &dyn Fn(&mut InvocationRequest)| {
        let mut variant = base.clone();
        change(&mut variant);
        variants.push(variant);
    };
    edit(&|request| request.format = "helm-schema/helm-invocation/v2".to_string());
    edit(&|request| request.platform = "linux-x86_64".to_string());
    // The engine, its program's bytes, and any line of its version (build id, patch).
    edit(&|request| request.engine = "helm-cli".to_string());
    edit(&|request| request.program_sha256 = "other helmsweep".to_string());
    edit(&|request| {
        request.program_version = indoc! {"
            build c
            helm.sh/helm/v4 v4.2.3 => ./third_party/helm-v4.2.3
        "}
        .to_string();
    });
    edit(&|request| request.working_directory = "/other".to_string());
    edit(&|request| {
        request
            .environment
            .push(("TZ".to_string(), "UTC".to_string()));
    });
    edit(&|request| request.environment[0].1 = "/other/home".to_string());
    // Release name, Kubernetes version, flags and their order.
    edit(&|request| request.arguments[1] = "other".to_string());
    edit(&|request| request.arguments[4] = "1.33.0".to_string());
    edit(&|request| request.arguments.push("--namespace=other".to_string()));
    edit(&|request| request.arguments.swap(3, 5));
    edit(&|request| request.arguments[0] = "lint".to_string());
    // The same paths naming different chart or values content.
    edit(&|request| request.inputs[0].1 = "other chart".to_string());
    edit(&|request| request.inputs[1].1 = "other values".to_string());
    // Different content in Helm's home, and a template that may reach a cluster.
    edit(&|request| request.inputs[2].1 = "other home".to_string());
    edit(&|request| request.client_only = false);
    let mut keys = BTreeSet::new();
    for variant in &variants {
        keys.insert(variant.key()?);
    }
    sim_assert_eq!(have: keys.len(), want: variants.len());
    Ok(())
}

/// A packaged dependency's bytes are chart content: changing only a nested
/// archive changes the prepared tree's identity.
#[test]
fn nested_archive_bytes_decide_the_tree_identity() -> eyre::Result<()> {
    let mut identities = BTreeSet::new();
    for value in ["a", "b"] {
        let root = ScratchDir::new("helm_invocation")?;
        write_chart(root.path(), "{}")?;
        let child = ScratchDir::new("helm_invocation")?;
        write_chart(child.path(), &format!("nested: {value}\n"))?;
        let archive = fs::File::create(root.path().join("charts.tgz"))?;
        let mut builder = tar::Builder::new(GzEncoder::new(archive, Compression::default()));
        builder.append_dir_all("child", child.path())?;
        builder.into_inner()?.finish()?;
        identities.insert(tree_sha256(root.path())?);
    }
    sim_assert_eq!(have: identities.len(), want: 2);
    Ok(())
}

fn write_chart(root: &Path, values: &str) -> eyre::Result<()> {
    fs::create_dir_all(root.join("templates"))?;
    fs::write(
        root.join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: replayed
            version: 1.0.0
        "},
    )?;
    fs::write(root.join("values.yaml"), values)?;
    fs::write(
        root.join("templates/configmap.yaml"),
        indoc! {r#"
        {{- if eq .Values.value "fail" }}{{ fail "requested abort" }}{{ end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: replayed
        data:
          value: {{ .Values.value | quote }}
          kubernetes: {{ .Capabilities.KubeVersion.Version | quote }}
    "#},
    )?;
    Ok(())
}

fn publish_chart(runner: &HelmRunner, values: &str) -> eyre::Result<PreparedTree> {
    let staged = runner.staging_dir()?;
    write_chart(&staged, values)?;
    runner.publish_tree(&staged)
}

struct Observed {
    outcome: Outcome,
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    key: String,
}

fn render(
    runner: &HelmRunner,
    chart: &PreparedTree,
    values: &str,
    kubernetes_version: &str,
    stage: &str,
    cacheability: &Cacheability,
) -> eyre::Result<Observed> {
    let case = ScratchDir::new("helm_invocation")?;
    let execution = runner.template(
        &TemplateRequest {
            chart,
            values: values.as_bytes(),
            kubernetes_version,
        },
        case.path(),
        stage,
        cacheability,
    )?;
    Ok(Observed {
        outcome: execution.record.outcome.clone(),
        success: execution.success(),
        stdout: execution.stdout,
        stderr: execution.stderr,
        key: execution.record.key,
    })
}

/// An identical invocation replays its stored outputs, labels do not take
/// part in the key, and any changed input executes again.
#[test]
fn identical_invocations_replay_and_changed_inputs_execute() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_invocation")?;
    let runner = HelmRunner::new(root.path(), true)?;
    let chart = publish_chart(&runner, "value: default\n")?;
    let cacheable = Cacheability::Cacheable;
    let first = render(
        &runner,
        &chart,
        r#"{"value": "a"}"#,
        "1.29.0",
        "render",
        &cacheable,
    )?;
    sim_assert_eq!(have: first.outcome, want: Outcome::Executed);
    let again = render(
        &runner,
        &chart,
        r#"{"value": "a"}"#,
        "1.29.0",
        "other label",
        &cacheable,
    )?;
    sim_assert_eq!(have: again.outcome, want: Outcome::Replayed);
    sim_assert_eq!(have: again.stdout, want: first.stdout);
    sim_assert_eq!(have: again.key, want: first.key);

    let changed_values = render(
        &runner,
        &chart,
        r#"{"value": "b"}"#,
        "1.29.0",
        "render",
        &cacheable,
    )?;
    let changed_version = render(
        &runner,
        &chart,
        r#"{"value": "a"}"#,
        "1.33.0",
        "render",
        &cacheable,
    )?;
    let other_chart = publish_chart(&runner, "value: other\n")?;
    let changed_chart = render(
        &runner,
        &other_chart,
        r#"{"value": "a"}"#,
        "1.29.0",
        "render",
        &cacheable,
    )?;
    for changed in [&changed_values, &changed_version, &changed_chart] {
        sim_assert_eq!(have: &changed.outcome, want: &Outcome::Executed);
    }
    eyre::ensure!(
        String::from_utf8_lossy(&changed_version.stdout).contains("v1.33.0"),
        "the Kubernetes version reaches the render"
    );

    Ok(())
}

/// An ordinary Helm failure is a completed execution and replays as one; a
/// bypassed chart always executes; a runner without replay never replays.
#[test]
fn failures_replay_and_bypassed_or_private_invocations_execute() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_invocation")?;
    let runner = HelmRunner::new(root.path(), true)?;
    let chart = publish_chart(&runner, "value: default\n")?;
    let cacheable = Cacheability::Cacheable;
    let failed = render(
        &runner,
        &chart,
        r#"{"value": "fail"}"#,
        "1.29.0",
        "render",
        &cacheable,
    )?;
    let failed_again = render(
        &runner,
        &chart,
        r#"{"value": "fail"}"#,
        "1.29.0",
        "render",
        &cacheable,
    )?;
    sim_assert_eq!(have: (failed.success, failed_again.success), want: (false, false));
    sim_assert_eq!(have: failed_again.stderr, want: failed.stderr.clone());
    eyre::ensure!(
        String::from_utf8_lossy(&failed.stderr).contains("requested abort"),
        "the failure diagnostic is kept"
    );
    sim_assert_eq!(have: failed_again.outcome, want: Outcome::Replayed);

    let bypass = Cacheability::Bypass("calls now".to_string());
    for _ in 0..2 {
        let bypassed = render(
            &runner,
            &chart,
            r#"{"value": "a"}"#,
            "1.29.0",
            "render",
            &bypass,
        )?;
        sim_assert_eq!(have: bypassed.outcome, want: Outcome::Bypassed("calls now".to_string()));
    }
    let private = HelmRunner::new(root.path(), false)?;
    let unreplayed = render(
        &private,
        &chart,
        r#"{"value": "a"}"#,
        "1.29.0",
        "render",
        &cacheable,
    )?;
    sim_assert_eq!(have: unreplayed.outcome, want: Outcome::Executed);
    Ok(())
}

/// A writer killed before publication leaves only staging debris, and a
/// truncated or incomplete entry is a miss: never a partial replay.
#[test]
fn killed_writers_and_corrupt_entries_are_misses() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_invocation")?;
    let runner = HelmRunner::new(root.path(), true)?;
    let chart = publish_chart(&runner, "value: default\n")?;
    let cacheable = Cacheability::Cacheable;
    let values = r#"{"value": "a"}"#;
    let first = render(&runner, &chart, values, "1.29.0", "render", &cacheable)?;
    let entry = root
        .path()
        .canonicalize()?
        .join(&first.key[..2])
        .join(&first.key);
    let complete = entry.with_extension("complete");
    fs::rename(&entry, &complete)?;

    // A staging directory beside the entry is invisible to readers.
    let staging = entry.with_file_name(format!(".staging-{}", first.key));
    fs::create_dir(&staging)?;
    fs::copy(complete.join("request.json"), staging.join("request.json"))?;
    fs::write(
        staging.join("stdout"),
        &first.stdout[..first.stdout.len() / 2],
    )?;
    let after_kill = render(&runner, &chart, values, "1.29.0", "render", &cacheable)?;
    sim_assert_eq!(have: after_kill.outcome, want: Outcome::Executed);
    sim_assert_eq!(have: after_kill.stdout, want: first.stdout.clone());

    let corruptions: [(&str, Corruption<'_>); 5] = [
        ("truncated stdout", &|entry| {
            fs::write(entry.join("stdout"), b"apiVersion: v1\n")
        }),
        ("missing result", &|entry| {
            fs::remove_file(entry.join("result.json"))
        }),
        ("different request", &|entry| {
            fs::write(entry.join("request.json"), b"{}")
        }),
        // Valid metadata that flips the stored exit status.
        ("changed result", &|entry| {
            let result = fs::read_to_string(entry.join("result.json"))?;
            fs::write(
                entry.join("result.json"),
                result.replace("\"exit_code\": 0", "\"exit_code\": 1"),
            )
        }),
        ("missing manifest", &|entry| {
            fs::remove_file(entry.join("manifest.json"))
        }),
    ];
    for (name, corrupt) in corruptions {
        if entry.exists() {
            fs::remove_dir_all(&entry)?;
        }
        copy_dir(&complete, &entry)?;
        corrupt(&entry)?;
        let observed = render(&runner, &chart, values, "1.29.0", "render", &cacheable)?;
        sim_assert_eq!(have: (name, observed.outcome), want: (name, Outcome::Executed));
        sim_assert_eq!(have: observed.stdout, want: first.stdout.clone());
        // The damaged entry was moved aside and the fresh execution replaces it.
        let repaired = render(&runner, &chart, values, "1.29.0", "render", &cacheable)?;
        sim_assert_eq!(have: (name, repaired.outcome), want: (name, Outcome::Replayed));
        sim_assert_eq!(have: repaired.success, want: first.success);
    }
    fs::remove_dir_all(&entry)?;
    copy_dir(&complete, &entry)?;
    let intact = render(&runner, &chart, values, "1.29.0", "render", &cacheable)?;
    sim_assert_eq!(have: intact.outcome, want: Outcome::Replayed);
    Ok(())
}

/// Damages a copied store entry.
type Corruption<'a> = &'a dyn Fn(&Path) -> std::io::Result<()>;

fn copy_dir(from: &Path, to: &Path) -> eyre::Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        fs::copy(entry.path(), to.join(entry.file_name()))?;
    }
    Ok(())
}

/// A replay reports the original execution's cost and is counted apart
/// from executions.
#[test]
fn replays_are_accounted_apart_from_executions() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_invocation")?;
    let runner = HelmRunner::new(root.path(), true)?;
    let chart = publish_chart(&runner, "value: default\n")?;
    let mut records = Vec::new();
    for _ in 0..2 {
        let case = ScratchDir::new("helm_invocation")?;
        let execution = runner.template(
            &TemplateRequest {
                chart: &chart,
                values: br#"{"value": "a"}"#,
                kubernetes_version: "1.29.0",
            },
            case.path(),
            "render",
            &Cacheability::Cacheable,
        )?;
        records.push(execution.record);
    }
    let totals = stage_totals(&records);
    let render = totals
        .get("render")
        .ok_or_else(|| eyre::eyre!("no render totals"))?;
    sim_assert_eq!(have: (render.executed, render.replayed), want: (1, 1));
    sim_assert_eq!(have: records[1].elapsed_ms, want: records[0].elapsed_ms);
    sim_assert_eq!(have: records[1].max_rss_bytes, want: records[0].max_rss_bytes);
    sim_assert_eq!(have: render.executed_ms, want: records[0].elapsed_ms);
    Ok(())
}

/// A chart at `root` whose templates are `templates`, with a packaged
/// dependency whose templates are `dependency_templates`.
fn policy_chart(
    root: &Path,
    templates: &[(&str, &str)],
    dependency_templates: &[(&str, &str)],
) -> eyre::Result<()> {
    write_chart(root, "value: now\n")?;
    fs::write(root.join("README.md"), "Set now to a random value.\n")?;
    for (name, source) in templates {
        fs::write(root.join("templates").join(name), source)?;
    }
    let child = ScratchDir::new("helm_invocation")?;
    write_chart(child.path(), "{}")?;
    for (name, source) in dependency_templates {
        fs::write(child.path().join("templates").join(name), source)?;
    }
    fs::create_dir_all(root.join("charts"))?;
    let archive = fs::File::create(root.join("charts/child.tgz"))?;
    let mut builder = tar::Builder::new(GzEncoder::new(archive, Compression::default()));
    builder.append_dir_all("child", child.path())?;
    builder.into_inner()?.finish()?;
    Ok(())
}

fn cacheability_of(
    templates: &[(&str, &str)],
    dependency_templates: &[(&str, &str)],
) -> eyre::Result<Cacheability> {
    let root = ScratchDir::new("helm_invocation")?;
    policy_chart(root.path(), templates, dependency_templates)?;
    render_cacheability(root.path())
}

/// Clocks, randomness, generated keys, unordered map iteration and `tpl`
/// anywhere a render executes them keep a chart's renders from being
/// replayed, in packaged dependencies too; `lookup` alone makes a chart
/// replayable only as a client-only template. Text Helm never executes as a
/// template does not count.
#[test]
fn nondeterministic_template_calls_bypass_render_replay() -> eyre::Result<()> {
    let deterministic = [
        (
            "_helpers.tpl",
            "{{- define \"name\" -}}{{ .Chart.Name | trunc 63 }}{{- end -}}",
        ),
        (
            "static.yaml",
            indoc! {r#"
                kind: {{ "{{ now }}" }}
                {{/* now */}}"#},
        ),
    ];
    sim_assert_eq!(
        have: cacheability_of(&deterministic, &[("config.yaml", "a: {{ include \"name\" . }}")])?,
        want: Cacheability::Cacheable,
    );
    sim_assert_eq!(
        have: cacheability_of(
            &[("secret.yaml", "{{ $s := lookup \"v1\" \"Secret\" \"ns\" \"n\" }}")],
            &[],
        )?,
        want: Cacheability::ClientOnly,
    );
    for (templates, dependency, reason) in [
        (
            vec![("time.yaml", "at: {{ now }}")],
            vec![],
            "templates/time.yaml: calls now",
        ),
        (
            vec![],
            vec![(
                "_helpers.tpl",
                "{{- define \"pw\" -}}{{ randAlphaNum 10 }}{{- end -}}",
            )],
            "charts/child.tgz:child/templates/_helpers.tpl: calls randAlphaNum",
        ),
        (
            vec![("cert.yaml", "{{ $ca := genCA \"ca\" 365 }}")],
            vec![],
            "templates/cert.yaml: calls genCA",
        ),
        (
            vec![("dynamic.yaml", "a: {{ tpl .Values.value . }}")],
            vec![],
            "templates/dynamic.yaml: calls tpl",
        ),
        // An action assembled from strings that hold no action themselves.
        (
            vec![(
                "assembled.yaml",
                "a: {{ tpl (printf \"%s%s\" \"{{ randAl\" \"phaNum 6 }}\") . }}",
            )],
            vec![],
            "templates/assembled.yaml: calls tpl",
        ),
        (
            vec![("order.yaml", "a: {{ keys .Values | join \",\" }}")],
            vec![],
            "templates/order.yaml: calls keys",
        ),
        (
            vec![
                (
                    "secret.yaml",
                    "{{ $s := lookup \"v1\" \"Secret\" \"ns\" \"n\" }}",
                ),
                ("order.yaml", "a: {{ values .Values | toJson }}"),
            ],
            vec![],
            "templates/order.yaml: calls values (and 1 more)",
        ),
    ] {
        sim_assert_eq!(
            have: cacheability_of(&templates, &dependency)?,
            want: Cacheability::Bypass(reason.to_string()),
        );
    }
    Ok(())
}

/// A Helm stand-in that reports the pinned version and otherwise ends as
/// `body` makes it.
#[cfg(unix)]
fn fake_helm(directory: &Path, body: &str) -> eyre::Result<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;
    let program = directory.join(format!("helm-{}", body.len()));
    fs::write(
        &program,
        format!("#!/bin/sh\nif [ \"$1\" = version ]; then printf v4.2.3; exit 0; fi\n{body}\n"),
    )?;
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755))?;
    Ok(program)
}

/// A child a signal ends, one exiting with a status Helm never uses, and
/// one that cannot be started are harness failures, never Helm verdicts,
/// and none is stored.
#[cfg(unix)]
#[test]
fn abnormal_executions_are_harness_failures() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_invocation")?;
    let programs = ScratchDir::new("helm_invocation")?;
    let real = HelmRunner::new(root.path(), true)?;
    let chart = publish_chart(&real, "value: default\n")?;
    for (body, expected) in [
        ("kill -KILL $$", "ended abnormally"),
        ("exit 2", "ended abnormally"),
        ("exit 0", "start Helm"),
    ] {
        let program = fake_helm(programs.path(), body)?;
        let runner = HelmRunner::with_program(root.path(), true, program.clone())?;
        if body == "exit 0" {
            fs::remove_file(&program)?;
        }
        let case = ScratchDir::new("helm_invocation")?;
        let request = TemplateRequest {
            chart: &chart,
            values: br#"{"value": "a"}"#,
            kubernetes_version: "1.29.0",
        };
        for _ in 0..2 {
            let Err(error) =
                runner.template(&request, case.path(), "render", &Cacheability::Cacheable)
            else {
                eyre::bail!("{body}: an abnormal execution became a Helm verdict");
            };
            eyre::ensure!(
                format!("{error:?}").contains(expected),
                "{body}: unexpected failure {error:?}"
            );
            // A missing program fails its check before any case evidence exists.
            if body != "exit 0" {
                check_preserved_bundle(case.path(), &format!("{error:?}"), request.values)?;
            }
        }
    }
    Ok(())
}

/// `error` names the preserved bundle of `case`, and the bundle alone holds
/// the render stage's inputs: its chart copy, its values and its Kubernetes
/// version, named by bundle-relative paths.
#[cfg(unix)]
fn check_preserved_bundle(case: &Path, error: &str, values: &[u8]) -> eyre::Result<()> {
    let relative = case
        .canonicalize()?
        .strip_prefix(test_util::scratch::root().canonicalize()?)?
        .to_path_buf();
    let bundle = test_util::scratch::evidence_root().join(relative);
    eyre::ensure!(
        error.contains(&format!("evidence={}", bundle.display())),
        "the failure does not report the preserved bundle {}: {error}",
        bundle.display()
    );
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(bundle.join("render.invocation.json"))?)?;
    let field = |name: &str| record.get(name).and_then(serde_json::Value::as_str);
    sim_assert_eq!(
        have: (
            field("chart"),
            field("values"),
            field("kubernetes_version"),
            bundle.join("charts/render/Chart.yaml").is_file(),
            fs::read(bundle.join("inputs/render.values.json"))?,
        ),
        want: (
            Some("charts/render"),
            Some("inputs/render.values.json"),
            Some("1.29.0"),
            true,
            values.to_vec(),
        )
    );
    Ok(())
}

/// Results come back in ordinal order whatever order the jobs complete in,
/// submitted jobs included, and the worker and memory bounds hold.
#[test]
fn pool_results_follow_ordinals_not_completion_order() -> eyre::Result<()> {
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    #[derive(Debug)]
    enum Job {
        Chart(usize),
        Probe(usize, usize, u64),
    }
    let limits = PoolLimits {
        workers: 4,
        memory_bytes: 100,
    };
    let mut observed = Vec::new();
    for delays in [
        [40_u64, 30, 20, 10, 0],
        [0, 10, 20, 30, 40],
        [25, 5, 40, 0, 15],
    ] {
        let running = AtomicUsize::new(0);
        let max_running = AtomicUsize::new(0);
        let reserved = AtomicU64::new(0);
        let over_budget = AtomicUsize::new(0);
        let jobs = (0..5)
            .map(|chart| ((chart, 0), Job::Chart(chart)))
            .collect();
        let reservation = |job: &Job| match job {
            Job::Chart(_) => 0,
            // One job reserves more than the whole budget and must run alone.
            Job::Probe(chart, probe, _) if *chart == 2 && *probe == 1 => 500,
            Job::Probe(..) => 30,
        };
        let (results, peaks) = run_ordered(
            limits,
            jobs,
            reservation,
            |ordinal: Ordinal, job, submitter| {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                max_running.fetch_max(now, Ordering::SeqCst);
                let cost = reservation(&job);
                let held = reserved.fetch_add(cost, Ordering::SeqCst) + cost;
                if held > limits.memory_bytes && cost <= limits.memory_bytes {
                    over_budget.fetch_add(1, Ordering::SeqCst);
                }
                let outcome = match job {
                    Job::Chart(chart) => {
                        for probe in 1..=3 {
                            let delay = delays[(chart + probe) % delays.len()];
                            submitter.submit((chart, probe), Job::Probe(chart, probe, delay));
                        }
                        std::thread::sleep(std::time::Duration::from_millis(delays[chart]));
                        format!("chart {chart}")
                    }
                    Job::Probe(chart, probe, delay) => {
                        std::thread::sleep(std::time::Duration::from_millis(delay));
                        format!("probe {chart}/{probe}")
                    }
                };
                reserved.fetch_sub(cost, Ordering::SeqCst);
                running.fetch_sub(1, Ordering::SeqCst);
                (ordinal, outcome)
            },
        );
        eyre::ensure!(
            max_running.load(Ordering::SeqCst) <= limits.workers,
            "worker bound"
        );
        sim_assert_eq!(have: over_budget.load(Ordering::SeqCst), want: 0);
        eyre::ensure!(peaks.running <= limits.workers, "reported worker bound");
        for (ordinal, (seen, _)) in &results {
            sim_assert_eq!(have: ordinal, want: seen);
        }
        observed.push(
            results
                .into_iter()
                .map(|(_, (_, outcome))| outcome)
                .collect::<Vec<_>>(),
        );
    }
    let mut expected = Vec::new();
    for chart in 0..5 {
        expected.push(format!("chart {chart}"));
        for probe in 1..=3 {
            expected.push(format!("probe {chart}/{probe}"));
        }
    }
    for outcomes in observed {
        sim_assert_eq!(have: outcomes, want: expected.clone());
    }
    Ok(())
}

/// The process's runner checks the pinned Helm release once and is shared.
#[test]
fn the_shared_runner_is_made_once() -> eyre::Result<()> {
    let first = HelmRunner::shared()?;
    let second = HelmRunner::shared()?;
    assert!(std::ptr::eq(first, second));
    let limits = PoolLimits::from_env()?;
    let cores = std::thread::available_parallelism().map_or(1, usize::from);
    eyre::ensure!(
        limits.workers >= 1 && limits.workers <= cores.saturating_sub(2).max(1),
        "worker count {} leaves two cores free",
        limits.workers
    );
    Ok(())
}

/// A Helm executable replaced after its bytes were hashed fails every later
/// invocation instead of running under the old identity.
#[cfg(unix)]
#[test]
fn a_changed_helm_executable_is_a_harness_failure() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_invocation")?;
    let programs = ScratchDir::new("helm_invocation")?;
    let real = HelmRunner::new(root.path(), true)?;
    let chart = publish_chart(&real, "value: default\n")?;
    let program = fake_helm(programs.path(), "exit 1")?;
    let runner = HelmRunner::with_program(root.path(), true, program.clone())?;
    let request = TemplateRequest {
        chart: &chart,
        values: br#"{"value": "a"}"#,
        kubernetes_version: "1.29.0",
    };
    let case = ScratchDir::new("helm_invocation")?;
    runner.template(&request, case.path(), "render", &Cacheability::Cacheable)?;
    let replaced = fake_helm(programs.path(), "exit 0")?;
    fs::rename(&replaced, &program)?;
    let Err(error) = runner.template(&request, case.path(), "render", &Cacheability::Cacheable)
    else {
        eyre::bail!("a replaced Helm executable ran under the old identity");
    };
    eyre::ensure!(
        format!("{error:?}").contains("changed during the run"),
        "unexpected failure {error:?}"
    );
    Ok(())
}

/// A client-only template of a chart calling `lookup` replays; the same
/// chart outside a client-only template never does.
#[test]
fn lookup_charts_replay_only_as_client_only_templates() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_invocation")?;
    let runner = HelmRunner::new(root.path(), true)?;
    let chart = publish_chart(&runner, "value: default\n")?;
    let values = r#"{"value": "a"}"#;
    let client_only = Cacheability::ClientOnly;
    render(&runner, &chart, values, "1.29.0", "render", &client_only)?;
    let again = render(&runner, &chart, values, "1.29.0", "render", &client_only)?;
    sim_assert_eq!(have: again.outcome, want: Outcome::Replayed);
    let mut request = base_request();
    let client = request.key()?;
    request.client_only = false;
    eyre::ensure!(
        request.key()? != client,
        "client-only mode is part of the key"
    );
    Ok(())
}

/// A panicking job releases its slot: the other workers drain the queue
/// and the panic reaches the caller instead of a deadlock.
#[test]
fn a_panicking_job_propagates_without_deadlock() -> eyre::Result<()> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let outcome = std::panic::catch_unwind(|| {
            run_ordered(
                PoolLimits {
                    workers: 2,
                    memory_bytes: 10,
                },
                (0..6).map(|index| ((index, 0), index)).collect(),
                |_| 10,
                |_, job, _| {
                    if job == 1 {
                        std::panic::resume_unwind(Box::new("job 1 failed"));
                    }
                    job
                },
            )
        });
        let _ = sender.send(outcome.is_err());
    });
    let propagated = receiver
        .recv_timeout(std::time::Duration::from_secs(30))
        .map_err(|_| eyre::eyre!("the pool deadlocked after a job panicked"))?;
    sim_assert_eq!(have: propagated, want: true);
    Ok(())
}

/// The resident helmsweep server prints exactly what the Helm CLI prints for
/// every kind of render (a manifest, a template abort, a coalescence type
/// mismatch, a log record, a values-file error, a bad Kubernetes version), and
/// its invocations are keyed apart from the CLI's.
#[test]
fn the_resident_server_renders_exactly_as_the_cli() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_invocation")?;
    let cli = HelmRunner::with_program(root.path(), false, find_helm()?)?;
    let resident =
        HelmRunner::with_helmsweep(root.path(), false, find_helmsweep()?, RENDER_TIMEOUT)?;
    let chart = publish_chart(
        &cli,
        indoc! {"
            value: default
            sub: {}
        "},
    )?;
    let staged = cli.staging_dir()?;
    write_chart(&staged, "value: default\n")?;
    fs::write(
        staged.join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: replayed
            version: 1.0.0
            deprecated: true
        "},
    )?;
    let deprecated = cli.publish_tree(&staged)?;
    let cases = [
        (&chart, r#"{"value": "a"}"#, "1.29.0"),
        (&chart, r#"{"value": "fail"}"#, "1.29.0"),
        (&chart, r#"{"value": {"nested": true}}"#, "1.33.0"),
        (&chart, r#"{"sub": "not a table"}"#, "1.29.0"),
        (&chart, "value: [unclosed", "1.29.0"),
        (&chart, r#"{"value": "a"}"#, "not-a-version"),
        (&deprecated, r#"{"value": "a"}"#, "1.29.0"),
    ];
    for (chart, values, kubernetes_version) in cases {
        let stage = "render";
        let want = render(
            &cli,
            chart,
            values,
            kubernetes_version,
            stage,
            &Cacheability::Cacheable,
        )?;
        let have = render(
            &resident,
            chart,
            values,
            kubernetes_version,
            stage,
            &Cacheability::Cacheable,
        )?;
        sim_assert_eq!(
            have: (have.success, String::from_utf8_lossy(&have.stdout), String::from_utf8_lossy(&have.stderr)),
            want: (want.success, String::from_utf8_lossy(&want.stdout), String::from_utf8_lossy(&want.stderr)),
        );
        eyre::ensure!(have.key != want.key, "the engine is part of the key");
    }
    Ok(())
}

/// A helmsweep stand-in built from the pinned Helm whose server ends as
/// `body` makes it.
#[cfg(unix)]
fn fake_helmsweep(directory: &Path, name: &str, body: &str) -> eyre::Result<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;
    let program = directory.join(name);
    fs::write(
        &program,
        format!(
            "#!/bin/sh\nif [ \"$1\" = version ]; then printf 'build x\\nhelm.sh/helm/v4 v4.2.3 => ./third_party/helm-v4.2.3\\nhelm-build v4.2.3 43e8b7feece8beb0fcba47059ec9b522fd929a64 clean go1.26.5\\n'; exit 0; fi\n{body}\n"
        ),
    )?;
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755))?;
    Ok(program)
}

/// Every answer that is not a completed render of the waiting request (the
/// server ending, a malformed line, another request's id, an abnormal end,
/// no answer within the timeout, a status Helm never uses) is a harness
/// failure, twice: the failed server is killed and reaped, never reused,
/// and nothing is stored.
#[cfg(unix)]
#[test]
fn resident_protocol_failures_are_harness_failures() -> eyre::Result<()> {
    let programs = ScratchDir::new("helm_invocation")?;
    let answer = |line: &str| format!("while read -r request; do printf '%s\\n' '{line}'; done");
    for (name, body, expected) in [
        ("exits", "exit 0".to_string(), "without answering"),
        ("garbage", answer("not json"), "malformed answer"),
        (
            "other-id",
            answer(r#"{"id":7,"exit_code":0,"peak_bytes":1,"held_bytes":1,"max_rss_bytes":1}"#),
            "is not to request 0",
        ),
        (
            "abnormal",
            answer(
                r#"{"id":0,"error":"abnormal: helm template would exit 2: panic: x","peak_bytes":1,"held_bytes":1,"max_rss_bytes":1}"#,
            ),
            "ended abnormally",
        ),
        (
            "hangs",
            "while read -r request; do exec sleep 600; done".to_string(),
            "no answer within",
        ),
        (
            "status",
            answer(r#"{"id":0,"exit_code":2,"peak_bytes":1,"held_bytes":1,"max_rss_bytes":1}"#),
            "exit code Some(2)",
        ),
    ] {
        let root = ScratchDir::new("helm_invocation")?;
        let program = fake_helmsweep(programs.path(), name, &body)?;
        let runner = HelmRunner::with_helmsweep(
            root.path(),
            true,
            program,
            std::time::Duration::from_secs(3),
        )?;
        let chart = publish_chart(&runner, "value: default\n")?;
        let request = TemplateRequest {
            chart: &chart,
            values: br#"{"value": "a"}"#,
            kubernetes_version: "1.29.0",
        };
        let case = ScratchDir::new("helm_invocation")?;
        let mut failures = Vec::new();
        for _ in 0..2 {
            let Err(error) =
                runner.template(&request, case.path(), "render", &Cacheability::Cacheable)
            else {
                eyre::bail!("{name}: a protocol failure became a Helm verdict");
            };
            check_preserved_bundle(case.path(), &format!("{error:?}"), request.values)?;
            failures.push(format!("{error:?}"));
        }
        eyre::ensure!(
            failures
                .iter()
                .all(|failure| failure.contains(expected) && failure.contains("helmsweep")),
            "{name}: unexpected failures {failures:?}"
        );
        let mut stored = fs::read_dir(root.path())?
            .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
            .collect::<eyre::Result<Vec<_>>>()?;
        stored.sort();
        sim_assert_eq!(have: stored, want: vec!["home", "inputs", "staging", "tmp", "trees"]);
    }
    Ok(())
}

/// Templates see the pinned release's complete capabilities through the
/// resident server exactly as through the CLI: Helm's version, Git commit,
/// tree state and Go version, the Kubernetes version and API versions.
#[test]
fn templates_see_the_release_capabilities_in_both_engines() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_invocation")?;
    let cli = HelmRunner::with_program(root.path(), false, find_helm()?)?;
    let resident =
        HelmRunner::with_helmsweep(root.path(), false, find_helmsweep()?, RENDER_TIMEOUT)?;
    let staged = cli.staging_dir()?;
    fs::create_dir_all(staged.join("templates"))?;
    fs::write(
        staged.join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: capabilities
            version: 1.0.0
        "},
    )?;
    fs::write(
        staged.join("templates/capabilities.yaml"),
        indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: capabilities
        data:
          capabilities: {{ .Capabilities | toJson | quote }}
          helm: {{ .Capabilities.HelmVersion | toJson | quote }}
    "},
    )?;
    let chart = cli.publish_tree(&staged)?;
    for kubernetes_version in ["1.29.0", "1.33.0"] {
        let want = render(
            &cli,
            &chart,
            "{}",
            kubernetes_version,
            "render",
            &Cacheability::Cacheable,
        )?;
        let have = render(
            &resident,
            &chart,
            "{}",
            kubernetes_version,
            "render",
            &Cacheability::Cacheable,
        )?;
        let want_text = String::from_utf8(want.stdout)?;
        eyre::ensure!(
            want.success
                && want_text
                    .contains(r#"\"git_commit\":\"43e8b7feece8beb0fcba47059ec9b522fd929a64\""#),
            "the CLI reports its release build: {want_text}"
        );
        sim_assert_eq!(have: String::from_utf8(have.stdout)?, want: want_text);
    }
    Ok(())
}

/// A resident server holding more than the retirement bound after a render
/// is terminated rather than kept idle; a small one is reused.
#[cfg(unix)]
#[test]
fn large_idle_resident_servers_are_retired() -> eyre::Result<()> {
    let programs = ScratchDir::new("helm_invocation")?;
    for (held, servers) in [(1_u64, 1_usize), (1 << 30, 2)] {
        let starts = programs.path().join(format!("starts-{held}"));
        let body = indoc::formatdoc!(
            r#"
            echo $$ >> '{starts}'
            while read -r request; do
              id=$(printf '%s' "$request" | sed -E 's/.*"id":([0-9]+).*/\1/')
              : > "$(printf '%s' "$request" | sed -E 's/.*"stdout_path":"([^"]*)".*/\1/')"
              : > "$(printf '%s' "$request" | sed -E 's/.*"stderr_path":"([^"]*)".*/\1/')"
              printf '{{"id":%s,"exit_code":0,"peak_bytes":1,"held_bytes":{held},"max_rss_bytes":1}}\n' "$id"
            done"#,
            starts = starts.display()
        );
        let root = ScratchDir::new("helm_invocation")?;
        let program = fake_helmsweep(programs.path(), &format!("held-{held}"), &body)?;
        let runner = HelmRunner::with_helmsweep(root.path(), false, program, RENDER_TIMEOUT)?;
        let chart = publish_chart(&runner, "value: default\n")?;
        for _ in 0..2 {
            render(
                &runner,
                &chart,
                "{}",
                "1.29.0",
                "render",
                &Cacheability::Cacheable,
            )?;
        }
        sim_assert_eq!(have: fs::read_to_string(&starts)?.lines().count(), want: servers);
    }
    Ok(())
}

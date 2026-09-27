# Producer and manifest errors

Every message below comes from `crates/helm-schema-test-support/src/manifest.rs`
(`ProvenanceError`) or `src/generate.rs`. Consumers (`consume`), the producer's final check and
`--verify` all use the same validator, so the same text appears in all three places.

| Message (fragment) | Meaning | Fix |
|---|---|---|
| `read <out>/manifest.json: No such file` | No finished producer run in that directory. The producer deletes the old manifest first and writes the new one last, so a killed or still-running run leaves none. | Check for a live run (`pgrep -af corpus_generation`); otherwise run the producer again. |
| `<out>/.corpus-generation.lock exists: another producer owns <out> (remove it if that run died)` | A run owns the directory, or a killed run left its lock behind. | If `pgrep -af corpus_generation` shows nothing, `rm <out>/.corpus-generation.lock` and rerun. Never remove it under a live run; use a separate `CORPUS_ARTIFACTS` directory instead. |
| `stale build: this binary was compiled from source digest …, but the tree now hashes to …` | A generation source changed after this binary was compiled: a generation crate's `src/`, `Cargo.toml` or `build.rs`, the workspace `Cargo.toml`, `Cargo.lock`, `.cargo/config.toml`, or the vendored grammar. Raised by the producer at start and end, and by consumers. | Finish editing, then rebuild and rerun the producer and the tests. Edits under `tests/` and `src/tests/` never cause this. |
| `stale producer: the manifest was written by a producer built as …, but this build is …` | The manifest's `build` (source digest, target triple, cargo profile) differs from the consuming test binary's: the dump predates your last source edit, or the producer was built with `--release`. | Rerun the producer in the debug profile on the current tree. |
| `the inputs of <key> changed since the producer read them` | A recipe input changed: the chart directory, a values file, a helper, or anything in `testdata/provider-bundle/`. A bundle change invalidates every chart and template artifact. | Rerun the producer. |
| `the manifest was produced for testdata at <A>, not <B>` | The directory belongs to another checkout or worktree. | Produce into a directory for this checkout. |
| `manifest harness version <n>, expected <m>` | Written by an older harness layout. | Rerun the producer. |
| `incomplete manifest: no entry for [...]` / `manifest lists duplicate or unregistered artifacts [...]` / `manifest entry <key> disagrees with the registry on its <field>` | The registry changed since the dump (chart added, removed or renamed, or recipe edited), or the manifest was edited by hand. | Rerun the producer. |
| `artifact <path> is missing or differs from its manifest entry` | An artifact was overwritten or deleted after publication, typically by `--only` into the shared directory or a manual copy. | Rerun the producer; keep `--only` output in a scratch directory. |
| `<out> holds files no registered artifact owns: [...]` | `--verify` and the producer's final check reject unlisted files: `*.tmp` from a killed run, helm-ready companions left from an earlier `--helm-ready` run, artifacts of a registry entry that no longer exists, or your own notes. | Delete the listed files, or the whole output directory (it is a cache), and rerun. |
| `invalid artifact registry: … fixtures: N, expected M` / `duplicate …` | `registry::validate` found a registry whose shape disagrees with `FIXTURE_COUNTS`, or a duplicate id, key, file name or fixture path. | Update `FIXTURE_COUNTS` and the hard-coded counts in `tests/registry.rs` together with the roster change. |
| `N of M artifacts failed:` | Generation itself failed for those artifacts. No manifest is written. | Fix the generation error. The listed keys can be reproduced one at a time with `--only`. |
| `<key>: the locally generated artifact differs from its fixture <path>` (panic with diff) | `HELM_SCHEMA_CORPUS_ARTIFACTS` was unset, so `consume` generated locally and found fixture drift. | Treat it as a fixture mismatch and follow the regeneration procedure. |

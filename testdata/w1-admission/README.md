# W1 admission suite

Every witness cell of the W1 family (Helm `required`, `ternary`, string parameters and `coalesce`
aborting by Go operand kind), with its pinned Helm 4.2.3 verdict and the verdict of main at
`73d1fa159703dc3ca2c2cb9a3016f83edc1bd579` (landing 10d). It is the admission suite of the
value-algebra stages R1–R3 (`plan/value-algebra-interpreter.md`, Stage 1 rescoped): a stage keeps
every cell main gets right and closes cells from `obligations.tsv`. Cells main gets wrong are
obligations, not failing tests.

The format is the `cell_matrix` cells file (`crates/helm-schema-test-support/src/cell_matrix.rs`),
because that tool already measures a (chart, values, Kubernetes version) cell against labelled
binaries and pinned Helm and checks per-cell expectations. The charts, values and recorded
documents here are the canonical inputs. The lane grounds they were taken from are not needed again.

## Contents

- `cells.tsv`: 995 cells, generated with `--declared-types annotate` (the witness lanes' mode).
- `cells-assert.tsv`: 614 cells (`frozen614`), generated with `--declared-types assert`, the
  binary's default, under which that lane was recorded.
- `charts/<lane>/<chart>/`: the cells' charts (`Chart.yaml`, `values.yaml`, `templates/`).
- `values/`: values files too long or unusual to inline. Values are ASCII-escaped JSON: raw U+0085
  decodes differently through YAML, and every lane's Helm ground read the escaped form.
- `coalesced/`: pinned Helm's coalesced values for the 8 cells whose `values.yaml` holds a YAML 1.1
  boolean, which the Rust values port does not model.
- `obligations.tsv`: the 575 cells main gets wrong, with their owner.

Each `expect` column holds `helm_class=<pass|reject> main=<accept|reject>`. Every cell renders at
Kubernetes 1.29.0.

| Group | Cells | Source |
|---|---:|---|
| sealed | 1294 | `round8-w1/.codex-evidence/eval-once-ratchet/witnesses` (22 lanes, 1286 cells) and the four `bool-default-obligations` supplements (8 cells) |
| astra69 | 69 | `round8-w1-astra/.codex-evidence/{coalesce-matrix,coalesce-emitters,coalesce-state,nullary-challenge}` |
| fable64 | 64 | Fable's controls64 lane |
| review75 | 75 | `review-cx*`: counterexamples of the first sol review of `w1/stage1-combined` |
| review-p1 | 7 | `review-p1`: the three P1 counterexamples of the second sol review |
| r0-probes | 83 | `plan/value-algebra-r0-probes/{run,review}.sh` probes that take no extra Helm flags and are deterministic |
| ternary-arity | 17 | `ternary` operand-count cells and their stale-guard controls |

## The gate

`crates/helm-schema-cli/tests/w1_admission.rs` runs in the nextest `integration` profile, so in
`task test:integration` and in CI. It takes about 4 seconds. It generates every chart with the
checkout's CLI. The recipe is `cell_matrix`'s (`admission::generation_arguments`): the bundled
Kubernetes schemas, `v1.29.0-standalone-strict`, the `full` profile, and each file's
declared-types mode pinned. The gate validates every cell's values composed over the chart's
defaults, or the recorded `coalesced/` document. It fails on:

- a NEW failure, where the verdict moves away from `helm_class`;
- a closure, where the verdict moves toward `helm_class`. Re-record the cell (below), so the
  suite always states the current main;
- any unreadable cell, failed generation or uncomposable values;
- any difference between `obligations.tsv` and the cells where `main` disagrees with Helm.

Helm does not run in the gate: `helm_class` is recorded.

## Re-recording

After an intended change (a closure, or a new chart or cell), re-measure with pinned Helm. Both
tables must label the binary that will become main `main`:

```sh
cargo build -p helm-schema-cli -p helm-schema-test-support --bins
cell_matrix --cells testdata/w1-admission/cells.tsv --declared-types annotate \
  --bin main=target/debug/helm-schema@<sha> --helm <helm 4.2.3> --helmsweep target/helmsweep --out <a>
cell_matrix --cells testdata/w1-admission/cells-assert.tsv --declared-types assert \
  --bin main=target/debug/helm-schema@<sha> --helm <helm 4.2.3> --helmsweep target/helmsweep --out <b>
admission_refresh <a>/matrix.tsv <b>/matrix.tsv
admission_refresh --coalesced <helm 4.2.3>
```

`admission_refresh` rewrites every `expect` column and `obligations.tsv`, and keeps each existing
obligation's owner. A new obligation gets `R1-R3`. `--coalesced` re-records `coalesced/`. Read the
diff before committing it. Adding `--require-expected` to `cell_matrix` instead checks the recorded
expectations against pinned Helm. The recipe generates with the bundled Kubernetes schemas, unlike
the lanes' `--no-k8s-schemas`, so the recorded verdicts belong to this protocol.

## Obligations

Main gets 575 cells wrong: 342 false rejections and 233 false acceptances.

- 560 cells belong to R1–R3.
- 15 cells (`ternary127.selected-{0,2,3}-{4..8}.0`) belong to ADR 0042, not W1 (decision D2,
  2026-10-23). They render once the root's `additionalProperties: false` is lifted
  (`plan/value-algebra-r0-probes/selected.sh`). With them excluded, ternary127 has 38 W1
  obligations.

| Lane | Wrong | Lane | Wrong |
|---|---:|---|---:|
| frozen614 | 292 | ternary127 | 53 (15 ADR 0042) |
| literal96 | 30 | arity54 | 28 |
| required64 | 22 | r0-probes | 22 |
| fable-controls64 | 21 | reviewer42 | 17 |
| astra-coalesce-matrix | 17 | empty31 | 15 |
| store24 | 13 | intrinsic60 | 6 |
| ternary-arity | 5 | astra-coalesce-emitters | 5 |
| compose8 | 4 | review-cx3 | 3 |
| astra-coalesce-state | 3 | guarded-writes | 3 |
| message16, nil31, piped3 | 2 each | 10 lanes | 1 each |

The 10 lanes with one cell wrong are argument6, controls32, header10, phase17, review-cx2,
supplement3 and the four `supp-*` lanes. emitters4, valuepaths36, tpl2, astra-nullary-challenge,
review-cx and review-p1 have none.

The `ternary-arity` lane records why the operand-count check did not land in stage 1. An abort
capture conjoins the call's guards, and those guards read the original inputs. The
`stale-guard-after-write.1` and `stale-and-after-write.1` cells render because a prior `set` makes
the guard false, yet a capture under the stale guard rejects them. Emitting only captures whose
conjunction stays empty still rejects `stale-local-guard.0`, whose guard folds to a stale constant
after a `with`-scoped reassignment and an aliased `set`. The check waits for R2's write-free
certificate.

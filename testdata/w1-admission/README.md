# W1 admission suite

Every witness cell of the W1 family (Helm `required`, `ternary`, string parameters and `coalesce`
aborting by Go operand kind), with its pinned Helm 4.2.3 verdict and the verdict of main at
`73d1fa159703dc3ca2c2cb9a3016f83edc1bd579` (landing 10d). It is the admission suite of the
value-algebra stages R1–R3 (`plan/value-algebra-interpreter.md`, Stage 1 rescoped): a stage keeps
every cell main gets right and closes cells from `obligations.tsv`. Cells main gets wrong are
obligations, not failing tests; nothing here runs in the workspace test profiles.

The format is the `cell_matrix` cells file (`crates/helm-schema-test-support/src/cell_matrix.rs`),
because that tool already measures a (chart, values, Kubernetes version) cell against labelled
binaries and pinned Helm and checks per-cell expectations.

## Contents

- `cells.tsv`: 994 cells, generated with `--declared-types annotate` (the witness lanes' mode).
- `cells-assert.tsv`: 614 cells (`frozen614`), generated with the binary's default declared types,
  as that lane was recorded.
- `charts/<lane>/<chart>/`: the cells' charts (`Chart.yaml`, `values.yaml`, `templates/`).
- `values/`: values files too long or unusual to inline. Values are ASCII-escaped JSON: raw U+0085
  decodes differently through YAML, and every lane's Helm ground read the escaped form.
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
| ternary-arity | 16 | `ternary` operand-count cells and their stale-guard controls |

## Running

```sh
cell_matrix --cells testdata/w1-admission/cells.tsv --declared-types annotate \
  --bin main=<main CLI>@73d1fa159703dc3ca2c2cb9a3016f83edc1bd579 --bin cand=<candidate CLI>@<sha> \
  --helm <helm 4.2.3> --helmsweep target/helmsweep --out <dir> --require-expected
cell_matrix --cells testdata/w1-admission/cells-assert.tsv \
  --bin main=<main CLI>@73d1fa159703dc3ca2c2cb9a3016f83edc1bd579 --bin cand=<candidate CLI>@<sha> \
  --helm <helm 4.2.3> --helmsweep target/helmsweep --out <dir> --require-expected
```

With a main binary of that commit, exit 0 means pinned Helm and main reproduced every recorded
verdict. In `matrix.tsv`, a cell is NEW for the candidate when `cand` disagrees with `helm_class`
(`accept` against `pass` and `reject` against `reject` agree) where `main` agrees, and closed in the
opposite case. The recipe generates with the bundled Kubernetes schemas, unlike the lanes'
`--no-k8s-schemas`, so the recorded verdicts belong to this protocol.

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
the guard false, yet a capture under the stale guard rejects them. The check waits for R2's
write-free certificate.

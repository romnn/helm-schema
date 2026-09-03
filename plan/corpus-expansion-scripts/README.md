# Corpus expansion v1 — reproduction assets

Supporting material for `plan/corpus-expansion-v1.md`. Nothing here runs in CI;
these are the exact tools and raw results behind that document's numbers.

## Files

- `survey.sh` — surveys ONE chart: fetch at a pinned version, `helm template` it
  on defaults, generate a schema with the corpus-canonical options, self-validate.
  Emits one JSON line. Set `CORPUS_SURVEY_DIR` to a scratch directory outside this
  repo, then drive it with `xargs`:

  ```sh
  export CORPUS_SURVEY_DIR=/tmp/corpus-survey
  cat worklist.tsv | xargs -P 4 -n 4 ./survey.sh >> "$CORPUS_SURVEY_DIR/survey.jsonl"
  ```

- `coalesce.sh <chart> <out.json>` — dumps the fully COALESCED `.Values` document
  Helm validates a `values.schema.json` against (root values + every subchart's
  defaults under its key + propagated globals). Helm has no command for this, so
  it renders a throwaway `{{ .Values | toYaml }}` template inside a copy of the
  chart. **Validate against this, not the raw root `values.yaml`.**

- `self-validation-prober.rs` — standalone validator, `main.rs` of a scratch crate
  depending only on `jsonschema` 0.47, `serde_json`, `serde_yaml`. Mirrors
  `crates/helm-schema-cli/tests/common/values_validation.rs` semantics exactly.
  Python `jsonschema` is far too slow at these schema sizes.

## Raw results

- `worklist.tsv` — the 349 surveyed charts: name, version, repo URL, stars.
  Sourced from the Artifact Hub popularity ranking, deduplicated against
  `testdata/charts`, filtered to reachable repositories.
- `survey.jsonl` — one line per chart: helm-renders-defaults, generation exit code
  and wall time, schema size, self-validation verdict.
- `final-verdicts.txt` — the flagged charts re-probed against COALESCED values.
  24 confirmed false rejections, 3 artifacts of the raw-values oracle.
- `recheck.txt` — the other direction: all 271 charts that PASSED the raw oracle,
  re-probed against coalesced values. 266 still accept; the 4 rejects are Istio
  charts tripping the dump technique's `_original` limitation (see the findings
  document), and 1 chart's values fail to parse.
- `corpus-gate.txt` — the existing 63 corpus charts re-gated against their
  committed fixtures using coalesced values. 50 of 50 renderable charts accept.

## Localizing an opaque rejection

`"False schema does not allow {...}"` means an `{"if": <cond>, "then": false}` arm
fired. To find which: extract each candidate arm's `if` into a standalone schema
with the full `$defs` attached, extract the relevant subtree of the coalesced
defaults into a JSON file, and run the prober against each. The arm that
*accepts* is the arm that fires.

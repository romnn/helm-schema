# T0: two-binary value-algebra differential gate

Builder-ready specification, 2026-10-24. Normative companion to
[value-algebra-r3b.md](value-algebra-r3b.md), §6. T0 is an independently landable **test-infrastructure**
change, before R3b construction. It changes no interpreter semantics, fixtures, acceptance roster
baseline or admission expectations. ADR 0066's infrastructure exception applies to T0 only.
R3b is semantic and has ten non-landable construction checkpoints; C02 is its first active migration.

## 1. Extraction and ownership

Implement in `crates/helm-schema-test-support/`:

| File/module | Responsibility |
|---|---|
| `src/acceptance/{mod,discovery,screen,adjudicate}.rs` | Shared comparison core: explicit schema inputs, composition/probes, schema screening, pinned Helm/Kubernetes evidence |
| `src/value_algebra_diff/{mod,capture,compare,history}.rs` | Capture provenance, witness universe, comparison classes, construction policy and chained receipts |
| `src/bin/value_algebra_diff.rs` | Typed subcommands; no semantic policy hidden in shell scripts |
| `src/bin/value_algebra_export.rs` | Same-revision AnalysisSession ContractDocument exporter plus schema for CLI cross-check |
| `tests/value_algebra_diff.rs`, `tests/acceptance.rs` | Public tool/core contracts, with helpers under tests; no production analyzer changes |

Extract the reusable machinery currently inside `helm-schema/tests/schema_emission_profiles.rs`:
`compare_charts`, `screen_chart`, `screen_template_probes`, `probe_groups`, `adjudicate_probe_group`,
`adjudicate_probe`, `fold_results`, their result/data types and the pure evidence classification.
Move the needed parts of `tests/common/{emission_profile_harness,lint_probes,lint_adjudication,helm_pool}.rs`
into test-support, reusing existing `test-support/src/helm/{adjudication,invocation,cache_policy}.rs`.
Avoid two copies of the same composition/screening/adjudication code. Existing tests import the
extracted APIs; test-support already depends on test-util and helm-schema, so no dependency cycle
or production library dependency on a test crate is needed. Keep the ordinary battery's named
entry point, tests and roster validation wrapper. Preserve its outcomes and reports on extraction.

**Do not invoke the ordinary battery as the construction comparator.** Today
`corpus_acceptance_comparison` loads its baseline using `ChartInputs::Fixture`/`git show` on committed
fixtures, not a baseline producer dump. `live_battery_preflight` requires `ROSTER_BASELINE`;
`validate_external_comparison` still enforces final false/undecided roster policy. Neither entry
point accepts temporary construction regressions simply by supplying two dump directories.

Replace ambient-environment decisions inside the extracted core with explicit configuration.
Environment variables remain adapters in the existing battery wrapper. The core accepts both
byte streams, manifest-backed provenance, recipes and context controls; it returns raw outcomes
and completeness evidence. It must neither import KNOWN_* rosters nor reject merely because a
semantic outcome is wrong. Parse/tool/composition/coverage errors remain typed failures.
Construction policy and ordinary final-landing policy are distinct consumers of the same report.

Rust API sketch (test-support types; field names are contractual, helper type details may reuse
existing equivalents). Opaque verified inputs cannot be constructed without verification:

```rust
pub struct VerifiedDump { /* private verified manifest, root, artifact index */ }
pub struct VerifiedSchema { /* private exact bytes, digest, recipe and dump identity */ }
pub struct SchemaPair {
    pub baseline: VerifiedSchema,
    pub candidate: VerifiedSchema,
    pub chart: PinnedChartInput,
    pub recipe: GenerationRecipe,
}
pub struct ComparisonConfig {
    pub helm: PinnedHelmIdentity,
    pub kubernetes: VerifiedProviderBundle,
    pub contexts: VerifiedContextRoster,
    pub discovery: DiscoveryLimits,
    pub pool: PoolLimits,
}
pub struct CoreReport {
    pub discovered: Vec<Witness>,
    pub results: Vec<WitnessOutcome>,
    pub coverage: CoverageReport,
    pub evidence: Vec<EvidenceRef>,
}
pub fn compare_pairs(
    pairs: &[SchemaPair], replay: &WitnessUniverse, config: &ComparisonConfig,
) -> Result<CoreReport, ComparisonError>;
```

Implement verified-dump construction using existing manifest verification rules. Each producer
verifies its own dump from its own recorded checkout/source digest; the comparator additionally
checks captured digests/bytes without compiling against the other revision's source. Two manifests
must match registry keys and recipe inputs; source/build digests differ legitimately. No implicit
fallback to committed schema fixtures or to whichever producer is on PATH.

Core reports include every composed document and its base/candidate schema verdict, changed/new
rejection reasons, exact Helm class, defaults/context-relative Kubernetes decision, and evidence
paths. Preserve Template, LintRaw and LintCoalescedTwice modes, intrinsic-abort/inapplicable status,
missing-values/templates applicability, uncertain-resource outcomes and declared-types policy.
If a policy attribution needs an alternate annotate schema, capture it using **each revision's**
recorded recipe/executable and verify its provenance. Never regenerate the baseline with candidate
libraries. Raw errors are not schema rejection. Keep unknown diagnostic/timeout failures distinct.

The full version-2 context roster is a required explicit input for corpus comparisons. Hash its
exact bytes, preserve all row origins, complete tested/context-only overlays, chart selection and
Kubernetes versions. Do not reconstruct contexts from paths. Empty defaults context is explicit.
Reuse existing dependency/import/composition completeness limits and coverage validation. Selected
chart diagnostics cannot certify complete coverage. Roster policy files/identities are additional
inputs to the *ordinary battery wrapper*, never implicit construction acceptance oracles.

## 2. Commands, capture and provenance

CLI grammar (all paths explicit; output directories must be new):

```text
value_algebra_diff capture --bin PATH --producer PATH --exporter PATH
  --checkout DIR --commit SHA --build-receipt FILE --suite DIR --controls DIR
  --bundle DIR --contexts FILE --helm PATH --helmsweep PATH --out NEW_DIR
value_algebra_diff init --main CAPTURE --plan HISTORY_PLAN --out NEW_HISTORY_DIR
value_algebra_diff compare --main CAPTURE --candidate CAPTURE --history DIR
  --previous RECEIPT --ledger FILE --out NEW_DIR
value_algebra_diff compare ... --seed-review FILE
value_algebra_diff compare ... --landing --gates GATE_RECEIPT
value_algebra_diff amend --history DIR --previous RECEIPT --witness-batch FILE
  --review FILE --out NEW_DIR
value_algebra_diff migrate --history DIR --previous RECEIPT --new-main CAPTURE
  --new-plan HISTORY_PLAN --review FILE --out NEW_DIR
value_algebra_diff verify --capture DIR
value_algebra_diff verify --history DIR --receipt FILE
```

Initial main==main self-test uses a dedicated `--self-test` mode; it creates no construction seed.
`init` creates genesis at ordinal 0 for M. HISTORY_PLAN (version 1) declares history ID, M/suite/
controls identities, checkpoint ordinals 1–10, seed ordinal 2, required lane/control IDs and
reviewer identity; actual checkpoint commits are bound when captured. C01 compares identically.
C02 alone permits `--seed-review`; every construction comparison requires a predecessor.
The separate self-test mode cannot publish construction receipts. Specify checkpoint
ordinal/commit in the reviewed history manifest, not an unconstrained caller label. No comparison
mutates older captures, ledgers or receipts. Capture and history updates use create-new/atomic
publication; failure leaves evidence and a failure report, never a success receipt. Source trees, Git index, fixture
expectations and shared caches are read-only. Scratch/replay caches are private to the run.

Capture manifest `format_version: 1` binds full git/tree/source digests, checkout path, CLI,
producer/exporter/tool binary hashes, command/configuration, Helm version **4.2.3** and binary hash,
Go/helmsweep build identity, suite/control roster+contents hashes, context roster, provider bundle,
Kubernetes release, recipes/profiles/declared-types mode and inventory format. Also bind Cargo
artifact JSON/build receipt (source digests before/after build, compiler/target/profile, artifact
paths/hashes). A caller-provided commit label alone proves nothing. Test-only sources are hashed
explicitly because the producer's source digest excludes tests. Reject mixed/stale builds.

Freeze main M after composed R2/R3a; capture copied binaries before subsequent builds can overwrite
them. Capture candidate with the same suite/control/recipe/provider/Helm inputs. Each capture:

1. Uses `cell_matrix`'s private prepared-chart semantics, including shipped-schema/test removal.
   Generate offline, `--no-config`, pinned Kubernetes bundle, full profile and all suite modes.
2. Generates one CLI schema per `(chart digest, declared-types mode, recipe)`, validates **every**
   admission cell's recorded coalesced document and verifies inputs/document hashes, roster
   completeness, main/obligation agreement and the fifteen exact ADR-0042 policy identities.
   Never validate bare override bytes, and never substitute `{}` for coalesced defaults.
3. Runs that revision's `corpus_generation --out NEW_DIR --helm-ready` once for the complete
   registry, then `--verify` with that same producer/checkout. Retains all schema, final-policy,
   lean, registered IR, internal artifacts and Helm-ready companions/name maps plus manifest.
4. Runs CLI equivalents for registry whole-chart recipes and requires byte/verdict agreement with
   the same revision's producer. Library-only recipes remain explicitly producer-only comparisons.
5. Runs companion exporter per admission/control/whole-chart job: construct AnalysisSession with
   identical GenerateOptions/provider/output policy, use public `contract_document()`, serialize
   stable JSON and emit schema through the same session pipeline. Schema must equal CLI output.
   Registered template IR uses existing `generate::ir_document`; no fake `IR_DUMP` environment API.
6. Records exact stdout/stderr/status and artifacts per job. Only recorded scratch-root prefixes
   may be normalized for process-output comparison. JSON array ordering, provenance, source sites,
   truncation and schema contents cannot be stripped to force equality.

Admission inputs initially include the R2 extension (1,821 cells/687 open at 0db9bbf6, reconciled
against actual main) and deterministic R3b probes turned into cells. Mode-stamped controls retain
lint/exact-output cases separately; a schema CLI is not a renderer. Their Helm ground truth feeds
interpreter full-result tests; CLI/IR queries must explicitly name the observation being tested.
No claim that a still-rendering schema verdict verifies a string/alias observation.

Use pinned Helm live remeasurement at baseline freeze and for every changed semantic verdict.
Reuse an oracle only when exact chart, input bytes, coalesced document, mode, environment, Helm,
Kubernetes and context identities match. Helm class drift fails infrastructure validation.
Unordered/random cases carry a set of possible outcomes or explicit unknown coverage; observed
samples never become an exact one-outcome oracle. Analysis cache tests assert disabled/cold/hit
invariance, not equality of random Helm samples. Unresolved oracle coverage cannot certify a closure.

## 3. Witness identity, reports and policy

Outputs: `cells.tsv`, `artifacts.tsv`, `changes.jsonl`, `regressions.tsv`, `closures.tsv`,
`coverage.json`, `witnesses.jsonl`, immutable blobs/evidence and `receipt.json`. Every required
job/cell/artifact/witness appears exactly once per applicable observation/mode. Stable sort for
presentation; row position is never identity. Invalid schema, missing/extra registry keys,
unexpected process failure, timeout, stale manifest or empty selection fails infrastructure.

| Class | Meaning |
|---|---|
| IDENTICAL | Same cell verdict/observation; byte equality reported separately, never implied |
| INHERITED | Main and candidate wrong identically under the pinned oracle; obligation retained |
| POLICY | Exact pre-approved ADR-0042/declared-types policy identity; no semantic closure credit |
| CLOSURE | Main wrong, candidate correct with live complete evidence |
| REGRESSION | Main right, candidate wrong, or any earned closure lost |
| EQUIVALENT-CHANGE | Reviewed denotation/observation equivalence for changed bytes |
| UNADJUDICATED | Changed schema/IR/output with insufficient evidence; comparison fails |
| INFRASTRUCTURE | Provenance/coverage/input/tool/execution failure; always fatal |

The extracted core discovers/screens fresh probes using **both dump schemas**, then replays the
persistent universe (§4) independently. Apply existing coalescing/null-deletion/forced-raw-audit
rules, coverage floors/caps, new-rejection-reason tests and Helm/Kubernetes adjudication. Do not
invoke `round74` with M masquerading as ROSTER_BASELINE. Final ordinary battery runs separately
against its approved roster baseline and the same final candidate dump, with existing policy intact.

Every schema/IR/output byte change has a structural diff and proof links in `changes.jsonl`:
source site, change group, relevant witness IDs, negative controls and projected artifact links.
A closure for one cell does not waive all changes in its chart. IR-only changes require concrete
Helm output/abort witnesses or reviewed representation-equivalence certificates. A certificate
binds both artifact hashes, JSON pointers, reviewer and reasoning, and expires when bytes change.
Pointers/hashes identify a particular **proof**, not the persistent semantic witness itself.
Finite verdict agreement alone is not an IR equivalence proof. No file/directory wildcard waiver.
The tool checks linkage/hash integrity; reviewers judge evidence adequacy. UNADJUDICATED cannot
be seeded as a tolerated semantic regression.

Persistent witness record, JSON format version 1:

```text
id = SHA256(canonical identity tuple)
identity = (chart-input digest, exact original values-bytes digest, transport/composition mode,
            kube/context identity, recipe/declared-types mode, stable observation key)
blobs = original values bytes, context-only bytes, composed document bytes, chart recipe inputs
oracle = Helm/build/provider identities, complete class/alternative coverage, evidence refs
origins = admission IDs / discovery checkpoints / guard or corpus generator / IR-output control
```

Store exact bytes in a content-addressed immutable blob store, not just hashes. Include all
validated composed-document hashes and applicability; recompose/reverify on replay. Deduplicate
execution by exact bytes/context as existing harness does, but retain all origins. Different
whitespace inputs remain distinct original-byte identities where the harness distinguishes them.
Observation key is, e.g., `schema-acceptance`, a named executed output/termination control, or a
reviewed semantic predicate tied to stable source identity. It excludes candidate schema digest,
current JSON pointer, generated ordinal and checkpoint number. When artifact structure shifts,
map the same observation to new pointers; inability to map is UNADJUDICATED, not removal.
Changing chart inputs creates a new identity and retains the old experiment in the universe.

## 4. Persistent universe and chained monotonicity

History contains pinned M, suite/control digests, retained captures/binaries for each checkpoint,
append-only witness batches, reviewed seed ledger, receipts and amendment receipts. Let U_i be
**all** witnesses ever materialized by admission, corpus discovery, context rows, IR/output controls
or reviewer additions through checkpoint i. Persist discoveries even if verdicts agree, no flip
is generated, or the checkpoint fails. A failed attempt adds its completed discoveries too; it cannot erase history. Before policy
validation, append the new batch to `history/universe/` and atomically advance its hash-chained
`universe-head` under an exclusive history lock. Each journal record binds its previous hash,
batch hash and capture/attempt identity. A crash after journaling resumes from that head.
The orchestrator pins this head even for failed attempts; removing a pending batch or rolling
back the head is an identity failure. Gate receipts separately chain accepted checkpoints.
Thus failure need not publish a success receipt to make its discovered witnesses permanent.

`U_i = U_(i-1) ∪ discovered_i`. Fresh-probe caps limit discovery only. Replay every U_i witness
against M and the candidate outside those caps, in bounded chunks with complete counts; no
sampling or time-budget skip. A newly generated targeted guard must never displace an old base
probe from regression checking. Persisted witnesses are screened even when the new generator
would no longer select their bucket. Coverage reports separately count discovery candidates,
admitted/cap-limited discovery, historical replay, applicability/errors and oracle completeness.
Existing discovery floors/caps still apply; a huge replay may cost more, never silently truncate.

On universe growth, backfill new witnesses against M and **all retained checkpoint captures**
(or rerun retained binaries with verified original inputs). Recompute the whole semantic history
on the expanded universe. Existing comparison receipts remain immutable; publish a hash-sealed
amendment receipt binding their hashes, the added batch and new per-checkpoint tables. The successor
references that amendment as its predecessor. Missing history/binaries blocks comparison; it is
not permission to start with a smaller universe. This applies equally to generated battery and
IR/output witnesses, not just committed admission IDs.

C02 establishes a measured, reviewed R_seed; C01 must equal M. Seed ledger rows include semantic
identity, M/candidate/oracle answers, error direction, exact evidence, owner/cause and repair
checkpoint. `--seed-review` compares the proposed set exactly with reviewed rows/reviewer IDs;
no blanket count/lane allowances and no second seed. All oracle-decided C02 losses relative to M must be listed; unchanged inherited failures stay
in INHERITED, not the seed. Unknown/unadjudicated artifacts cannot enter the seed.

For each later checkpoint, on the **same expanded universe** require `R_i ⊆ R_(i-1)` and preserve
all previously correct/closed observations. A removed regression cannot return. Equal counts
with exchanged IDs fail. Track verdict/error direction and failed predicates as well as ID;
a stronger/different failure needs fresh adjudication and cannot hide behind the old cell name.
Semantic checkpoints must shrink R or add verified closures; representation/cache checkpoints
may keep R unchanged. Candidate expectations never feed this comparison.

If a newly discovered witness proves a previously unknown C02 loss, the ordinary compare stops.
A reviewed **history amendment** may append that retrospectively measured C02 member only with
all checkpoints replayed and the entire reconstructed post-seed sequence monotone (C01 remains identical to M; C02 is the sole
seed exception). It does not move M,
reset ordinal, erase a previously earned right or authorize a newly introduced later loss.
This is the sole retrospective seed-extension procedure; there is no automatic seed expansion. Its receipt
records explicit reviewer/evidence IDs and the original seed hash. An already-known witness can
never use this procedure. Newly exposed non-monotonic history blocks progression until a reviewed
repair/reconstruction restores the chain; merely changing the branch name is ineffective.

Receipt format version 1 has these required fields:

```text
format_version, tool_version_and_hash, run_id, history_id, kind
checkpoint_ordinal, checkpoint_commit, capture_hash, baseline_capture_hash
suite_hash, controls_hash, context_roster_hash, provider_hash, helm_identity
predecessor_receipt_sha256, seed_ledger_sha256, universe_parent_hash, universe_hash, universe_journal_head_sha256
witness_batch_hashes, artifact_manifest_hashes, coverage_hash, proof_index_hash
regression_set_hash, earned_closure_set_hash, full_outcome_table_hash, gate_mode, exit_code
```

`kind` is genesis/checkpoint/amendment/migration/landing. `amend` performs the backfill procedure
above from the current journal union and a reviewed amendment payload; `migrate` performs full
replay under explicitly new identities, preserving the prior chain. Both require the trusted
predecessor, produce a new receipt and never edit old outcome tables. An amendment does not
consume a construction ordinal; its receipt retains that of the last accepted checkpoint.
Migration records old and new history-plan identities and their mapping. Genesis alone has null predecessor; history ID
binds M, suite and the reviewed branch plan. Canonical JSON hashing excludes only its own hash;
all referenced blobs are rehashed by verify. Reject unknown versions, ordinal gaps, forks supplied
as a different predecessor, baseline/suite substitution, dropped witness batches or shrinking
universes. Local success is relative to the orchestrator's frozen trusted gate-head receipt and universe journal head, supplied
in history; hash chaining alone is not authenticity. Orchestrator atomically advances that head
once per accepted gate successor (the universe head advances for every discovery batch); a stale head cannot authorize another seed or fork. No Git writes.

Main/suite/tool moves require explicit history migration: preserve old receipts, replay the entire
chain against the new inputs/tool, retain old experiment witnesses and prior earned closures, and
publish a reviewed migration amendment. No implicit refresh. Ledger reductions are proposed exact
edits; stale rows fail until reconciled, additions fail except the explicit new-witness procedure.

Exit codes: 0 complete and policy-satisfied; 1 semantic/unadjudicated/monotonicity failure;
2 invalid request/identity/chain; 3 execution/provenance/coverage infrastructure failure.
Every construction success receipt says **NON-LANDABLE**, even if R is empty.
`--landing` requires R empty, strictly fewer inherited semantic failures, all agreed R3b controls
closed, no unapproved owner changes, complete proof coverage and all applicable ordinary final
gates tied to the final tree/dump. A zero-flip run is not evidence of semantic progress.

## 5. Build order and acceptance tests

T0 can use local implementation commits, but lands as one infrastructure change:

1. Extract core/explicit inputs; existing ordinary battery tests and policies remain unchanged.
   Add dump-pair adapter and report all outcomes without roster-policy side effects.
2. Add capture/export/build receipts and byte parity checks; freeze M and run M==M non-vacuously.
3. Add persistent union/discovery replay, core witness serialization, chained history and policies.
4. Add AST inventory construction mode driven by the reviewed deletion map. Preserve frozen
   class/seal checks; exact due keys, supplemental `edge:` caller/producer mappings and post-move
   owners are checked. Temporary F/S/E rows require `replaces` and C09 expiry. Never alter
   `landed.txt` or weaken ordinary lint/final inventory policy.
5. Publish a self-test receipt plus representative two-dump evidence; document commands and costs.
   T0 cannot predeclare R3b losses or refresh main expectations.

Required tool tests (debug tests; meaningful fixture inputs, no production evaluator changes):

- Two verified dump schemas deliberately differ from committed fixtures: report exactly the dump
  pair's flip. Changing committed fixture bytes cannot affect it. Swapped/stale manifests fail.
- Construction accepts an exactly reviewed temporary semantic loss; ordinary final battery rejects
  that same unrostered loss. Context-roster omissions/mode mismatch fail composition coverage.
- M==M, missing/duplicate cell/artifact, wrong coalesced bytes, invalid schema, unknown diagnostic,
  timeout, stale producer/exporter, source changes during capture, CLI/producer/export mismatch.
- Closure, new regression, equal-count set swap, lost closure, changed failure severity, stale
  ledger row, attempted second seed, C01 seeding, receipt-version/parent/ordinal/baseline mismatch.
- Guard-probe cap displacement: witness selected at C02 disappears from fresh discovery at C03,
  yet persists/replays and a regression is still detected. Repeat for a corpus acceptance flip
  and for an IR-only/output witness whose current JSON pointer changes.
- Late witness backfill across M/C01/C02/later checkpoints; missing history blocks. Nonmonotone
  retrospectively discovered history fails. Reviewed monotone amendment retains old receipt hashes.
- Unordered Helm samples cannot earn an exact oracle/closure. Zero-flip and empty/vacuous coverage
  do not pass semantic landing. Unknown/inapplicable composition is accounted explicitly.
- Final ordinary battery extraction parity (coverage/outcomes/rosters), producer verification,
  admission completeness and inventory mode separation. Preserve existing test names/entry points.

Use existing tracing/Perfetto support for capture, discovery/replay, composition and adjudication;
no custom phase timers. Report witness-union growth, replay/cache counts, tool wall/CPU/RSS and
per-chart costs separately from analyzer performance. Run applicable verification-gates on T0's
final tree; a passing self-test alone does not replace those gates. No R3b production implementation
is included in this design deliverable.

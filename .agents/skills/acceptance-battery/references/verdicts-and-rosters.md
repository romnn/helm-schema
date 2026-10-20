# Verdicts, probe coverage, and rosters

Source of truth: `adjudicate_flip`, `HelmAdjudicationCoverage`,
`validate_helm_adjudication_coverage` and `validate_probe_coverage` in
`crates/helm-schema/tests/schema_emission_profiles.rs`; the rosters in
`crates/helm-schema/tests/common/known_false_acceptances.rs` and
`crates/helm-schema/tests/common/known_undecided_acceptances.rs`; the probe
generator in `crates/helm-schema/tests/common/emission_profile_harness.rs`.

In the harness, `full` or `before` always means the **baseline** schema and
`lean` or `after` means the **candidate** schema. The names come from the
profile comparison the harness was first built for.

## The probe battery (per chart)

The legacy structural probes are judged against the **coalesced** values document. Sparse
overrides are composed over the chart's coalesced defaults with Helm's
`CoalesceTables` semantics, where a `null` deletes the value it meets
(`test_util::helm_values`). A bare `{}` is not "defaults apply".

| Probe group | Name shape | Source |
|---|---|---|
| defaults | `defaults` | the coalesced defaults |
| all keys deleted | `all declared keys deleted` | composed document keeping only dependency roots, which Helm refills |
| base probes, depth 1 and 2 | `<a.b> <- <class>` | every default path at depth <= 2, set to each of 12 value classes: `null deletion`, `false`, `true`, `integer`, `number`, `empty string`, `coercible string` (`"3"`), `non-coercible string`, `empty array`, `empty object item` (`[{}]`), `empty object`, `unknown object member` (`{unknown: true}`). Interleaved round-robin across (top-level key, class) buckets. |
| depth-3 deletions | `<a.b.c> <- null deletion [depth 3]` | every default path at depth 3 |
| guard-state probes | `… [targeted: <path> <- <class>]` | satisfying and violating witness pairs for the root `if`/`then` guard arms of the baseline and candidate schemas, deduplicated across both. `guards_discovered` is therefore larger for a real old-versus-new pair than for a schema compared with itself. |
| composite probes | composite guard and payload states | from the same guard discovery |

These are the "three granularities" CLAUDE.md requires of an old-versus-new
prober: top-level keys, second-level keys, and empty member and item probes.
The battery already implements them over the coalesced document model, so an
ad-hoc scratch prober is rarely needed. For an arbitrary pair of schemas, use
`external_schema_pair_flips_are_helm_adjudicated`.

Caps (`emission_profile_harness.rs`): 50,000 probes per chart, 2,048 depth-3
deletions, 24 guard arms attempted, 8 guard state pairs, 128 guard witness
candidates, and 8 composite pairs.

`validate_probe_coverage` fails the run when:

- base or depth-3 probes were truncated, or none were emitted;
- the candidate, emitted, and dropped counts do not add up (base, depth 3,
  guards, witnesses, composites);
- the guard sampling strategy is not the disclosed `SchemaOrderPrefix`;
- fewer than half of the chart's composed probes are reachable.

A composed probe is **unreachable** when no values file makes Helm produce
that document, for example because Helm refills a deleted dependency root. It
is excluded from screening and listed under `unreachable_probes`.

Guards skipped by the cap are disclosed in the coverage report but do not fail
the run.

## Dependency and context document coverage

`common/lint_probes.rs` adds defaults and typed dependency controls plus every row
from the optional context roster (see `env-vars.md`).
Each overlay/version pair screens `LintRaw`, `LintCoalescedTwice`, and `Template`
through cached `AcceptanceInputs`, without sharing mutable tables across calls.
The overlay `{}` means defaults for these input rows; do not confuse it with the
legacy all-keys-deleted composed-document probe.
Missing `values.yaml` makes the raw rule inapplicable.
An absent root `templates/` makes the twice-coalesced lint rule inapplicable,
matching Helm's early return before schema validation.
Intrinsic Helm aborts are counted as `not_validated`; `Unmodelled` and other
errors fail coverage.

`charts[].lint` reports built-in/imported rows, distinct original-file-bytes/version
inputs (the `distinct_overlays` field), contributing
origins and hashes, and per-document `composed`, `not_validated`, `errors`,
`screened`, and `flips` counts.
Every row must be accounted for; composed documents must all be screened, and
composed plus inapplicable documents must equal that distinct input count.
An empty lane, omitted work, or document errors fail in both the corpus and
external-pair entry points, including comparisons with zero screened flips.
Caps are 8,192 combined distinct original-file-bytes/version inputs per chart,
including whitespace variants of the same overlay, with three attempts per input within the remaining 50,000 total budget, dependency depth 64,
and 8,192 imported rows per chart in a roster of at most 64 MiB.
The chart selection must be nonempty.
Existing unchanged-guard sampling remains disclosed sampling; these document-row
caps never silently certify a prefix as complete.

## Screening, then adjudication

1. **Screening** (Rust, every probe): `screens_a_flip` is true when the
   baseline and candidate disagree, or when both reject but the candidate
   rejects for a new reason: a violation with an (instance path, keyword,
   detail) the baseline lacks. The second condition keeps a new false rejection
   from hiding behind an old one. Screening uses the Rust coalescence port, so
   it is not exact. `screening_is_exact` is always reported as `false`.
2. **Schema-only mode** (no `ADJUDICATE_WITH_HELM`): each screened probe becomes
   a flip string `<chart>: <probe>: before=<bool>, after=<bool>`. Any flip then
   fails `round74_…` with "fixture flips were not all live-adjudicated".
3. **Live mode**: each screened probe's overlay goes to pinned Helm. Helm first
   coalesces the values (through a private chart copy that dumps `.Values`),
   then renders the chart. Both profiles re-judge Helm's **exact** coalesced
   document. The render is compared with the chart's defaults render under
   the offline Kubernetes validator (v1.29 strict bundle plus the CRD
   catalog). Flip strings are `<chart>: <probe>: candidate accepts=<bool>`.

### Document-specific live adjudication

A screened document retains its kind through adjudication.
Baseline-schema, candidate-schema, and schema-free chart copies install schemas
only at the root and strip dependency schemas.
The candidate uses the producer's hash-verified Helm-ready companion; the
baseline preserves committed bytes through 5 MiB and shortens only above that
limit, exactly as the sweep does, without regenerating inference.
Validator-only normalization is separate from shipped-byte identity.
Lint uses the selected pinned Helm engine, a cleared environment and the
same invocation identity, replay and failure machinery as template, with a
separate 1,200-second timeout. The resident engine runs Helm's lint action
with fresh settings, chart, values and validation state. Only successful
compiled schemas with no external loads are reused, keyed by exact schema
bytes. Resident `serve` keeps the two most recently used schemas; evicted schemas
recompile on demand, while active callers retain their immutable validator.
Sweep instead retains warmed schemas until their chart completes, so overlapping
charts cannot evict each other before cell execution. The resident entry limit does not bound bytes. Helm's unresolved-URN warning history resets between
commands so diagnostics match a fresh CLI. Engine/build/patch identity
separates stored observations. After each command, idle servers share the
configured Helm memory budget; the oldest idle servers are evicted when
necessary. Active reservations are measured separately, so this is not a
hard bound on combined active and idle RSS.
One fresh baseline/candidate/schema-free lint triplet supplies every screened
acceptance document with the same exact original values-file bytes and Kubernetes
version within one prepared chart and schema pair.
Helm already observes both lint schema rules in that transcript; document kinds
are not independent statistical trials of nondeterministic templates.
Payload renders are shared within that group. Defaults/context observations
are shared lazily across groups only when `render_cacheability` marks the chart
replayable. Otherwise each group observes fresh controls. The key binds SHA-256
of exact values bytes and Kubernetes version; the owning prepared chart binds
chart bytes, engine/build and environment. Retained entries contain evidence
paths, not rendered output, coalesced values or lint transcripts; observations
are reloaded on demand and missing evidence fails adjudication. Each independent
payload group remains fresh; equivalent parsed inputs with different bytes stay
distinct. Per-input locks coordinate only callers of the same exact control.
Each document retains its own origins, applicable contexts, verdict and original
coverage position, with a fresh evidence/decoder directory. A failed control is
observed only by documents whose applicable context set needs it.
Independent nondeterministic invocation requests still bypass replay.
Defaults and context-only controls run schema-free lint alone: neither compared
root schema participates in control usability.
Timeouts, signals, loader failures, unresolved diagnostics, and disagreement
between the Rust prediction and the corresponding real lint rule are failures.

Rule-qualified errors distinguish raw-values and template-rule schema failures.
Equal overall lint exits never collapse a document flip: another lint rule may
already reject the chart.
Schema-free lint and the existing template/Kubernetes oracle compare the tested
input with defaults and every explicit context-only control under the same
Kubernetes version.
A lint or template rejection justifies a tightening only when attributable to the
tested input; an inherited control error never certifies a new schema restriction.
A control that cannot render, including a non-installable library chart, is
unresolved and fails adjudication. Before any lint, template, or Kubernetes
rejection can be attributed, defaults must have no Kubernetes uncertainty and
each context must have no uncertainty or new violated assertion versus defaults.
An unusable control fails with its uncertainty or violation evidence preserved.
All Kubernetes comparisons, including defaults-only and explicit-context lanes,
use one canonical assertion representation and one multiset comparison. A key
contains resource API/kind, the resource-qualified canonical constraint location,
keyword and structural details, and a canonical instance path. Constraint identity
is the resolved schema-resource URI plus the actual keyword's pointer within that
resource. Relative `schema_path` alone is insufficient: different embedded `$id`
resources can both report `/type`. The validator and identity proof share one
prepared offline registry. Before compilation, the retrieval/base URI and every
dialect-defined resource ID and registered anchor must be unique after URI
resolution; embedded declarations may not shadow the retrieval alias or each
other. Any collision makes the entire pinned schema unresolved, even for otherwise
valid documents. Instance literals such as `default` are not schema declarations.
The retrieval entry must also equal the independently supplied pinned root.
The pinned validator also caches compiled nodes by resource base and compiler
pointer, omitting source-node identity. Before building it, a bounded traversal
checks that every such address denotes one source node, dialect and reference
base across validation descendants and reference targets. The reference base is
needed because cached nested reference suffixes depend on it. Definitions are
visited when referenced. The check ignores dynamic scope conservatively and rejects
any collision; it does not disambiguate or rewrite schemas. This covers both anchor
aliases compiled at the empty pointer and pointer aliases crossing embedded IDs.
The proof stops unresolved if it exceeds 100,000 positions. Noncolliding aliases
remain supported. Fragment-bearing resource IDs with a nonempty fragment and
schemas containing dynamic/recursive references remain unproved.

Only an intact validator route is node-resolution evidence. For `propertyNames`,
the outer error reconstructs a route from the child's resource-relative path;
identity and instance roles instead use the wrapped child's retained evaluation
route. Other supported errors must identify their actual keyword and instance.
Boolean-schema errors, omitted dependency-required triggers, `minContains` /
`maxContains` aggregates without complete keyword correspondence and reference errors
fail unresolved. Grouped unevaluated diagnostics continue to fail when member
correspondence is unproved. Custom keywords are not registered. This boundary follows
the pinned validator's error constructors; resolving a selected node back to itself
is never sufficient evidence of provenance.

Execution certainty is a separate precondition. One recursive check rejects
`BacktrackLimitExceeded`, `RegexEngineFailure` or reference errors anywhere in returned
`anyOf` / `oneOf` contexts or wrapped `propertyNames` errors. Boolean applicators and
pattern-key dispatch can discard these errors before constructing a diagnostic, so
returned errors alone cannot establish certainty. Before resource validation, a shared
execution preflight checks every distinct document string value and object key against
every distinct `pattern` / `patternProperties` regex discovered by the compiler-location
traversal, including reference targets. It uses the pinned validator's default engine,
translation and limits, ignores ordinary mismatches, and fails unresolved on execution
errors or more than 1,000,000 regex/string pairs. The original resource validator remains
the sole source of acceptance decisions and canonical assertions.

This deliberately conservative check also rejects failures in inactive pairs; it does
not interpret applicators or claim to intercept every internal engine call. Literal
pattern optimizations use infallible predicates; mixed pattern-key dispatch may execute
the backend for the equivalent literal forms. The guarantee is specific to the audited
pinned implementations, not hypothetical future backend bugs. Record the boolean-path
audit with source locations and hashes, including Draft 4 handlers. Measure execution
preflight results alongside the pinned-bundle assertion audit and full schema-only
battery; report newly unresolved cases and existing compilation failures separately.

After these preconditions, the intact route resolves the constraint node. Its origin
in the pinned schema determines the resource URI and keyword pointer through
dialect-defined resource boundaries, independently of retrieval aliases. Pointer,
URI and supported anchor references to one node inherit once. The resulting URI
must resolve back to that node, and the reported absolute resource must agree with
route resolution. The reported pointer and retrieval alias are not assumed
canonical. Anonymous roots receive a stable base URI. Missing location evidence,
conflicting resources or unproved reference correspondence (including dynamic /
recursive references) fail unresolved. A bundle preflight visits every schema node
in the pinned Kubernetes release and CRD catalog, checks registry integrity and
newly unsupported error shapes, and exercises reported errors with negative
instances. Run it alongside the full schema-only battery and report compilation
failures separately; a zero flip count alone does not measure assertion provenance.
The same qualified location is used for occurrence deduplication and
multiset comparison. Homogeneous array indices become `*`; tuple `items[n]` and
`prefixItems[n]` positions remain fixed, even when slots share a `$ref` target.
Map keys become `*` only at `additionalProperties` or `patternProperties` positions.
Fixed property names remain fixed. Typed path segments distinguish a literal `*`
property from a wildcard and preserve empty, numeric and escaped property names.
Each forbidden property remains a separate assertion with its member name;
missing required properties and invalid property names also retain their names.
Offending values and their JSON types do not distinguish occurrences of the same
constraint. Concrete instance paths and validator messages remain in diagnostics.
Before counting, repeated reports of the same canonical constraint at the same
concrete position within one resource are deduplicated. Distinct positions and
resources retain multiplicity. Conflicting schema roles for the same occurrence
fail unresolved. Grouped `additionalItems` errors expand to one occurrence for
each forbidden tail member, using the validator's proven tuple limit.
Moving a homogeneous array element, renaming a permitted map member, or reaching
the same constraint through another reference route does not create a new violation.
Moving a failure between positional tuple/prefix slots does create a new violation.
Comparison is per canonical key as a multiset: payload counts above the applicable
control's count are new; the remaining occurrences are inherited.
The evaluated keyword route establishes instance-position provenance only; it is
never part of constraint identity. Unsupported or ambiguous correspondence fails
unresolved, even when other violations are proved, and cannot certify a matched
result. Grouped `unevaluatedItems` and `unevaluatedProperties` errors remain
unresolved: the validator does not expose enough concrete member provenance to
prove the occurrence identities. Unchanged resources undergo the same validation;
unknown or inexact defaults cannot establish a usable control by document equality.
Cached controls are bound to the render version and validator policy/bundle content.
Defaults establish context usability only. Payloads are compared independently
against each applicable context's own assertion multiset, with no unused defaults
allowances. A loosening fails when any applicable context exposes a new violation;
a tightening needs new rejection evidence in every applicable context.
Mixed tightening outcomes are unresolved. Explicit defaults contexts remain
applicable alongside non-default contexts, including deduplicated built-in origins.
Per-context Kubernetes evidence records each overlay and its new/inherited errors.
Inherited filename-extension lint errors do not hide a false rejection when the
tested render and controls remain usable and add no Kubernetes violation.
Changed diagnostics behind inherited lint errors are unresolved, not matched.
A candidate rejection with usable renders and no attributable lint/template failure
or proved new Kubernetes violation is a false rejection and fails; it cannot be rostered.

## Verdicts (live mode)

A **tightening** means the candidate rejects, whether the baseline accepted or
the candidate found a new reason. A **loosening** means the candidate accepts
where the baseline rejected. **A tightening is a direction, not a verdict.** It
is correct only if Helm or Kubernetes rejects the document too. A tightening of
something Helm renders cleanly is a false rejection, which is a regression.

| Verdict | Condition | Counted as |
|---|---|---|
| `Collapsed` | on Helm's exact document both profiles agree, with no new reason | dropped: not a flip (`screened_flips_collapsed`) |
| `TighteningMatchedHelmAbort` | candidate rejects, Helm aborts | matched |
| `TighteningMatchedKubernetesRejection` | candidate rejects, render adds a new Kubernetes violation beyond the defaults render | matched |
| `TighteningMatchedDeclaredTypesPolicy` | the candidate rejects, Helm renders with no proved Kubernetes violation, the candidate is the chart's `--declared-types=assert` regeneration, its `annotate` regeneration accepts the document outright, and every value the assert regeneration rejects is a path explain C1 reports unread; whatever the baseline rejects is irrelevant, and no failures are compared | matched, listed in `tightenings_attributed_to_declared_types`; never rostered |
| `TighteningMatchedLintRejection` | candidate rejects and schema-free lint identifies a semantic rejection attributable to the tested input versus usable defaults and context controls | matched, `tightenings_matched_lint_rejection` |
| `LooseningMatchedLintValidation` | candidate accepts, schema-free lint and render succeed, and there is no new Kubernetes violation or unresolved resource | matched, `loosenings_matched_lint_validation` |
| `CandidateAcceptsLintRejects` | candidate accepts while schema-free lint identifies an attributable semantic rejection | false acceptance, `Rejection::LintRejects`; requires exact roster handling |
| `LooseningMatchedKubernetesValidation` | candidate accepts, render valid, every changed resource decided | matched |
| `LooseningMatchedDefaultsViolations` | candidate accepts, and every violation is one the defaults render already carries (counted with multiplicity) | matched |
| `LooseningWithUncertainKubernetes` | candidate accepts, Helm renders, some new or changed resource has no decidable schema | must be listed **exactly** (same uncertainty strings, in order) in `KNOWN_UNDECIDED_ACCEPTANCES` |
| `CandidateAcceptsHelmAborts` / `CandidateAcceptsKubernetesRejects` | candidate accepts a document Helm or Kubernetes rejects; the baseline rejected it for reasons other than its defaults' | false acceptance, baseline `RejectsUnlikeItsDefaults`; must be on `KNOWN_FALSE_ACCEPTANCES` |
| `UninformativeBaselineFalseAcceptance` | same, but the baseline rejected the document with exactly the violations it has on the chart's own defaults, so the baseline is no evidence | false acceptance, baseline `RejectsItsDefaults`; must be on the roster |

Hard adjudication failures, reported together after the whole run:

- `tightening rejects a document Helm renders without a proved Kubernetes violation`:
  the render is uncertain.
- `tightening rejects a document whose render adds no Kubernetes violation to the defaults render`:
  a proven false rejection.
- `Helm rendered values it could not coalesce`: a harness invariant was broken.
- Per-chart preparation errors and probes whose values file cannot be composed.

Every failure message ends with `evidence=<dir>`.

## Evidence on disk

Each chart prints `Helm adjudication evidence: <dir>` and each flip prints
`HELM_FLIP <verdict>: evidence=<probe dir>`. You only see these lines with
`--no-capture`. Both directories are under `<target>/scratch/helm-schema/` and are swept once
the test process has exited and another test process starts. A failure's `evidence=<dir>`
has already been copied, with its prepared charts, under `<target>/evidence/helm-schema/`;
so have the cases of unlisted false or undecided acceptances and, when the flip count gate
fails, every adjudicated flip. The copied `prepared.json` names the charts by bundle-relative paths.

- `<dir>/prepared.json` holds the source chart, the prepared render and
  coalesce copies, the `--kube-version`, and render cacheability.
- `<dir>/probe-*/` holds `values.json` (the overlay), `coalesced.json`,
  `render.yaml`, `render.stderr`, `render.status`, `render.invocation.json`,
  and `schema-verdicts.json`. The last file contains both profiles'
  accept/reject and errors, `baseline_rejects_it_as_its_defaults`,
  `candidate_rejects_for_a_new_reason`, the Helm exit, new, inherited and
  uncertain Kubernetes errors, and per-acceptance-document verdicts.

Lint evidence also retains the three lint executions and invocation records,
rule-qualified diagnostics, exact document JSON, overlay origins, schema hashes,
and a document-specific `schema-verdicts.json` under the shared lint evidence.
The verdict evidence also records defaults/context controls and inherited diagnostics;
unusable controls and inherited-only rejection evidence are unresolved failures.
Successful control evidence keeps execution records and outputs without copying
complete chart trees.
Failure preservation recursively materializes chart and values inputs for nested
controls and decoders, deduplicating repeated input paths within each directory.
Saved records use relative paths, so replay does not depend on scratch directories
surviving.

Adjudicate a flip from these files. Never rebuild a probe document by splitting
the dotted probe label.

## Rosters

`KNOWN_FALSE_ACCEPTANCES` groups rows by chart, `Rejection` (`HelmAborts` /
`KubernetesRejects` / `LintRejects`), `Baseline` (`RejectsItsDefaults` /
`RejectsUnlikeItsDefaults`) and suspected defect `Family`. Each row is a
`Probe { path, value }`. It matches a case whose probe name is
`<path> <- <value>`, or a targeted guard probe ending in
`[targeted: <path> <- <value>]`. `KNOWN_UNDECIDED_ACCEPTANCES` rows carry the
exact `uncertain` strings.

Document/context probe names instead bind the document kind, canonical complete
JSON overlay, transport, and Kubernetes version:
`document=<kind> transport=values-file kube=<version> overlay=<json>`.
Legacy path/class selectors do not authorize these names.
Add explicit `ContextSelector` rows to `KNOWN_CONTEXT_FALSE_ACCEPTANCES` or
`KNOWN_CONTEXT_UNDECIDED_ACCEPTANCES`, with the full document kind, JSON overlay,
`ContextTransport::ValuesFile`, and Kubernetes version. Each row records one complete context.
False-acceptance rows also bind rejection, baseline class, and family; undecided
rows bind the exact ordered uncertainty reasons. Stale contextual rows are checked
only when that complete identity was actually examined.
Do not invent a dotted-path selector for a complete context or treat a legacy
roster row as permission for a new lint false acceptance.
False rejections are never rosterable, and this lane does not change
`ROSTER_BASELINE` or `SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS`.

For every chart the live run adjudicated, which is every screened chart, the
run fails on:

- an observed false or undecided acceptance that no row lists ("missing from
  KNOWN_FALSE_ACCEPTANCES" or "missing from KNOWN_UNDECIDED_ACCEPTANCES");
- a row that was not observed failing alike ("roster entries that no longer fail
  alike; remove or re-adjudicate them"). This check is skipped when the row's
  probe is unreachable: the run prints `UNOBSERVABLE roster row …` and keeps the
  row.

Rows of charts that were not screened, for example under
`SCHEMA_ACCEPTANCE_CHART`, are not checked.

Editing rules:

- **Removing a row** is right only when your change really fixed the defect.
  First confirm in the evidence that Helm or Kubernetes still rejects the
  document and that the candidate now rejects it. If instead the whole roster
  "no longer fails", suspect the inputs: the wrong baseline, a stale dump, or
  a candidate identical to the baseline.
- **Adding a row** is a finding, not a fix. It means the analyzer accepts a
  document Helm or Kubernetes rejects. Add it only with the adjudication, from
  the probe's `schema-verdicts.json` and Helm stderr, and with the suspected
  family. Use `Family::Unfiled` when no family fits. Behind a baseline that
  rejects unlike its defaults, also record why the baseline rejected the cell.
  A new row caused by your own change is a regression. Fix the change instead
  of listing the row.
- An undecided row needs the adjudicator's exact uncertainty text and a comment
  explaining why the pinned evidence cannot decide the resource.

## Exact inherited hard failures

`common/known_inherited_failures.rs` holds `KNOWN_INHERITED_FAILURES`, the
shrink-only list of hard failures inherited by landing 10a (`c23467f4`).
Its contextual rows reuse `ContextSelector`; legacy template rows reuse `Probe`
and additionally bind the exact Kubernetes version and values-file transport.
Every row binds chart, complete lane/overlay or exact probe selector, failure
class and exact reason, plus `owner` (repair track) and `observed_on`.
No chart-wide, class-wide, targeted-probe suffix, or overlay wildcard is allowed.
The identity also binds the original values-file SHA256 and a SHA256 of the
ordered applicable control inputs plus the defaults control.
Byte-distinct encodings remain separate entries even when their parsed overlay
and display case are identical; each entry must recur independently.
Repeated observations share an entry only when all these inputs match.

Contextual failures also bind a digest of the complete real Helm lint error
blocks for baseline, candidate, schema-free execution, and executed controls.
Normalization removes invocation headers, summaries, blank lines, and trailing
whitespace, but retains rule headers and every diagnostic continuation line.
JSON Schema error trees sort siblings because Go map iteration can reorder them;
parent/child relationships, exact messages and duplicate multiplicity remain bound.
Other diagnostic text retains its order.
In particular, a metaschema error binds its failing keyword and pattern/message,
not only the generic Rust/Helm disagreement or the first lint error line.
The live matcher verifies the saved original values bytes as well as the digest.
Legacy template failures without lint executions bind their exact specific
Kubernetes-control reason and original input/control hashes.

A live round-74 run must match every adjudication failure exactly and observe
every entry, including entries for charts omitted by a chart filter.
An unlisted failure, changed reason, duplicate entry, missing ownership, or stale
entry fails; even an unexamined or unreachable entry is stale.
Removing an entry requires proving the defect fixed or the oracle corrected.
Later landings may only remove entries, never add or broaden one to admit a new
failure.
Schema-only preservation runs gain no allowance from this list.

The CRD-v1 not-found oracle false alarms bind the exact false-acceptance
identity, rejection/baseline class, and complete ordered Kubernetes errors.
They remain visible in raw false-acceptance coverage, but count separately in
the summary and never authorize a false-acceptance roster row.
They require real saved adjudication evidence: a matching case alone cannot
excuse a different Kubernetes rejection or missing evidence.
An unusable defaults/context control is an adjudication failure, never a
roster-eligible undecided acceptance.
Genuine inherited false acceptances and accepted undecided resources still
require the existing contextual rosters and their evidence rules.
The list changes neither `ROSTER_BASELINE` nor the matched-flip count policy.

## Why the roster baseline is pinned, and how to move it

A roster row is defined as a flip against `ROSTER_BASELINE`: a cell that
baseline rejected and the candidate accepts. Against any other baseline the
row is not screened at all, so the row would look "fixed". A baseline equal to
the candidate screens no flips, so every row would look fixed. The live
battery therefore refuses any other baseline up front. `roster_baseline_problem`
prints the command to use instead.

Moving the baseline is a deliberate design decision for the user. It is never
a side effect of a landing. It requires:

1. Changing `ROSTER_BASELINE`. Also update the example sha in
   `a_live_battery_needs_the_roster_baseline` if it equals the new value.
2. Running the live battery over the whole corpus against the new baseline,
   with one clean dump of the new candidate.
3. Re-deriving both rosters from the observed outcomes, each row re-adjudicated
   from evidence with a family.
4. Giving the rows that become unobservable a separate home. These are false
   acceptances the new baseline already shares, so they are no longer flips.
   Their Helm and Kubernetes rejections are absolute facts, not relative ones,
   and dropping them silently would lose known defects.
5. Checking that every current corpus fixture exists at the new baseline. The
   harness has no skip for missing fixtures, so `git show` fails the run for a
   chart the baseline predates.

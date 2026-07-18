# Chart-corpus findings: status ledger

Last reconciled 2026-07-18 (ninth round) after auditing the F105 airflow
`labels` string-typing claim. Green corpus tests are a
baseline, not completion evidence: the reopening pass verified claims with
concrete opposite-polarity probes, and the recent rounds implemented the ones
that survived verification, re-adjudicating the ones whose root cause turned
out to be a deliberate policy. Where a finding has both a completed bounded
part and a remainder, the completed part is listed below with a "(bounded)"
marker and the residual is classified separately. Per-finding history lives
in `chart-corpus-expansion.md`.

## Completed

Fixed on the current tree and pinned by tests (corpus fixtures,
`chart_reaudit` cases, or focused gen/IR reproducers):

- F1 dotted values keys split into fabricated nested paths
- F2 guarded overlays closing objects to the observed member subset
- F3 self-truthy-guarded typed leaves keeping value facets unconditionally
- F4 stringification sinks typing scalars as `[string, null]`
- F5 null-declared default plus guarded use pinning `type: null`
- F6 structural shape alternatives collapsing to one shape
- F7 `tpl X $ctx` context argument bleeding into the value's type
- F8 `with`-scoped map splice taking the manifest position's schema
- F9 undeclared values via `tpl (toYaml …)` guessed as objects
- F10 whole-CRD subtree inlining per overlay arm (size pathology)
- F11 longhorn performance outlier
- F13/F15 literal member probes closing helper-ranged declared-empty maps
- F14 `$defs` substitution discarding processed branch schemas (the exact
  downstream chart revision is gone; the structural regression is fixed)
- F16 corpus fixtures leaking the developer's CRD catalog cache
- F17 stringification transfer functions rejecting values Helm accepts
- F18 shape-erasing uses deleting independent strict uses
- F19 `printf` conflating the format parameter with data parameters
- F20 local-guard runtime contracts binding path-wide (loki `kindIs` arm)
- F21 guarded `range` domains
- F22 numeric casts modeled as identity
- F23 `typeOf` dispatch losing string-versus-structured alternatives
- F24 total-stringification facts lost in guard-only paths
- F25 direct `typeIs` Go container names
- F26 guarded `range` rejecting rangeable integers
- F27 compound document guards dropping chart-level string contracts
- F28 type-validation guards and `fail` branches as schema evidence (bounded;
  range-local terminal implications remain In progress)
- F29 condition transform collection ignoring pipeline order
- F30 Helm `required` termination as schema evidence (incl. dynamic
  `extraEnvConfigMaps` members)
- F32 cross-path Boolean `fail` implications
- F33 finite `.Files.Get (printf …)` selectors
- F34 literal-key `dig` navigation
- F35 helper-computed type alternatives behind declared defaults
- F36 executing catch-all branches losing structural requirements
- F37 nested type dispatch leaking provider typing across siblings
- F39 integer range widening ignoring loop-body requirements
- F40 nested range requirements through ranged locals
- F41 `with`-rebound dot losing the originating path in type dispatch
- F42 `default`/`coalesce`-guarded contracts (direct, helper-boundary, and
  fallback-selection scope; Helm-falsy escapes stay open)
- F43 range-derived union alternatives bypassing shape requirements
- F44 key-predicate contracts on dynamic map values (bounded; ranged-member
  conjunctions and dynamic key domains remain In progress)
- F45 string-only call effects (bounded; incl. `substr`, `htpasswd`; strict
  hash operands remain In progress)
- F46 empty-map/observed-subset defaults closing passthrough objects
- F47 secretKeyRef/configMapKeyRef closing to name-only
- F48 list-valued paths typed or closed as objects
- F49 int-or-string scalar flags (PDB percentage with provider)
- F50 string-form alternatives and declared-null values
- F52 Helm-executed `NOTES.txt` analysis
- F53 `tpl` contracts in helpers, registry/default chains, range-key
  equality members
- F54 type-dispatch overlays making supported arms impossible
- F55 partial type dispatch re-closing the unmatched complement
- F56 generic fragment fallback vs structural placement (jaeger,
  CloudNativePG, airflow, CoreDNS ranged items)
- F57 broad fragment alternatives bypassing member/range contracts
- F58 integer rangeability vs range-variable arity (jenkins hasKey/member
  slot degradation)
- F59 range-body requirements reaching iterable lanes (direct ranged
  members; velero schedules via `additionalProperties`)
- F60 `eq`/`ne` operand domains incl. missing/null tolerance
- F61 strict collection-call signatures for audited functions (bounded;
  newly audited hash calls remain In progress and the unknown long tail is
  Rejected)
- F62 opening empty declared containers erasing the container type
- F63 chained member reads requiring intermediate members (incl.
  header-member ordering)
- F64 dead-branch strict contracts under unlowerable guards, completed by
  the exact semver comparator-to-regex arm (airflow base_url)
- F65 ordered helper mutation in accepted input domains
- F66 runtime consumer domains scoped by call execution
- F67 integer rangeability across JSON roundtrips
- F69 range/member projections escaping live outer guards
- F70 `index` access preconditions — literal indices and literal split
  positions (bounded; dynamic cross-path remainder is Rejected)
- F73 statically selected file-backed template programs (`.Files.Get`
  programs, BasePath partials)
- F74 strict parser lexical domains — semver/duration/URL catalog,
  conditional literal reassignment, lexical escape tokens (bounded; parser
  range/authority checks and derived tag preimages remain In progress)
- F75 shape erasure through `first`/`last`/`initial`/`rest`/`compact` and
  audited nested member paths (bounded; dynamic `slice`/opaque identities
  are Rejected)
- F76 YAML scalar lexical safety (bounded): plain-token exclusions with class-aware
  allowances, numeric-grammar exclusions, double/single-quoted content,
  flow style, mapping keys, completed-token contracts, composite-in-quotes
  recursive serialization preimage (F76.2), and empty-scalar defaults under
  member projection; resolver-token coverage remains In progress
- F77 `and`/`or` selected-operand values
- F78 value-selecting functions keeping candidate-selection predicates
- F79 `break`/`continue` suppressing later-iteration contracts
- F80 ordered `merge`/`mergeOverwrite` layers with per-key shadowing arms
  (the direct Velero provider-splice case; the airflow recursive
  custom-merge lane landed in the eighth round — see its entry below;
  the worker-family provider-typing residual stays In progress)
- F81 Sprig arithmetic coercion boundary
- F82 chart-authored `values.yaml` programs executed by `tpl`
- F84 split-segment provider preimage for integer slots (bounded; general
  numeric enum/range projection is Rejected)
- F86 strict Boolean call signatures incl. architecture partitions and
  `IntGt` sound subsets
- F87 builtin signatures constraining nested collection element kinds
  (bounded; element parser domains remain In progress)
- F88 derived literal-membership and `typeOf`→`regexMatch` dispatch guards
  (bounded; provider intersection on the selected lane remains In progress)
- F89 statically constructed finite `tpl` programs
- F90 caller predicates over mutually exclusive helper-return alternatives
- F91 parenthesized nil-safe selectors and receiver members
- F92 synthetic helper-dict field provenance identities
- F94 reflect `invalid` kind as presence/nullability predicate
- F96 header-condition string contracts (null override coalesces to
  absence — renamed accordingly)
- F97 niladic methods on typed Helm objects
- F98 provider-required output fields requiring source leaves
- F99 finite literal `fromYaml` path programs (traversal interpreter)
- F100 post-`tpl` regex requirements on raw template programs
- F101 provider availability as a committed deterministic test input
  (`testdata/provider-bundle/`, cold/warm equivalence)
- F102 bitnami-redis locked `common` dependency vendored plus a `Chart.lock`
  presence gate (bounded; legacy locks and unpacked-version verification remain
  In progress)
- F103 test compositors scrubbing nulls only along map chains
- F104 `$tplYaml` program-wrapper alternatives at value nodes, completed by
  wrapper RESULT compatibility (seventh round): a replace program's static
  decoding must inhabit the node's accepted kinds (certainly-incompatible
  lexeme classes reject; dynamic programs stay open), spread programs must
  decode to the parent collection's kind (scalars always abort; the values
  root refuses the spread wrapper), sentinels are classified structurally
  (a `hasKey`-guarded `fail` marks the spread form), and a singleton
  sentinel map never rides a node's ordinary object domain — the engine
  intercepts it before consumers see it

- F31 scalar-domain fail implications (bounded): `len`, literal membership,
  semver-minimum, and raw-integer subsets are lowered; direct/local
  `int`/`int64` provenance covers Jenkins' integer 0..=1 lane. Coerced-string
  preimages remain In progress.
- F51 existential range sentinels (bounded): branch joins stamp arm conditions onto
  changed truthiness reductions, the joined
  `Range ∧ member-Eq` flag lowers as `ConditionalGuard::
  ContainsMemberEquals` (`contains` on the array lane, the double-negated
  member quantifier on the object lane), and terminal clauses admit
  approximate conjuncts through their sound subsets (airflow's celery
  broker sentinel). General terminals inside ranges remain In progress.
- F68 range-key slot domains: a raw range key rendered at a provider slot
  rides a marked splice (`range_key`) whose collection gains a
  keys-must-be-strings arm when the slot is string-only — non-empty lists
  excluded, maps and empty lists open (minio `environment`, and the
  `extraObjects`-family arms across the corpus)
- F71 optional-dependency helper availability: unconditional include
  closures over define bodies plus define ownership by chart directory
  yield terminal clauses for the inactive states of an optional
  dependency that solely owns a live helper (bitnami-postgresql's
  `tags.bitnami-common`, scoped by the including chart's own activation —
  the airflow postgresql counter-pin)
- F93 same-map member identity through `keys | sortAlpha | pluck | first`
  and member-local type partitions (bounded and gen-pinned; the representable
  SigNoz singleton lane remains In progress, while general dedup correlation
  is Rejected)
- NATS direct `extraResources` member kinds (bounded): a ranged member spliced as a whole
  document at column zero must be an object when present and non-null
  (Helm decodes every manifest as a mapping). Program-wrapper bypasses remain
  In progress under F104.
- F83/F85 inline-local kind partition: an inline-conditional `kind:`
  chain records per-arm guard sources (detector), the evaluator lowers
  them through the live scope into `KindBranch` predicates on the
  per-use `ResourceRef`, and the builder concretizes each row's kind
  when its conjunction entails exactly one arm — with exact `has X
  (list <scalar literals>)` membership and reduction-backed `not $var`
  lowering as load-bearing collateral (airflow scheduler
  strategy/updateStrategy per-arm provider scoping incl. dead-arm
  tolerance; a StatefulSet/DaemonSet shared-slot gen pin discriminates
  the concretization from pointer-miss fallback)
- F76 resolver tokens (sixth round): the numeric/Boolean token grammars are
  now derived from go-yaml v2's `resolve()` — underscore stripping, signs,
  radix prefixes, trailing-dot floats, the exact signed-infinity/unsigned-NaN
  table, and float-overflow fallback to string — symmetrically for
  string-slot exclusions and int/number/bool-slot accept preimages
  (external-dns `"1_000"` now rejected in a string slot; metrics-server
  `"+443"` and crossplane `"yes"` now accepted)
- F102 dependency-integrity gate: Helm-v2 `requirements.lock` is discovered
  (datadog was previously entirely unchecked) and unpacked dependency
  directories must record the locked version in their own `Chart.yaml`
- F88 provider intersection on kind-dispatched lanes: a "number" type
  partition over an integer-allowing branch no longer unions `{type:
  number}` into the arm — draft-07 `integer` accepts integral floats, so
  the arm stays satisfiable while fractional floats reject (sealed-secrets'
  `typeOf`-dispatched policy/v1 `minAvailable` rejects `1.5`, keeps `2.0`
  and `"50%"`)
- F87 nested parser domains: `genSignedCert`/`genSelfSignedCert` ip-list
  items carry an IP lexical domain (exact dotted-quad IPv4 plus an IPv6
  textual superset) through a new per-item pattern channel on the
  collection-items capture (cilium's Hubble SANs reject `"not-an-ip"`)
- F45/F61 strict hash operands: the checksum family (`sha1sum`,
  `sha256sum`, `sha512sum`, `adler32sum`) is catalogued as a strict
  Go-string consumer with unknown-call value semantics (an
  `include … | sha256sum` annotation keeps its serialized attribution);
  the effect survives ranged-member `default ""` selection via a
  truthy-scoped member requirement, outer branch guards via
  fail-polarity strengthened decoding, and the `if (include …)`
  document gate via literal-dispatch include-truthiness (bitnami-redis
  ACL passwords)
- F28/F51/F44 ranged terminals (sixth round): member truthiness lowers as a
  `HelmTruthy` member requirement (sealed-secrets rejects empty-string
  `privateKeyAnnotations` members), member name equalities negate to
  `NotEquals` requirements on the member field (cilium rejects backoff
  env-name collisions while the feature is live), and range-KEY regex
  terminals lower to `propertyNames` through the new `RangeKeyMatches`
  guard (traefik rejects uppercase `ingressRoute` keys)
- F31 decimal coercion preimages (bounded): `IntGt`/`IntLt` encodings carry
  digit-wise decimal string preimages (clean spellings only — a leading
  zero flips `ParseInt` to octal and abstains), and declared-default
  evaluation reads decimal string defaults (jenkins rejects `"5"` and
  `"-1"` replicas beside the raw integers)
- Include-truthiness condition decoding: a bare `include "name" .` in
  condition position decodes through the helper's literal dispatch when
  every arm renders static text (including bare literal outputs like
  `{{- true -}}`), with whitespace-ambiguous arms abstaining — document
  gates like `if (include "redis.createConfigmap" .)` now lower exactly
  instead of degrading to an undecodable marker

- F104 wrapper result compatibility (seventh round): see the F104 entry
  above; pinned by `wrapper_program_results_must_be_compatible_with_node_
  and_parent` (gen) and `nats_wrapper_results_must_be_compatible_with_
  their_sinks` (CLI), every polarity verified under `helm template`
- F93 singleton `additionalEnvs` (seventh round): a first-iteration-provable
  dedup guard (`not (hasKey $acc …)` over an accumulator that is a provably
  empty dict, in a single-depth loop) carries `Guard::AtMostOneMember` as a
  sound subset; row conditions substitute such subsets (fires-less-often is
  safe for positive rows), and the member-wildcard `if`-side encodes as the
  ∀-member quantification — signoz now rejects a singleton
  `additionalEnvs: {AUDIT: {value: 7}}` while the case-colliding shadowed
  multi-key map stays open (kubeconform-verified both ways)
- F80 external-secrets guard-scoped `omit` (seventh round): `omit` on a
  values-backed map records removed keys as an effect riding the binding
  meta; the branch join fills sound RETAIN guards from the omitting arm's
  header negation (now including `or`-headers, one negated equality per
  disjunct); the provider payload subtracts omitted members and re-adds
  each key as a root-anchored arm under branch + retain guards. With the
  new exact `Guard::MinMembers` decode for `gt (keys . | len) N`, the real
  chart now accepts a string `runAsUser` under `adaptSecurityContext:
  force`/`auto` and rejects it under `disabled` with a live render gate —
  all polarities verified with `helm template --skip-schema-validation` +
  kubeconform (the chart's shipped `values.schema.json` is deliberately
  not evidence)
- F28/F51 oauth2-proxy legacy `extraPaths` (seventh round, re-adjudicated):
  the ground truth is the chart's own `deprecation.yaml` `fail`, not a
  provider splice — helm aborts when a legacy `backend.serviceName/
  servicePort` is set while `capabilities.ingress.apiVersion` resolves
  `networking.k8s.io/v1`. Member-field truthiness now negates to the new
  `FieldHelmFalsy` requirement, and the capability equality lowers through
  a sound subset flipping the dispatch's `semverCompare "<C"` bounds into
  `>=C` kubeVersion patterns (literal dispatch now reads `{{- print … -}}`
  arms and trim-marker delimiter tokens). Pinned kubeVersions reject the
  legacy shape, the v1beta1 lane keeps it, and the unpinned
  capability-dependent lane soundly abstains
- F80 airflow recursive `workersMergeValues` lane (eighth round): the three
  diagnosed gaps landed. (a) `IrAnalysisDb::custom_merge_helper` recognizes
  the bounded recursive-merge define shape (list-indexed map params, empty
  `dict` accumulator, `has`-probed literal full-overwrite list, ranges only
  over the two maps, member-disciplined `set` values, self-recursion,
  `toYaml ACC` terminal) and the call site substitutes
  `MergedLayers([overwrite, input])` marked YAML-serialized so
  `include … | fromYaml` round-trips; (b) `set $copy.Values KEY V` on a
  `deepCopy`-of-root local is observed as a values-member overlay on the
  local, document-scope assignments apply that context-copy mutation, and
  `.Values.…` fields resolve through a `with`-dot whose `Values` member was
  replaced; (c) strict-operand captures walk merge layers in order — each
  layer truthy-scoped (the merged value exists regardless of any one layer,
  so no layer's presence may be demanded), deeper layers additionally
  conditioned on the earlier layers' absence, opaque layers blocking
  everything below them (`MergedLayers` member projection now keeps an
  opaque layer as an unknown shadow instead of dropping it). Collateral:
  document-scope ranges over structured/joined iterables bind their item
  variable to the member domain, and fail-polarity `Or` guards drop
  undecodable disjuncts instead of vetoing the arm. The real chart now
  rejects scalar `workers.celery.sets[].labels` and
  `workers.celery.sets[].persistence` while map-shaped per-set overrides
  stay open — every polarity verified under `helm template`; pinned by
  `airflow_worker_set_overrides_bind_strict_member_kinds` (CLI) and the
  recognizer tests (IR)
- F105 airflow root `labels` string-typed under the connection-secret
  conditions (ninth round): the producer was the checksum lane — the
  `include (print $.Template.BasePath …) . | sha256sum` annotations render
  each secret/configmap template through a bound-helper summary whose
  `labels` flow keeps its `with`-branch meta, and the widened digest value
  re-lowered that influence path into a guarded splice at the annotation
  slot, where the summary's `yaml_serialized` mark promoted the row to
  `YamlSerialized` and provider typing read the Deployment's
  annotation-value schema (string). Three lowerings landed: (a) a widened
  transform's guarded arms at a SCALAR slot become DIGEST rows for
  derived-text paths that are neither shape-erased nor encoded — the row
  lowers as `Serialized` (no provider or metadata typing) and the builder
  splits its facts so the BRANCH keeps serialized tolerance (grafana's
  checksum'd `datasources` overlay must not re-type through the declared
  default) while the PATH gains no serialization use (which would hand
  the base resolution to the serialization owner); Fragment slots
  (`include … | nindent` locals) keep their payload-carrying rows;
  (b) `contract_use_base_cmp` includes `merge_layers` and `digest` in the
  render-site identity so a marked row no longer folds into a plain row
  at the same site and mis-attributes its disjuncts; (c) merge-layer
  identities require the layer to BE a path identity (also through
  `Choice` arms and nested `MergedLayers` lineage, with pathless literal
  off-states) — a constructed dict merely referencing one path
  (external-dns's `merge $defaultSelector .podAffinityTerm` selector
  built from `nameOverride`, bitnami's `common.labels.standard`) no
  longer keys the merge shadow on the referenced path. The real chart now
  accepts `labels: {team: data}` (helm renders) while a truthy scalar
  still rejects (the `mustMerge` sites abort — helm-verified); grafana's
  `datasources`/`notifiers`/`dashboardProviders` keep accepting
  null/empty (helm renders; the falsy family joins F106 for airflow's
  `labels`). Pinned by
  `airflow_checksum_annotations_do_not_string_type_root_labels` (CLI);
  21 corpus fixtures, 2 gen fixtures, and 3 IR fixtures regenerated with
  a per-chart old-versus-new acceptance probe showing no tightenings at
  the top-level key domain

## In progress

- **F31 residual — non-decimal coercion preimages.** Clean decimal spellings
  now reject through the `IntGt`/`IntLt` string preimages (jenkins `"5"` and
  `"-1"`), with declared-default evaluation extended to decimal strings.
  Radix-prefixed and leading-zero spellings (`ParseInt` base detection reads
  them as hex/octal), mixed-sign bound regions (a positive `IntLt` bound),
  and cilium/traefik's not-yet-lowered `ge`/`le`/`gt`-chain comparators stay
  open.
- **F74 residual — duration/URL exactness and the datadog tag domain.**
  Semver core components are now bounded at 20 digits (21+ certainly
  overflow `ParseUint` and abort) while staying a superset of the accepted
  language. Still open: `time.ParseDuration` overflow (value-dependent per
  unit), exact URL authority validation, and datadog's
  `toString | trimSuffix "-jmx"` tag domain — the latter needs the semver
  comparator preimage to flow through derived-text subjects with lexical
  escapes.
- **F106 — airflow root `labels` Helm-falsy scalars at the base
  (false rejection, partially pre-existing).** Uncovered by the F105
  audit: `labels: ""` and `labels: []` were already rejected by the base
  typing, and with the digest rows no longer provider-typed the previous
  fixtures' incidental `labels: null` acceptance is gone too, while
  `helm template` renders all three (every `with` guard skips a falsy
  value, and the `if or .Values.labels .Values.X.labels` `mustMerge`
  sites tolerate falsy operands — sprig merge no-ops them; all
  helm-verified). The base falsy escape is blocked because the or-guarded
  `mustMerge` rows are not self-guarded renders; treating merge-layer
  rows as self-guarded by construction was attempted and reverted (it let
  the declared-default array typing leak into the self-guarded `sets`
  overlay branch and regressed the F80 map-shaped `sets` pins). The old
  null arm was itself fallout of the conditional-target base assembly
  under the buggy string overlays, not a nullability grant. Reopening
  needs either a falsy-tolerance catalog fact for merge operand rows that
  does not reroute the overlay's declared-default merge, or the F80
  merge-aware candidate decoding below.
- **F80 residual — worker-family provider typing under the merged
  context.** With `.Values.workers` resolving through the per-set merge,
  the `airflowPodSecurityContext` priority-chain decoding cannot scope the
  layered candidate exactly, so `workers.securityContext` abstains from
  provider claims entirely (previously an overlay-guarded use; the
  scheduler family keeps the exact break-scoped overlay). Sound but
  incomplete; re-tightening needs merge-aware candidate-priority decoding.

## Rejected (invalid or won't fix by design)

Closed without (further) implementation. Reopening any of these needs new
evidence or a model extension, not more of the same analysis.

- **F12 — strict-mode policy adjudications.** Dead/misplaced CI keys
  (datadog, grafana typo) stay rejected by design; the root `global`
  namespace stays open by policy; dynamic-`tpl`-only key introductions
  remain a documented strict-mode limitation.
- **F38/F72/F95 — input-channel numeric kinds.** One Draft-07 instance
  cannot accept Helm's `--set` int64 channel while rejecting the
  values-file float64 channel for the same JSON number (istiod
  `certSigners`, CoreDNS zero/negative `servers`). The analyzer emits the
  explicit `InputChannelNumericRangeAmbiguity` diagnostic instead of
  presenting a channel-dependent answer as exact. The structural parts
  (rangeability, arity, zero-iteration domains) are Completed.
- **F70 remainder — dynamic cross-path index cardinality.** A
  `length(source) > index` relation where the index comes from another
  path or a loop is relational and not expressible as an ordinary Draft-07
  property schema; literal cases are Completed.
- **F75 remainder — dynamic collection projections.** Dynamic `slice`
  bounds and identities hidden behind opaque locals/helpers intentionally
  abstain.
- **F61 remainder — uncatalogued call long tail.** Unknown Sprig/Helm
  functions abstain; treating every unknown call as strict (or copying
  output types onto operands) would recreate the false-rejection classes
  this plan removed. Audited functions get catalogued as audits surface
  them; the newly audited checksum family is In progress above.
- **F84 remainder — general substring preimages.** Projecting an arbitrary
  provider numeric enum/range onto the nth substring of a raw string is
  not faithfully encodable as a Draft-07 regex once signs, bases, and
  coercion are involved; the integer-slot subset is Completed.
- **F93 remainder — cross-map dynamic key correlation.** Draft-07 cannot
  correlate one dynamic property name across two independent maps; only
  same-map bounded projections are representable.
- **SigNoz `additionalEnvs` member constraints — relational member set.**
  The chart's `renderAdditionalEnv` gates every render on a case-folding
  dedup accumulator: a member can be SHADOWED by an earlier
  case-colliding key and never render, so a blanket per-member EnvVar
  constraint would falsely reject `{audit: {value: 7}, AUDIT: …}`. The
  schema soundly keeps the members open
  (`signoz_additional_env_members_stay_open_under_dedup_shadowing` pins
  the shadowed-member acceptance). General multi-member correlation stays
  Rejected; the representable singleton lane is In progress above.
- **F80 kyverno scalar-shadow lane — declared-default policy.** The audit's
  false rejection of a scalar `features.logging` shadowed by every
  controller's `featuresOverride.logging` is real but does not originate in
  the merge analysis: the rejection comes from the declared-default object
  typing of `values.yaml` (the composed-defaults evidence channel). Making
  it conditional would need a root-level relational arm over all four
  controllers' override presence — representable but disproportionate for a
  lane that requires deliberately overriding a declared map with a scalar
  AND shadowing it everywhere. The merged-member projection fix (layered
  precedence through `pick`/`deepCopy`) landed; the declared-shape typing
  stays as policy, like the F12 strict-mode adjudications.
- **Adjudicated-wrong audit claims.** AWS LBC `nameOverride: "null"`:
  rendering yields a null label value that the strict v1.35.0 schemas
  reject on every resource, so the plain-token exclusion is correct.
  SigNoz zookeeper printf pin: helm aborts on a non-string
  `clickhouse.zookeeper.nameOverride` inside Sprig `contains`, so the
  operand-abstention pin was wrong and was replaced by the branch-scope
  pin.

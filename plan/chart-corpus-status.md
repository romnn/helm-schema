# Chart-corpus findings: status ledger

Last reconciled 2026-07-19 after two concurrent rounds: a fresh
chart-source versus generated-schema audit (tenth round — every reopened or
new item below has a concrete bad case plus a good Helm/provider control)
and the open-items implementation round (eleventh round — F106 implemented,
the F31/F74 comparator/duration halves landed, the F80 residual attributed
to two named machinery gaps). Green corpus tests are a baseline, not
completion evidence. Where a finding has both a completed bounded part and a
remainder, the completed part is listed below with a "(bounded)" marker and
the residual is classified separately. Per-finding history lives in
`chart-corpus-expansion.md`.

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
  (bounded; total-`toString` literal preimages remain In progress)
- F18 shape-erasing uses deleting independent strict uses
- F19 `printf` conflating the format parameter with data parameters
- F20 local-guard runtime contracts binding path-wide (loki `kindIs` arm)
- F21 guarded `range` domains
- F22 numeric casts modeled as identity
- F23 `typeOf` dispatch losing string-versus-structured alternatives
- F24 total-stringification facts lost in guard-only paths (bounded; terminal
  truthiness over the derived string remains In progress)
- F25 direct `typeIs` Go container names
- F26 guarded `range` rejecting rangeable integers
- F27 compound document guards dropping chart-level string contracts
- F28 type-validation guards and `fail` branches as schema evidence (bounded;
  range-local terminal implications remain In progress)
- F29 condition transform collection ignoring pipeline order
- F30 Helm `required` termination as schema evidence (incl. dynamic
  `extraEnvConfigMaps` members)
- F32 cross-path Boolean `fail` implications (bounded; nested/defaulted and
  derived-text implications remain In progress)
- F33 finite `.Files.Get (printf …)` selectors
- F34 literal-key `dig` navigation
- F35 helper-computed type alternatives behind declared defaults
- F36 executing catch-all branches losing structural requirements
- F37 nested type dispatch leaking provider typing across siblings
- F39 integer range widening ignoring loop-body requirements
- F40 nested range requirements through ranged locals
- F41 `with`-rebound dot losing the originating path in type dispatch
- F42 `default`/`coalesce`-guarded contracts (direct, helper-boundary, and
  fallback-selection scope)
- F43 range-derived union alternatives bypassing shape requirements
- F44 key-predicate contracts on dynamic map values, including audited
  ranged-member conjunctions and range-key domains
- F45 string-only call effects (incl. `substr`, `htpasswd`, and the audited
  checksum family)
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
- F56 generic fragment fallback vs structural placement (bounded; provider
  evidence leaking through scalar text fragments remains In progress)
- F57 broad fragment alternatives bypassing member/range contracts
- F58 integer rangeability vs range-variable arity (jenkins hasKey/member
  slot degradation)
- F59 range-body requirements reaching iterable lanes (direct ranged
  members; velero schedules via `additionalProperties`)
- F60 `eq`/`ne` operand domains incl. missing/null tolerance
- F61 strict collection-call signatures for audited functions (the unknown
  long tail is Rejected)
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
- F76 YAML scalar lexical safety: plain-token exclusions with class-aware
  allowances, numeric-grammar exclusions, double/single-quoted content,
  flow style, mapping keys, completed-token contracts, composite-in-quotes
  recursive serialization preimage (F76.2), empty-scalar defaults under
  member projection, and go-yaml v2 resolver-token coverage
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
  (bounded; exact IPv6 parser domains remain In progress)
- F88 derived literal-membership and `typeOf`→`regexMatch` dispatch guards,
  including provider intersection on the selected lane
- F89 statically constructed finite `tpl` programs
- F90 caller predicates over mutually exclusive helper-return alternatives
- F91 parenthesized nil-safe selectors and receiver members
- F92 synthetic helper-dict field provenance identities
- F94 reflect `invalid` kind as presence/nullability predicate
- F96 header-condition string contracts (null override coalesces to
  absence — renamed accordingly)
- F97 niladic methods on typed Helm objects
- F98 provider-required output fields requiring source leaves (bounded;
  ranged array/map member leaves remain In progress)
- F99 finite literal `fromYaml` path programs (traversal interpreter)
- F100 post-`tpl` regex requirements on raw template programs
- F101 provider availability as a committed deterministic test input
  (`testdata/provider-bundle/`, cold/warm equivalence)
- F102 bitnami-redis locked `common` dependency vendored plus legacy-lock and
  unpacked-version verification (bounded; recursive nested-lock discovery
  remains In progress)
- F103 test compositors scrubbing nulls only along map chains
- F104 `$tplYaml` program-wrapper alternatives at value nodes (bounded):
  wrapper RESULT compatibility (seventh round): a replace program's static
  decoding must inhabit the node's accepted kinds (certainly-incompatible
  lexeme classes reject; dynamic programs stay open), spread programs must
  decode to the parent collection's kind (scalars always abort; the values
  root refuses the spread wrapper), sentinels are classified structurally
  (a `hasKey`-guarded `fail` marks the spread form), and a singleton
  sentinel map does not ride a node's ordinary post-rewrite object domain.
  Consumers that execute before the rewrite remain In progress.

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
- F93 same-map member identity through `keys | sortAlpha | pluck | first`,
  member-local type partitions, and the representable SigNoz singleton lane
  (general dedup correlation is Rejected)
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
- F106 airflow falsy-family root `labels` (eleventh round): re-verification
  OVERTURNED the falsy-no-op note — helm 4's `merge`/`mustMerge` take
  typed `map[string]any` parameters, so a LIVE gate aborts on any non-map
  operand and the true domain is relational (falsy non-map `labels`
  renders iff every partner is falsy; all boundaries helm-verified). The
  or-gated `ValueType{object}` fail arms already carried the relational
  rejections; the missing base escape landed as: (a)
  `Effects::merge_operand_paths` marks each identity-bearing DIRECT merge
  operand (via the shared `AbstractValue::merge_layer_identity`
  discipline) through `SpliceMeta`/`ContractUse::merge_operand`, joining
  the render-site identity; (b) a new
  `all_render_uses_falsy_tolerant` fact — merge-operand and digest rows
  cannot reject a falsy input at the base — feeds ONLY the base falsy
  escape, gated on `!has_referenced_descendants`, never overlay routing
  or declared-default placement (the reverted attempt's leak). `""`/`[]`/
  `0`/`false` accept alone, truthy scalars reject, a truthy scheduler
  partner rejects the falsy combination exactly, and the workers-partner
  combination stays a documented widening (wildcard disjunct; see the F80
  residual). Pinned by
  `airflow_falsy_root_labels_render_while_live_merge_gates_bind`
- F31 inclusive comparators and De Morgan chains (eleventh round): `ge`/`le`
  normalize into `IntGt`/`IntLt`/len/member-count vocabulary with shifted
  bounds; approximate condition lowering decomposes nested `and`/`or`
  recursively and distributes `not` by De Morgan with region-flipped
  int-cast subsets; `fail_outer_guard` and `terminal_clause_guard` lower
  `And` all-or-nothing; literal-key `index` navigation is an admitted
  equality subject binding the member path. cilium's `envoy.baseID`
  window rejects both sides and the ENI/AlibabaCloud cluster-id windows
  reject exactly at every boundary (helm-verified; pinned by
  `cilium_inclusive_comparator_chains_bound_integer_domains`); istiod and
  traefik's `ge`-gated arms materialize
- F74 duration overflow bounds and the semver significant-digit fix
  (eleventh round): `mustDateModify` terms are bounded per unit by
  certain-overflow SIGNIFICANT digit counts (ns 19 … h 7; leading zeros
  and fractional digits unbounded — both value-free; helm-verified at the
  exact boundaries), and the semver core grammar becomes
  `0*[0-9]{1,20}` per component — `ParseUint` overflow-checks the value,
  so the raw length cap was a latent false rejection on zero-padded
  spellings

## In progress

- **F17 residual — total-string literal preimages.** Cilium documents
  `kubeProxyReplacement` as string-or-Boolean and compares its `toString`
  result with `"true"`/`"false"`; the schema rejects both raw Booleans while
  Helm renders them. Project derived literal membership back through total
  stringification; the two string spellings are the passing controls.
- **F24 residual — terminal truthiness after total stringification.** Cilium's
  removed `proxy.prometheus.enabled` guard stringifies the `dig` result before
  testing it. The schema accepts raw `false`, but Helm sees truthy `"false"`
  and aborts; an absent/empty value remains the valid control.
- **F28/F51 residual — ranged terminals and accumulator state.** Traefik
  accepts an HTTPS gateway listener without `certificateRefs` and HTTP/3 with
  TLS disabled although both range-local terminals abort Helm. Velero also
  accepts both legacy fs-restore label/image forms because the `$breaking`
  accumulator mutated inside a range is lost before the final `fail`; current
  forms and satisfied Traefik members are the passing controls.
- **F31 residual — coercion preimages and helper-bound numeric terminals.**
  Radix/leading-zero `ParseInt` spellings and mixed-sign bounds remain (the
  `ge`/`le`/`gt` chains landed in the eleventh round); the mixed-sign
  regions are now understood to be encodable as the COMPLEMENT of the
  above-bound patterns once the radix family is complete — `cast.ToInt64`
  coerces every unparseable and overflowing spelling to 0, inside every
  positive-bound region. Separately, Kyverno accepts
  `admissionController.replicas: 0`, but the `kyverno.deployment.replicas`
  template helper aborts; `1` renders. Carry terminal summaries across scalar
  `template` returns as well as completing the coercion domains.
- **F32 residual — nested cross-path/default implications.** Cilium accepts
  GKE+tunnel and AKS-BYOCNI+native routing, invalid ingress/Gateway API
  `externalTrafficPolicy`, and external kvstore mode with apiserver replicas
  `2`; Helm aborts each, while `native`, `tunnel`, `Cluster`/`Local`, and
  replicas `1` respectively render. Preserve the nested guards through
  `default` and project the guarded `toString` equality to its raw preimage.
- **F56 residual — provider evidence leaking through scalar text.**
  Chart-authored OAuth2 Proxy and Argo CD redis-ha values put scalar members
  under block-scalar config text, and Traefik quotes
  `tracing.otlp.resourceAttributes` members into command arguments. The
  generated schemas type those members as `null` and reject the charts' own
  values, while Helm plus strict provider validation succeeds. Scalar text
  fragments must not inherit the outer ConfigMap/argument provider shape.
- **F74 residual — parser exactness and fallback selection.** Exact URL
  authority and Datadog's derived `toString | trimSuffix "-jmx"` semver
  preimage remain open (per-term duration overflow bounds landed in the
  eleventh round; multi-term sums stay a superset by design — a sum bound
  is not regex-representable). Datadog's own OTEL gateway CI values add
  the opposite selection bug: an empty image tag is rejected by the raw
  semver arm although the helper replaces it with the agent-version
  fallback; an explicit valid tag is the passing control.
- **F80 residual — merge selection and provider attribution.** Airflow's
  worker-family `securityContext` still loses provider typing under its merged
  context — now attributed exactly to two stacked gaps (eleventh round):
  (1) `removeNilFields … | fromYaml` summarizes as `Unknown`, erasing the
  celery layer's identity, so every `hasKey`/truthiness probe of the
  priority chain lowers `Approximate` and the builder skips every placed
  row; (2) with identities restored, the per-set layer's probes decode to
  wildcard-member guards (`¬Absent(workers.celery.sets.*.securityContext)`)
  the conditional-overlay vocabulary cannot encode — member quantification
  exists only in the fail-implication lane, and the overlay arm needs the
  existential form. Re-tightening needs a nil-scrub identity recognizer
  plus existential member-guard encoding; neither piece alone yields any
  tightening. Kube Prometheus Stack adds a literal-`dig`/`mergeOverwrite` case:
  a scalar per-rule annotation operand passes the schema but aborts Helm, and
  a numeric annotation member renders but fails the committed PrometheusRule
  provider schema; map/string controls pass. Preserve kind and member
  provenance through the selected ordered layers.
- **F87 residual — exact IPv6 element language.** The Cilium Hubble SAN regex
  is only an IPv6 superset: it accepts `":"`, but `genSignedCert` rejects it
  as an invalid IP; `"::1"` passes schema and Helm. Replace the superset with
  the builtin parser's exact accepted language or a provably sound subset.
- **F98 residual — provider-required leaves on ranged members.** Promtail
  accepts `extraPorts.audit: {}` and renders null Service/DaemonSet port
  fields; kube-state-metrics accepts a startup-probe `httpHeaders: [{}]` and
  renders null `name`/`value`. Strict provider validation rejects both, while
  populated controls pass. Back-propagate required leaves through ranged
  dynamic-map and array members.
- **F102 residual — nested dependency-lock coverage.** The integrity test
  scans only immediate corpus chart roots although nested vendored charts in
  Airflow, Datadog, Kyverno, MetalLB, and SigNoz carry their own locks. The
  current nested locks match, but deleting or drifting one is invisible to the
  gate. Discover locked charts recursively while avoiding duplicate traversal.
- **F104 residual — wrapper consumers before tree rewrite.** NATS accepts a
  `$tplYaml` wrapper at `nameOverride`, but `nats.fullname` calls `trunc` on
  the sentinel map before the rewrite and Helm aborts. The raw string control
  and a wrapper at the later `container.image.fullImageName` consumer render.
  Wrapper alternatives must be scoped to the consumer's execution phase.
- **F107 — terminal contracts lost across helpers/caller scopes.** Verified
  schema-accept/Helm-fail cases are Kube Prometheus Stack Grafana dashboards
  without exactly one folder selector or non-empty `matchLabels`, OAuth2
  Proxy standalone Redis without `connectionUrl`, Vault HTTPRoute without
  `parentRefs` and invalid redundancy-zone combinations, and Datadog's
  forbidden `unix:` OTLP gRPC endpoint. Their satisfied controls render.
  Summarize helper terminals and retain the caller's live guard when applying
  them.
- **F108 — NATS static JSON Patch item grammar.** `service.patch: [{}]`, an
  unknown `op`, and a non-rooted `path` all pass the schema but abort the
  chart's bounded `_jsonpatch.tpl` interpreter; a valid `test` operation
  renders. Infer object items, the operation enum, JSON-pointer lexical rules,
  and the per-operation `value`/`from` requirements; pointer existence may
  remain unknown.
- **F109 — helper-return alternatives collapsed into one object shape.**
  Traefik's local-plugin helper has mutually exclusive legacy-hostPath and
  inline-plugin return arms. Both documented valid shapes are rejected, while
  an unknown `type` passes the schema and aborts Helm because the generated
  definition conjoins `hostPath` and `type`. Preserve the helper's per-arm
  requirements and `type` literal domain instead of intersecting the arms.

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

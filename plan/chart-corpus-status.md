# Chart-corpus findings: status ledger

Last reconciled 2026-07-18, after the remainder completion round (the
jenkins F31 variable-bound coercion validator and the F83/F85
inline-local kind partition; see the work log's "Remainder completion
round"). One classification per finding. Where a finding has
both a completed bounded part and a remainder, the remainder is what is
classified here; the completed part is listed under Completed with a
"(bounded)" marker. Per-finding evidence and fix history live in the dated
sections of the historical work log in `chart-corpus-expansion.md`.

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
- F28 type-validation guards and `fail` branches as schema evidence
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
- F44 key-predicate contracts on dynamic map values
- F45 string-only call effects (incl. `substr`, `htpasswd`)
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
- F61 strict collection-call signatures for every audited function
  (uncatalogued long tail abstains by design — see Rejected)
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
  conditional literal reassignment, lexical escape tokens (bounded; the
  printf-derived datadog tag is Rejected)
- F75 shape erasure through `first`/`last`/`initial`/`rest`/`compact` and
  audited nested member paths (bounded; dynamic `slice`/opaque identities
  are Rejected)
- F76 YAML scalar lexical safety: plain-token exclusions with class-aware
  allowances, numeric-grammar exclusions, double/single-quoted content,
  flow style, mapping keys, completed-token contracts, composite-in-quotes
  recursive serialization preimage (F76.2), empty-scalar defaults under
  member projection
- F77 `and`/`or` selected-operand values
- F78 value-selecting functions keeping candidate-selection predicates
- F79 `break`/`continue` suppressing later-iteration contracts
- F80 ordered `merge`/`mergeOverwrite` layers with per-key shadowing arms
  (velero securityContext)
- F81 Sprig arithmetic coercion boundary
- F82 chart-authored `values.yaml` programs executed by `tpl`
- F84 split-segment provider preimage for integer slots (bounded; general
  numeric enum/range projection is Rejected)
- F86 strict Boolean call signatures incl. architecture partitions and
  `IntGt` sound subsets
- F87 builtin signatures constraining nested collection elements
- F88 derived literal-membership and `typeOf`→`regexMatch` dispatch guards
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
- F102 bitnami-redis locked `common` dependency vendored plus
  corpus-integrity gate
- F103 test compositors scrubbing nulls only along map chains
- F104 `$tplYaml` program-wrapper alternatives at value nodes (bounded;
  the extraResources member-kind case is the NATS entry below)

- F31 scalar-domain fail implications: `len` bounds via the pattern
  subset, `int`-coerced inequality pairs via the raw-integer subset,
  negated literal membership via the exact NotEq conjunction, and the
  semver-minimum terminal through the comparator pattern subset (cilium
  name/kvstoreMode/maxConnectedClusters, airflow minimum version).
  Variable-bound casts included: a local bound to `int`/`int64` of a
  direct selector (optionally through a literal-integer `default`)
  carries `IntCastSource` provenance, both raw-integer recognizers
  resolve through it, `IntLt` mirrors `IntGt` for the below-bound
  direction, and disjunctive fail conditions lower arm-by-arm
  (`AnyOf`) — jenkins' `controller.replicas` 0..=1 domain, plus
  kyverno's PDB mutual-exclusion terminals as fallout
- F51 existential range sentinels: branch joins stamp arm conditions onto
  changed truthiness reductions (bounded), the joined
  `Range ∧ member-Eq` flag lowers as `ConditionalGuard::
  ContainsMemberEquals` (`contains` on the array lane, the double-negated
  member quantifier on the object lane), and terminal clauses admit
  approximate conjuncts through their sound subsets (airflow's celery
  broker sentinel)
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
- F93 same-map member identity through `pluck`: `keys m` keeps the map
  identity (`KeysList`), `sortAlpha` preserves it, `pluck . $dict |
  first` over the ranged key is a member projection, `printf "%T"` joins
  the type-descriptor family, and member-local type partitions lower to
  member overlays carrying the provider projection (pinned at gen level;
  the signoz corpus chart itself abstains — see Rejected)
- NATS `extraResources` member kinds: a ranged member spliced as a whole
  document at column zero must be an object when present and non-null
  (Helm decodes every manifest as a mapping); wrapper items are objects
  and stay open
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

## In progress

Nothing. Both former remainders (the jenkins F31 variable-bound
coercion validator and the F83/F85 inline-local kind partition)
completed in the 2026-07-18 remainder round.

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
  them.
- **F74 remainder — datadog printf-composed agent tag.**
  `get-agent-version` composes derived text through `printf`; no sound
  bounded preimage exists, so the raw tag abstains (false ACCEPT, not a
  false rejection).
- **F84 remainder — general substring preimages.** Projecting an arbitrary
  provider numeric enum/range onto the nth substring of a raw string is
  not faithfully encodable as a Draft-07 regex once signs, bases, and
  coercion are involved; the integer-slot subset is Completed.
- **F93 remainder — cross-map dynamic key correlation.** Draft-07 cannot
  correlate one dynamic property name across two independent maps; only
  the same-map projection (In progress above) is representable.
- **SigNoz `additionalEnvs` member constraints — relational member set.**
  The chart's `renderAdditionalEnv` gates every render on a case-folding
  dedup accumulator: a member can be SHADOWED by an earlier
  case-colliding key and never render, so a blanket per-member EnvVar
  constraint would falsely reject `{audit: {value: 7}, AUDIT: …}`. The
  schema soundly keeps the members open
  (`signoz_additional_env_members_stay_open_under_dedup_shadowing` pins
  the shadowed-member acceptance); the same-map projection MACHINERY is
  Completed and gen-pinned. A future bounded increment could constrain
  singleton maps (`maxProperties: 1` ⇒ the first iteration provably
  renders).
- **Adjudicated-wrong audit claims.** AWS LBC `nameOverride: "null"`:
  rendering yields a null label value that the strict v1.35.0 schemas
  reject on every resource, so the plain-token exclusion is correct.
  SigNoz zookeeper printf pin: helm aborts on a non-string
  `clickhouse.zookeeper.nameOverride` inside Sprig `contains`, so the
  operand-abstention pin was wrong and was replaced by the branch-scope
  pin.

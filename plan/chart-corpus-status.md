# Chart-corpus findings: status ledger

Last reconciled 2026-07-21 after the open-items round (twenty-second
round), which closed the four items the twenty-first round left: the F32
defaulted-comparison residual, the F74 transformed-semver bound, the F108
per-op requirement bound, and the F80 reroot chain. Notable mechanisms: a
defaulted binding carries its literal fallback
(`HelperOutputMeta::default_fallback`), so `eq $mode FALLBACK` decodes
the exact `eq ∨ ¬truthy` arm (external-secrets' deleted `renderMode`
selects the default arm instead of the invalid-mode `fail`); lexical
escapes gained the `CutAtToken` erasure (`regexReplaceAll "TOK.*$" X ""`)
and one-escape-per-edge sets compose exactly, which also lets
`semverCompare` bounds project through cilium's digest-strip/v-trim
pipeline as fail-position sound subsets; helper-dict root fields no
longer shadow same-named range variables and JSON-roundtripped member
identities decode `hasKey`, which binds the nats jsonpatch per-op
`value`/`from` requirements; and the airflow reroot chain landed in four
pieces — the scrub marker survives on the set-free merge operand, nested
`MergedLayers` flatten in precedence order for identity extraction, a
choice layer's constant-False `hasKey` alternatives drop as OR
identities, and negated member-quantified guards encode the
`anyOf[¬∀, ∀¬]` pair so empty per-set collections leave deeper layers
unshadowed. Rerouted layer arms whose unlowerable conditions HARD-NEGATE
foreign-family selections keep the pre-layered routing (airflow's
deprecated `securityContext` fallback stays open behind a live
`securityContexts.pod`). The coalesced-document doctrine stands
unchanged: the generated schema validates the COALESCED values document,
absence semantics follow the parent-declared/dependency-owned ownership
rule, and every pin composes its override over the chart defaults
(`chart_instances::with_override`). The remaining open note is the F32
signoz `global.imagePullSecrets` re-widening (needs its own
zookeeper-side capture); the F80 quantifier corners are documented
bounds. Green corpus tests are a baseline, not completion evidence.
Where a finding has both a completed bounded part and a remainder, the
completed part is listed below with a "(bounded)" marker and the residual is
classified separately. Per-finding history lives in
`chart-corpus-expansion.md`.

## Completed

Fixed on the current tree and pinned by tests (corpus fixtures,
`chart_reaudit` cases, or focused gen/IR reproducers):

- **F53 — helper-local `tpl` operands (twenty-first round; closes the
  residual).** Sprig `has LITERAL .Values.list` decodes to the new
  `ContainsEquals` guard (the dual of the literal-list membership):
  `has` returns false on a nil haystack and aborts on non-lists, so the
  guard holds exactly for arrays carrying the literal and encodes as
  `{type: array, contains: {const: L}}` with coalesced absence read from
  the subchart-defaults doc. With the gate decodable, the helper-internal
  truthy⇒string captures from `tpl` operands bind back to callers:
  oauth2-proxy rejects maps/ints/bools at `config.existingSecret` (its
  own truthy self-guard) and at `config.{cookieSecret,clientSecret,
  clientID}` under `has "…" .Values.config.requiredSecretKeys`, while
  empty and partial membership lists keep each tpl dormant — every
  polarity helm-verified. Collateral wins: external-dns's five
  `has "gateway-*" .Values.sources` RBAC gates, harbor's
  `jobLoggers`/`proxy.components` membership arms (the jobservice PVC's
  provider typing now binds: accessModes/claimName/size/annotations
  reject junk, each landing in a strictly-typed PVC field), and nack's
  `--control-loop` argument probe. Pinned by
  `oauth2_proxy_helper_tpl_operands_bind_string_contracts` plus four
  regenerated fixtures. Null-deleted secrets under a live gate stay
  accepted (the truthy⇒string lane's documented widening; `tpl nil`
  aborts).
- **F56 — helper text inside adopted block scalars (twenty-first round;
  closes the residual).** The false-reject was a PARSER bug, not an
  adoption bug: the tree-sitter go-template grammar attaches an
  un-spaced pipe to the last ARGUMENT (`print (include "a" .)| sha256sum`
  parsed as `print((include "a" .) | sha256sum)`), while Go's tokenizer
  emits `)` and `|` identically with or without whitespace — the pipe
  always splits the enclosing command, and argument-position pipelines
  are only legal parenthesized. `call_with_unfolded_pipe` restores the
  Go reading at AST conversion (nested stages flatten), so redis-ha's
  checksum annotation (`print (include …) …)| sha256sum`) decodes as a
  digest again and every script-helper read rides the Serialized lane
  instead of leaking a provider plain-scalar-string domain. With
  redis-ha live, oauth2-proxy and argo-cd accept numeric
  `sentinel.quorum`, `splitBrainDetection.{interval,retryInterval}`,
  `ro_replicas`, and numeric `tls.certFile` — all helm-verified renders.
  Pinned by the `unspaced_argument_pipe_*` AST cases (the
  `default "x" .Values.y|quote` idiom included) and
  `oauth2_proxy_redis_script_reads_stay_partial_text`.
- **F65 — accepted inputs through root rewrite (twenty-first round;
  closes the residual).** An overwrite-merge whose destination is the
  LITERAL values root (`mustMergeOverwrite $.Values (index $.Values
  "pilot")` — istiod's zzy descope; a copied destination like `deepCopy
  $.Values` deliberately does not qualify) records a
  `values_root_overlay_prefixes` fact that rides interpreter → contract
  → signals, and `with_root_overlay_fail_implications` twins every
  fail-grade implication onto the prefixed spelling: guards about the
  subject path move with it, foreign guard paths keep their root
  spellings (the documented bound — cross-path conditions supplied
  through the same overlay may mis-scope a twin). istiod now rejects
  `pilot.env: "oops"` and `pilot.env: [1]` exactly like the root
  spellings while `pilot.env` maps and the descope-style overrides
  render — helm-verified each way. A conditionally-activated subchart
  clears its prefixes (same abstention as conditional default sources).
  Pinned by `istiod_pilot_overlay_carries_root_contracts`.
- **F98 — required leaves through helper projections (twenty-first
  round; closes the residual).** Sprig `quote`/`squote` SKIP nil
  operands — unlike every other total stringification they render
  NOTHING for a missing source, forcing an explicit YAML null into the
  slot — so quoted splices now carry a `nil_omitting` marker through
  splice meta, helper summaries, and `ContractUse` into
  `ProviderSchemaUse`. A nil-omitting Serialized use of a RANGED member
  leaf survives provider-use construction (typing still abstains — any
  input quotes) solely so `synthesized_ranged_member_required_implications`
  can bind the `FieldPresentNotNull` presence requirement when the slot
  is provider-required. Traefik: both local-plugin alternatives now
  require `mountPath` (helm renders the null volumeMount; the committed
  provider bundle is the rejecting stage), with the reaudit alternatives
  pin migrated to carry it. Collateral wins: velero's Schedule
  `schedule` and BackupStorageLocation `bucket` presence (both
  CRD-required, quote-rendered). Pinned by
  `quoted_ranged_leaves_bind_presence_through_the_pod_template_projection`
  and `traefik_local_plugin_mount_paths_bind_provider_presence`.
- **F28/F51 — ranged terminals and accumulator state (twenty-first
  round; closes the remainder).** The traefik http3 service terminal now
  binds: a range over a local-dict OVERLAY (`$services :=
  .Values.service.additionalServices` + unconditional `set $services
  "default" (omit …)`) records one literal entry as the member
  variable's DEFINITE binding (`definite_range_member_values`, joined
  across branches only when identical), and an unfaithful member
  condition re-decodes under that binding into a sound subset
  (`approximate_with_sound_subset`) — the entry iterates on every
  render, so "the condition holds for the definite member" implies "it
  holds for some iteration", usable exactly where firing less often is
  safe (the terminal-clause subset lane). Traefik: http3 without
  `http.tls.enabled` now rejects through the always-present "default"
  service, http3 with tls renders, and `service.enabled: false` keeps
  the terminal dormant — helm-verified each way. Pinned by
  `overlay_range_member_gates_carry_definite_entry_sound_subsets` and
  `traefik_http3_terminal_binds_through_the_services_overlay`.
- **F30 — guarded `required` absence (twentieth round).** The `Absent`
  guard condition encodes the coalesced document uniformly:
  `anyOf[missing, explicit-null]` regardless of declared defaults —
  coalescing already filled every parent default, so a missing key always
  means nil at render (the old `declared_default_fills` branch encoded
  the raw-user-file view, where the state was unreachable and the
  conditions were dead). AWS LBC: `autoscaling.enabled: true` with
  `maxReplicas: null` (deleted) now rejects at validation while the
  default-filled, dormant, and explicit-value states render — all
  helm-verified. The ten corpus flips were each verified as real aborts:
  trivy-operator's four `required` image/compliance keys, kyverno's four
  `required` repository/create chains, and signoz's
  `externalClickhouse.{host,cluster}` requirements (the signoz pin's
  disabled-clickhouse instance now carries both, as any helm-accepted
  coalesced document must). Pinned by
  `guarded_required_rejects_null_deleted_declared_defaults` plus the
  regenerated fixtures.
- **F31 — base-0 cast preimages (twentieth round).** The int-cast string
  preimage languages are now exact for clean spellings:
  `magnitude_windows_above` builds per-position digit windows in every
  radix Go's base-0 parse accepts (decimal unpadded; hex/binary/octal
  with `0*` padding; legacy leading-zero octal), capped where int64
  overflow starts coercing to 0, and the Excluding escape language is the
  windows plus bounded residues (underscored spellings, the one
  boundary length per family where overflow is spelling-dependent).
  `ne (int X) L` decodes to a REGION DISJUNCTION
  (`IntLt{L} ∨ IntGt{L}` as strengthened approximates, with the
  call-level `default` collision exclusions), so string spellings ride
  the same window encodings as raw integers. Integer provider slots also
  accept the octal-fallback integral-float family ("08" renders as a
  YAML float that marshals back to JSON 8). All verified: cilium's DNS
  proxy `"0"`/`"00"`/`"0x0"`/unparseable spellings, `cluster.id: "1"`
  under the default name, `maxConnectedClusters: "300"` (each aborts
  helm) and their `"10094"`/`"0x1ff"`/`"255"` accept-controls; jenkins
  `"05"`/`"0x5"` abort while `"08"` renders. A differential test
  (`int_cast_preimages_agree_with_the_reference_coercion`) checks every
  claim direction against a reference Sprig coercion
  (`strconv.ParseInt` base 0 over `trimZeroDecimal`, underscore rules
  included) over an exhaustive short-spelling corpus plus boundary
  spellings across nine bounds. Chart pin:
  `cilium_int_cast_validators_bind_base0_string_preimages`.
- **F68 — provider constraints on ranged keys (twentieth round).**
  `synthesized_range_key_implications` projects the provider key-slot's
  own string constraints — `pattern` plus the new `StringLengthBounds`
  requirement for `minLength`/`maxLength` — onto the ranged source map's
  `propertyNames`, scoped to the use's branch guards. Traefik:
  `gateway.listeners.Audit` violates the Gateway CRD's lowercase
  SectionName pattern (helm renders it; the committed provider is the
  rejecting stage), 254-char and empty keys violate the length window,
  and lowercase keys plus both dormant states stay open — pinned by
  `traefik_listener_keys_carry_the_provider_section_name_domain`.
- **F32 core — coalesced-document absence semantics (twentieth round).**
  Condition encoding reads absence through the ownership rule stated in
  the header: `build_default_aware_leaf_condition_fragment` takes
  `absent_holds` = "does the guard hold at the value the render reads
  when the key is missing" — nil for parent-owned paths (Truthy/With and
  HasKey never hold; Eq only against the null literal; NotEq against
  every non-null literal; the int regions exactly when 0 lands inside;
  TypeIs only for "null"; AtMostOneMember vacuously) and the subchart
  default for dependency-owned paths (`build_dependency_values_yaml`:
  the dependency charts' composed defaults MINUS parent-declared paths,
  threaded into the encoders as `subchart_defaults_doc`, with
  chart-internal root-merge defaults copied in). Cluster Autoscaler's
  `minAvailable`-alone false-reject is gone, cilium's tproxy+netkit
  exclusion decodes through the new piped-membership lane, and the
  kvstoreMode/cluster-name validators now reject null-deletion states
  exactly. Roughly thirty pins migrated from raw-file shorthand to
  coalesced instances (composing over chart defaults via
  `chart_instances::with_override`).
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
  (the total-`toString` literal preimages landed in the twelfth round and
  the coalesce-default rescue of empty/null spellings in the thirteenth —
  see their entries below)
- F18 shape-erasing uses deleting independent strict uses
- F19 `printf` conflating the format parameter with data parameters
- F20 local-guard runtime contracts binding path-wide (loki `kindIs` arm)
- F21 guarded `range` domains
- F22 numeric casts modeled as identity
- F23 `typeOf` dispatch losing string-versus-structured alternatives
- F24 total-stringification facts lost in guard-only paths (terminal
  truthiness over the derived string landed in the thirteenth round — see
  its entry below)
- F25 direct `typeIs` Go container names
- F26 guarded `range` rejecting rangeable integers
- F27 compound document guards dropping chart-level string contracts
- F28 type-validation guards and `fail` branches as schema evidence (bounded;
  range-local terminal implications remain In progress)
- F29 condition transform collection ignoring pipeline order
- F30 Helm `required` termination as schema evidence (bounded; dynamic
  `extraEnvConfigMaps` members landed, guarded missing-value preimages remain
  In progress)
- F32 cross-path Boolean `fail` implications (bounded; nested/defaulted and
  negated-disjunction implications landed, further cross-path and
  absence-polarity cases remain In progress)
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
  equality members (bounded; further helper-local operands remain In progress)
- F54 type-dispatch overlays making supported arms impossible
- F55 partial type dispatch re-closing the unmatched complement
- F56 generic fragment fallback vs structural placement (bounded;
  helper-internal YAML lexical evidence inside adopted scalar text remains In
  progress)
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
- F65 ordered helper mutation in accepted input domains (bounded; effective-root
  rewrite preimages remain In progress)
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
  custom-merge lane landed in the eighth round, the fresh-dict
  layer-ordering/dormant-gate half in the eighteenth, and the nil-scrub
  recognizer with null-relaxed layer arms in the nineteenth — see the In
  progress entry; the real airflow worker-family chain stays open behind
  the `$globals` root re-root and per-set loop)
- F81 Sprig arithmetic coercion boundary
- F82 chart-authored `values.yaml` programs executed by `tpl`
- F84 split-segment provider preimage for integer slots (bounded; general
  numeric enum/range projection is Rejected)
- F86 strict Boolean call signatures incl. architecture partitions and
  `IntGt` sound subsets
- F87 builtin signatures constraining nested collection element kinds
  (the exact IPv6 parser language landed in the twelfth round — see its
  entry below)
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
- F98 provider-required output fields requiring source leaves (bounded; the
  direct ranged array/map member half landed, helper/roundtrip projections
  remain In progress)
- F99 finite literal `fromYaml` path programs (traversal interpreter)
- F100 post-`tpl` regex requirements on raw template programs
- F101 provider availability as a committed deterministic test input
  (`testdata/provider-bundle/`, cold/warm equivalence)
- F102 bitnami-redis locked `common` dependency vendored plus legacy-lock,
  unpacked-version, and recursive nested-lock verification
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
  `int`/`int64` provenance covers Jenkins' integer 0..=1 lane. The
  initial coerced-string preimage lanes landed in the fourteenth round — see
  that bounded entry and the residual below.
- F51 existential range sentinels (bounded): branch joins stamp arm conditions onto
  changed truthiness reductions, the joined
  `Range ∧ member-Eq` flag lowers as `ConditionalGuard::
  ContainsMemberEquals` (`contains` on the array lane, the double-negated
  member quantifier on the object lane), and terminal clauses admit
  approximate conjuncts through their sound subsets (airflow's celery
  broker sentinel). General terminals inside ranges remain In progress.
- F68 range-key slot domains (bounded): a raw range key rendered at a provider slot
  rides a marked splice (`range_key`) whose collection gains a
  keys-must-be-strings arm when the slot is string-only — non-empty lists
  excluded, maps and empty lists open (minio `environment`, and the
  `extraObjects`-family arms across the corpus). Provider lexical constraints
  on those keys remain In progress.
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
- F31 decimal coercion preimages: `IntGt`/`IntLt` encodings carry
  digit-wise decimal string preimages, and declared-default evaluation
  reads decimal string defaults (jenkins rejects `"5"` and `"-1"`
  replicas beside the raw integers); the radix family, the mixed-sign
  complement lane, and the zero-padded-octal false-rejection fix landed
  in the fourteenth round — see its entry below
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
- F17 total-`toString` literal preimages (twelfth round): an equality whose
  subject is the exact `%v` rendering of a path projects its literal back
  through the `toString` preimage — `"true"`/`"false"` admit the raw
  Booleans, `"<nil>"` admits null, and clean sub-million decimal spellings
  admit the number (larger magnitudes keep the string alone: float64 `%v`
  switches to exponent form at 1e6). The image is tracked precisely: a new
  `Effects::stringified_paths` channel records `toString` over a pure
  identity operand (never `quote`/`join`/`len`/casts, whose text differs),
  rides `HelperOutputMeta::stringified` through binding meta and value
  arms (`mark_stringified_identities`), and `eq`/`ne` decoding expands
  `Eq`/`NotEq` into the preimage disjunction/conjunction; a direct
  `toString <selector>` call is now an admitted equality subject. cilium
  accepts raw `kubeProxyReplacement: true`/`false` while `strict` and `1`
  still abort (helm-verified). Pinned by
  `cilium_kube_proxy_replacement_accepts_raw_booleans` (CLI) and
  `stringified_equality_binds_the_tostring_preimage` (gen)
- F74 datadog empty-tag fallback selection (twelfth round): an `if`-arm
  that reassigns a local to ANOTHER source path (`if not $imageTag {
  $imageTag = include "get-agent-version" . }`) now severs the entry
  identity like a literal sentinel — the kept raw arm gains a capture
  exclusion whose sound subset decodes falsiness headers to the path's
  truthiness — and `stringified` identity arms survive the `toString`
  reassignment in parser-operand collection, so the semver domain binds
  only truthy raw tags. The gateway CI values' empty tag and a null tag
  render through the agent-version fallback while `junk` still aborts
  (helm-verified). Pinned by
  `datadog_otel_gateway_empty_tag_selects_the_agent_version_fallback`
  (CLI) and
  `falsy_reassignment_to_another_source_scopes_the_parser_to_truthy_values`
  (gen)
- F87 exact IP element language (twelfth round): the
  `genSignedCert`/`genSelfSignedCert` ip-list item pattern is now
  `net.ParseIP`'s exact accepted language — dotted-quad IPv4 without
  leading zeros plus RFC 4291 IPv6 under Go's rules (1-4 hex digits per
  group, one `::` expanding at least one zero group, embedded quads only
  as the final four bytes, no zones), with the v4-embedded left/right
  splits enumerated. Fuzz-differentialed against `net.ParseIP` over ~56k
  candidates and cross-checked against `helm template`; a bare `:` and a
  zoned address now reject while every compressed/mixed form renders.
  Pinned by `ip_item_pattern_is_the_parse_ip_language` (ast) and the
  extended `cilium_certificate_sans_require_string_members` (CLI)
- F102 recursive dependency-lock discovery (twelfth round): the corpus
  integrity gate walks every `charts/` subdirectory as a chart root of its
  own, so nested vendored locks (airflow's postgresql, signoz's
  clickhouse → zookeeper chain) are checked; pinned by
  `nested_dependency_locks_are_discovered`
- F109 local-plugin alternative shapes (twelfth round): a fail whose test
  conjoins several member conditions now negates to the DISJUNCTION of
  their negations — `FailValueRequirement::AnyOf` alternatives (plus the
  new `FieldEquals` for `eq $plugin.type "…"` dispatch arms holding),
  emitted as `{type: object, anyOf: […]}` so property carriers merge
  conjunctively. Two union-combiner defects surfaced and were fixed:
  `merge_object_schemas` treated an alternation-only object as
  unstructured (replacing it wholesale) and silently dropped the other
  side's sibling `anyOf`. traefik's legacy-hostPath and inlinePlugin
  shapes now render, an unknown `type` (even beside a hostPath) and a
  member with neither field reject — all helm-verified. Pinned by
  `traefik_local_plugins_keep_their_alternative_shapes` (CLI) and
  `multi_test_fail_negations_lower_as_member_alternatives` (gen)
- F56 block-scalar adopted includes (fourteenth round, bounded): the audit's
  OAuth2 Proxy / Argo CD "block-scalar claims" DID reproduce once the
  `redis-ha` gate was enabled — the twelfth round's re-check had only
  exercised the charts' own values, whose `enabled: false` kept the
  guilty arm dormant. redis-ha's ConfigMap writes `redis.conf: |`
  followed by a COLUMN-ZERO `{{- include "config-redis.conf" . }}`; the
  include's rendered lines are deeper than the entry, so at render time
  they continue the still-open block scalar — pure text. The evaluator
  instead let the include escape to the parent container as structure,
  anchoring the helper's ranged `redis.config` members at the `data`
  field itself, whose OBJECT provider schema scalar-restricts to
  `type: null` — rejecting every member Helm renders (argo-cd's own
  `save: '""'` default rejected once `redis-ha.enabled`). A bare Output
  hanging under a block-scalar entry or item is now ADOPTED into the
  block text with exactly the block-body hole discipline: fragment
  renders keep their semantic rows without minting structure, plain
  holes contribute partial scalar text — so the strict `tpl`
  string-program contract on `customConfig` survives while the members
  open up. Pinned by
  `oauth2_proxy_redis_ha_config_members_render_as_block_text` and
  `argo_cd_redis_ha_own_defaults_render_when_enabled` (CLI) and
  `block_scalar_adopted_includes_render_as_text_not_structure` (gen,
  provider bundle); all polarities helm-verified
- F31 coercion preimages and the kyverno terminal (fourteenth round,
  bounded): (a) `eq (int X) N` decodes in fail position
  as the `IntGt{N-1} ∧ IntLt{N+1}` region pair (with the default-zero
  escape), and its negation as the inequality subset — kyverno's
  `kyverno.deployment.replicas` helper terminal now rejects
  `replicas: 0` through the `{{ template … }}` call while `"0"` keeps
  the helper's own `kindIs "string"` escape; (b) the single-sign string
  preimages gained the radix family (hex/binary/explicit and legacy
  octal, nonzero lead, overflow-capped digit counts; underscored and
  zero-padded spellings abstain); (c) mixed-sign regions (positive
  `IntLt` bound, negative `IntGt` bound) now claim the COMPLEMENT of an
  overapproximated parse-escape language — every unparseable, empty, or
  wrong-sign spelling coerces to 0 inside the region; (d) the
  below-zero pattern's `-0*[1-9][0-9]*` arm was a live FALSE REJECTION:
  a zero-led spelling parses as octal, where an 8/9 digit is a parse
  error coercing to 0 (`"-018"` renders — helm-verified), so the
  zero-padded arm now admits valid octal digits only. Pinned by
  `kyverno_zero_replicas_abort_through_the_template_helper` (CLI),
  `int_cast_zero_equality_fails_reject_raw_zero` and
  `int_cast_string_preimages_cover_radix_and_complement_lanes` (gen);
  all coercions verified against `helm template` renderings of
  `int`/`int64`
- F98 ranged-member required leaves (fourteenth round, bounded): a wildcard
  member LEAF rendered as a direct scalar hole
  into a provider-REQUIRED field emits an explicit null for every
  member missing the leaf, which strict provider validation rejects.
  The new `synthesized_ranged_member_required_implications` lane
  projects `Members`-targeted `FieldPresentNotNull` requirements onto
  the collection: collection-level guards ride the arm as outer guards,
  and a NEGATIVE member-scoped truthiness guard (an else-arm) becomes
  the `FieldHelmTruthy` ESCAPE alternative of a per-member disjunction
  — promtail's `service`-arm members escape the `containerPort`
  requirement… except promtail ALSO renders every member's
  `containerPort` unconditionally at the pod ports, so `service.port`
  alone is still provider-invalid (helm-rendered nulls verified).
  Positive member-scoped guards abstain: those arms read from the
  guarded subtree, where the leaf routinely rides a `default` fallback
  whose primary source the projection cannot see. Tolerant render
  forms (serialized/fragment/partial/nullable/self-guarded) abstain
  like the direct lane. kube-state-metrics' probe `httpHeaders: [{}]`
  and promtail's `extraPorts.audit: {}` now reject while populated
  members and the zero-iteration lanes stay open. Pinned by
  `promtail_extra_port_members_require_the_container_port` and
  `kube_state_metrics_probe_headers_require_name_and_value` (CLI),
  `ranged_member_leaves_of_required_provider_fields_bind_presence` and
  `ranged_member_required_leaves_keep_the_else_arm_escape` (gen)
- F108 direct-range inequality enums (fourteenth round, bounded): a
  conjunction of `ne $item.field "…"` tests guarding a ranged fail now
  negates to the DISJUNCTION of the equalities — `Guard::NotEq` joined
  the negatable member tests, lowering through `FieldEquals`
  alternatives (presence rides Go's nil-comparing `ne`: a missing
  field differs from every literal, so the inequality HELD there and
  the enum requires the field). Pinned by
  `ranged_not_equals_chains_negate_to_the_field_enum` (gen). The nats
  jsonpatch grammar itself stays In progress below — its fails ride a
  helper-scope range whose captures lack member identities.
- F107 helper-terminal decode lanes (fifteenth round, bounded): four
  condition shapes that abstained helper-terminal captures now decode
  exactly. (1) `eq (include "h" .) "true"` where the helper body is ONE
  boolean-valued expression synthesizes the two-arm literal dispatch
  `if <expr>` → "true" / else → "false" (oauth2-proxy's
  `redis.enabled` helper). (2) `eq (default D X) V` over a literal
  fallback binds X exactly (V == D also admits every Helm-falsy X; a
  truthy V ≠ D is the plain equality; a falsy V ≠ D never holds) —
  oauth2-proxy's `clientType`/"standalone" caller gate. (3) Scalar-dot
  helper terminals (`include "verify-…" .grpc.endpoint`) already bound
  the caller path; `hasPrefix`/`hasSuffix` over a values-path subject
  now lower as anchored `MatchesPattern` tests so datadog's
  `hasPrefix "unix:" .` terminal rejects beside the existing
  `regexMatch ":[0-9]+$"` port test. (4) `X | toString` pipelines
  decode like the `toString X` call form in equality position (vault's
  redundancy-zone gates, cilium's operator update-strategy arm). Chart
  flips: oauth2-proxy standalone Redis without `connectionUrl` rejects
  while the explicit-url and enabled-subchart variants render
  (`oauth2_proxy_standalone_redis_requires_a_connection_url`);
  datadog's `unix:`-with-port and portless OTLP gRPC endpoints reject
  under the apiKey/enabled gates while host:port and the disabled
  receiver stay open
  (`datadog_otlp_grpc_endpoints_reject_the_unix_protocol`). Gen
  reproducers: `helper_terminals_keep_caller_guards_and_boolean_include_arms`,
  `scalar_dot_helper_terminals_bind_the_caller_argument_path`,
  `pipeline_tostring_gates_decode_in_helper_terminals`. The vault/KPS
  chart-level residuals stay In progress below.
- F32 defaulted-pipeline and negated-disjunction tests (fifteenth
  round): cilium's provider-mode gates decode end to end.
  `ne (.Values.routingMode | default "native") "native"` rides the
  default-eq lane, so GKE+tunnel and AKS-BYOCNI+native reject while the
  unset and matching spellings render; `not (or (eq P "Cluster")
  (eq P "Local"))` now negates by De Morgan over EXACT per-disjunct
  decodes (faithfulness-gated so truthy stand-ins are never negated),
  keeping the conjunction flat for guard extraction — the ingress and
  Gateway API `externalTrafficPolicy` domains reject unlisted values.
  The audited kvstore-replicas case was adjudicated already-correct:
  replicas `1` with the default `identityAllocationMode=crd` ALSO
  aborts Helm (line 201's identity-mode check), so both rejections
  stand, and the fully valid combination (kvstore identity mode,
  replicas 1, placeholder config) renders and validates. Pinned by
  `cilium_provider_modes_pin_routing_and_traffic_policy_domains` (CLI)
  and `defaulted_pipeline_and_negated_disjunction_tests_decode` (gen).
- Member-access fanout regression fix (fifteenth round): decoding MORE
  guards must never lose an unconditional navigation's typing. The
  member-access guard-set cap previously skipped a whole path once its
  access count passed the fanout bound — with the new decode lanes,
  paths like oauth2-proxy's `sessionStorage` crossed the bound and lost
  the unconditional `type: object` the chart's unguarded
  `.Values.sessionStorage.type` navigation requires (helm errors on a
  scalar host). The cap now bounds only the guarded-only ANY-OF folds;
  an unconditional access (empty guard set) binds regardless. The
  rescue re-types 27 corpus charts' unconditionally navigated hosts
  (airflow `dags.gitSync`, datadog `providers.gke`, harbor `redis`,
  kyverno `global`, traefik `providers.*`, …) — twelve helm spot
  checks all reject the probes, and the falsy sub-class is pinned by
  datadog's `agent-services.yaml` unguarded deep navigation
  (`can't evaluate field receiver` on `otlp: false`).
- F107 vault half — branch-conditioned root-set value dispatch
  (sixteenth round): a root-context key assigned a scalar literal in
  EVERY arm of a complete if/else chain (vault's five-arm `vault.mode`)
  now joins into a `RootValueDispatch` — mutually exclusive, total
  (condition, literal) arms — so `eq .mode "ha"` / `ne .mode
  "external"` decode as the exact disjunction of the assigning arms
  (negation exact by totality). Four machinery pieces landed together:
  (a) if/else regions evaluate each arm from the ENTRY root-set state
  (one arm's `set` no longer leaks into a sibling's evaluation) and
  join outcomes after the region — a last-write replay for incomplete
  chains, the exact dispatch when the chain has an unconditional else,
  every arm condition decoded without approximation, and scalar
  literals throughout; the joined truthiness (disjunction of
  truthy-literal arms) replaces the old wrong last-arm predicate.
  (b) The contract-guard negation algebra is complete under De Morgan:
  `¬(a ∨ b)` flattens to the guard conjunction of the negations,
  `¬(a ∧ b)` to one `AnyOf` alternative per conjunct — abstaining
  whole (never dropping a leaf) when any leaf cannot flip — and
  `Guard::Not`/`Or`/`AnyOf` gained `ConditionalGuard` encodings so
  mode-dispatch conditions key member-access arms and rows instead of
  vetoing them. (c) The caller's root truth predicates and value
  dispatches thread into bound-helper resolutions when the helper dot
  IS the caller's root context (memoization keys include them), so
  helper bodies like vault's volume-claims decode `ne .mode "dev"`.
  Chart flips, all helm-verified: httproute enabled without
  `parentRefs` aborts while the parentRefs and external-mode variants
  render; redundancy zones without `server.ha.enabled` (and with ha
  but without raft) abort while the full combination and the
  external-mode variant render; the `ui.*` service ports became
  EXACTLY conditional — `ui.enabled: false` (the default) frees
  `externalPort`/`targetPort` to any shape the templates never read
  (the shipped `values.schema.json` is deliberately not evidence),
  while `ui.enabled: true` still rejects a string port; thirteen
  statefulset payload classes tightened under the now-decoded internal
  modes (extraContainers/volumes/extraPorts/extraSecretEnvironmentVars/
  extraVolumes template-fail; annotations/nodeSelector/tolerations/
  resources/hostAliases/topologySpreadConstraints kubeconform-invalid
  against v1.29 strict). The redundancy-zone CONFIG placeholder fail
  (`regexMatch "(?m)^…autopilot_redundancy_zone…"`) stays open by
  design: Go's `(?m)` flag has no Draft-07 ECMA-pattern encoding.
  Pinned by
  `vault_mode_dispatch_binds_httproute_and_redundancy_zone_fails`
  (CLI) and `root_set_literal_chains_decode_as_value_dispatch_guards`
  (gen); 9 CLI + 3 IR + 1 gen fixtures adopted, the probe battery's
  112 flips (all vault) adjudicated as above — the eight other
  re-encoded charts show zero acceptance flips.
- F107 capabilities half — the Kubernetes version policy in IR condition
  lowering (sixteenth round — completes this half): the analysis session threads
  the normalized primary `--k8s-version` core (`v1.29.0-standalone-strict`
  → `1.29.0`) into `SymbolicIrContext::with_policy`, and `semverCompare`
  conditions over Capabilities-defaulted subjects decode exactly. The
  subject lanes: a bare `.Capabilities.KubeVersion.Version|GitVersion`
  selector evaluates the constraint against the policy version as a
  CONSTANT; `default .Capabilities.KubeVersion.X <values-path>` (directly
  or through a bound local, tracked by the new `kube_version_sources`
  channel) splits into the falsy-override policy arm and the
  truthy-override `MatchesPattern` arm over the constraint's exact regex
  language. The semver pattern encoder gained the two prerelease-FLOOR
  idioms charts actually use — `>=X-0` (core ≥ X, prereleases included)
  and `<X-D` with a single-digit prerelease (core < X plus X's own
  prereleases whose first identifier is a numeric below D) — each row
  differential-verified against `helm template` renderings of
  `semverCompare` (including `9.9.9-10` vs `-8.junk` boundaries).
  Chart flips: KPS's grafana operator dashboards without
  `matchLabels` abort under the corpus policy while a pre-1.14
  `kubeTargetVersionOverride` turns every dashboard document off
  exactly; vault's redundancy-zone combination now version-rejects at
  policy v1.29 (`helm template --kube-version 1.29.0` fails with
  "requires Kubernetes >= 1.35") while external mode stays dormant.
  Pinned by
  `kube_prometheus_stack_dashboard_gates_decode_the_version_policy`
  (CLI), the re-scoped vault pin's cluster-version case, and
  `capabilities_defaulted_semver_gates_decode_against_the_policy_version`
  (gen). Ten corpus fixtures adopted; the probe battery's 82 flips
  adjudicate to: the KPS declared-type-hint properties on newly-live
  dashboard reads (established declared-shape policy), the
  nfs-subdir/vault provider tightenings (template-fail /
  kubeconform-invalid at v1.29 strict), and template-verified widenings
  (cilium's dormant preflight PDB, vault's `ui.*` service fields under
  the disabled UI). Two KPS widenings are documented residuals in the
  tolerated direction (see the F107 residual entry).
- F56 self-ranged collection map lane (twelfth round, bounded): a
  self-ranged Scalar row at an array provider slot
  (`ForeignSchemaRestriction::ScalarCollection`) keeps an OPEN map lane
  beside the array rewrite — `range` iterates maps, and the loop body may
  render values as partial text, so an array-only type falsely rejected
  map-shaped sources (traefik's `resourceAttributes` flag loops at the
  container args slot; the direct and nested-include lanes are pinned by
  `scalar_collection_restriction_keeps_the_map_lane_beside_the_array`).
  The real chart's `template: {{ include "traefik.podTemplate" . |
  fromYaml | toYaml | nindent 4 }}` lane landed in the thirteenth round —
  see its entry below
- F17 coalesce-default rescue (thirteenth round): an equality against
  exactly the constant fallback of a `coalesce` over a STRINGIFIED
  identity also admits the Helm-empty spellings — the empty string
  always, plus every spelling a preceding `if eq $x "<nil>" { $x = "" }`
  normalization arm diverts (recorded at the branch join, where the
  divert header decodes exactly, and only when every identity-losing arm
  is an explained empty fold). The facts ride
  `HelperOutputMeta::{empty_fold_spellings, empty_rescue}` with
  agreement-or-drop merges; `eval_coalesce` records the rescue for the
  bounded two-arm shape whose alternatives are all explained (raw first
  arms abstain — their Helm-emptiness spans false/0/nil/empty
  collections). cilium's `kubeProxyReplacement: ""`, null, and even the
  literal `"<nil>"` spelling now render (all helm-verified). Pinned by
  the extended `cilium_kube_proxy_replacement_accepts_raw_booleans`
  (CLI) and `stringified_equality_binds_the_tostring_preimage` (gen)
- F24 stringified terminal truthiness (thirteenth round): truthiness of
  a total stringification tests the RENDERED text against the empty
  string — `"false"`, `"0"`, and `"<nil>"` are truthy strings. Two
  subjects decode exactly: a literal-key `dig` with an EMPTY-string
  default (present-with-non-empty-value; explicit null renders truthy
  `"<nil>"`, encoded through the new strict `Guard::HasKey`/
  `ConditionalGuard::HasKey` presence vocabulary — `Guard::Absent`
  deliberately keeps its null-as-absent semantics for the nil-safe
  selector lanes) and a direct selector (absent/null render `"<nil>"`,
  so only the raw empty string is falsy). Wired through the call and
  pipeline forms, `not_predicate` (which previously minted a WRONG
  raw-truthiness negation), and `or_predicate`'s truthy shortcut (which
  previously swallowed exactly-decodable pipeline disjuncts, poisoning
  the whole `or` — cilium's removed-option gates were entirely
  unenforced). cilium now aborts on `proxy.prometheus.enabled` false/
  true/null/0 and `proxy.prometheus.port` while ""/absent render (all
  helm-verified). Pinned by
  `cilium_removed_options_abort_even_when_disabled` (CLI) and
  `stringified_dig_truthiness_rejects_falsy_raw_spellings` /
  `direct_tostring_truthiness_is_a_rendering_test` (gen)
- F56 roundtrip partial-text discipline (thirteenth round): the
  `include … | fromYaml | toYaml` pod-template roundtrip re-lowers the
  helper's PROJECTED value, which flattened composed scalar parts into
  bare per-path renders — minting full-value provider preimages and
  string-lexical arms for paths that only render INSIDE flag text
  (traefik's `--…={{ $value }}` items scalar-restricted the
  resourceAttributes map to `type: null`). Three lowerings landed:
  (a) a projected scalar with literal text AROUND splices marks each
  path `HelperOutputMeta::partial_text` (splice-only part sets stay
  bare — contribution-set degradation merges ALTERNATIVE renders, and
  airflow's nil-aware `revisionHistoryLimit` picker must keep its
  provider int typing); (b) the fragment re-lowering keeps
  `partial_text` splices at `PartialScalar`, so provider typing and
  full-value lexical preimages abstain exactly like the direct lane's
  partial rows; (c) a self-ranged FRAGMENT use projects rangeability
  only (`anyOf [array, object]`) — the loop renders derived items, so
  the slot's item schema types the rendered items, never the source's
  members. traefik's `tracing.otlp.resourceAttributes` members render
  again under the committed provider bundle (string/int/list shapes;
  a non-rangeable scalar still aborts — all helm-verified). Pinned by
  `traefik_otlp_resource_attributes_render_as_flag_loops` (CLI, provider
  bundle) and
  `roundtrip_pod_templates_keep_ranged_flag_rows_at_item_depth` (gen)
- F111 nack root `global` null false rejection (nineteenth round): the
  base-typing source was the declared values.yaml mapping default — the
  presence-guarded member-host arm was already null-exact (`Absent`
  counts explicit null). A target whose member-host requirements ALL
  ride its own strict presence was only ever read through the nil-safe
  grouped form (`((.Values.global).labels)`), so its base host relaxes:
  a tree host drops `type: object`, a declared-default foreign base
  widens to `type: [object, null]` — helm's null-deletion renders
  `global: null` and every spelling is helm-verified (null/absent/maps
  render, scalars and `false` abort). Pinned by
  `nil_safe_grouped_receiver_with_declared_default_admits_null`; the
  KPS subchart-prefix keys (`kube-state-metrics`, `prometheus-node-exporter`)
  lost the same decorative base pin with polarities unchanged — their
  null spellings render but stay rejected by the subchart-composition
  lane, a separate pre-existing widening target.
- F110 provider regex dialect portability (nineteenth round): provider
  fragments normalize regex dialects at INGESTION
  (`ProviderSchemaFragment::new` /
  `helm_schema_core::normalize_schema_pattern_dialects`): a leading
  global `(?i)` case-folds to an exactly language-equal ECMA/Go-portable
  spelling (`^(?i)(abort|warn)?$` → `^([aA][bB][oO][rR][tT]|…)?$`,
  Unicode simple-fold partners for `k`/`s` included; unfoldable
  constructs abstain and stay reportable). The
  `schema_dialect_hygiene` gate walks every committed schema artifact
  the generator owns — corpus, gen, and CLI fixtures — validating the
  metaschema and compiling every schema-position `pattern` /
  `patternProperties` key under a real ECMA-262 engine (`regress`),
  instance-data keywords excluded. jenkins and KPS regenerated with the
  portable spelling; differential-verified against the RE2 semantics
  (`folded_patterns_accept_exactly_the_re2_language`).

## In progress

- **F28/F51 residual — ranged terminals and accumulator state (bounded;
  seventeenth round).** Landed in four pieces. (a) Compound ranged
  terminals negate to per-member alternatives: a member-field equality
  flips to the absence-tolerant `FieldNotEquals`, a negated nested-field
  truthiness to `FieldHelmTruthy`, and the member's own truthiness gate
  contributes the `HelmFalsy` escape — traefik's gateway HTTPS listeners
  now require `certificateRefs` (empty list rejects, non-HTTPS escapes;
  helm-verified; `compound_ranged_terminals_negate_to_member_alternatives`
  gen pin plus the traefik corpus fixture). (b) The velero `$breaking`
  accumulator survives: an ambient `RangeKeyEquals` concretizes stamped
  truthiness reductions (`Range(p) ∧ key = "k"` collapses to
  `HasKey(p, k)` with `p.*` wildcards rebound to the named member), so the
  final `fail $breaking` rejects both legacy fs-restore forms and the
  removed top-level keys exactly
  (`range_appended_error_accumulator_reaches_the_final_fail`, velero
  fixture flips helm-verified). (c) Helper-scope ranges over
  JSON-roundtripped dict members keep member identities:
  `json_roundtrip_identity` now roundtrips container structure member-wise
  (identity members stay identities, opaque members stay PRESENT), and a
  multi-candidate variable key no longer extends a whole-values-root
  choice arm
  (`helper_scope_ranges_bind_member_identities_in_fail_captures`).
  (d) Exact-range items beyond the alternatives' shared prefix carry an
  approximate conjunct on CAPTURE conjunctions only — rows keep the
  ordinary join — so a conditional `$opPathKeys` append cannot bind
  `from` on every patch member while kyverno's caller-joined label-merge
  lists keep their exact rows. The traefik `http3` remainder closed in
  the twenty-first round (definite overlay-entry sound subsets; see the
  Completed entry).
- **F32 residual — defaulted-comparison fallback literals (twenty-second
  round; closes the residual).** The core landed in the twentieth round:
  the CA `minAvailable`-alone false-reject is gone (a null-deleted
  `maxUnavailable` reads as nil, so the both-bounds terminal no longer
  fires on absence), and the positive piped membership
  `list "netkit" "netkit-l2" | has X` decodes to its exact equality
  disjunction in predicate, faithfulness, AND `not` position — cilium's
  tproxy+netkit exclusion rejects both netkit modes while veth/dormant
  states stay open, helm-verified. The twenty-second round closed the
  remainder: a `$mode := PATH | default LIT` binding records the literal
  fallback (`HelperOutputMeta::default_fallback`, exact-fact merge), and
  equality decoding adds the fallback arm — `eq $mode LIT` also holds
  where the path is falsy (`eq ∨ ¬truthy`), `ne $mode OTHER` likewise —
  bounded to the pure single-truthy-branch binding with no value
  transform. external-secrets' `serviceMonitor: {renderMode: null}`
  deletion state now selects the default literal's arm instead of the
  invalid-mode `fail`, while junk modes still abort every render
  (helm-verified both ways; pinned by
  `defaulted_binding_comparison_carries_the_fallback_arm` and
  `external_secrets_deleted_render_mode_selects_the_default_literal_arm`).
  REMAINING (separate note): signoz's `global.imagePullSecrets` scalar
  rejections (a nineteenth-round F30 win) re-widened — their arm's
  condition rode an `Absent(signoz-otel-gateway.postgresql.enabled)`
  disjunct that the subchart-default semantics correctly narrowed; the
  zookeeper-side abort needs its own capture to reject again.
- **F74 residual — parser exactness and transformed comparisons (bounded;
  seventeenth round).** (a) The `urlParse` operand pattern is now Go
  `url.Parse`'s accepted language, differential-verified against ~900k
  fuzz candidates with zero mismatches in either direction against the
  lenient oracle (`GODEBUG=urlstrictcolons=0`; Go 1.26's http-only
  strict-colon hardening stays a deliberate cross-version widening):
  exact authority (userinfo charset, bracket hosts as the shared
  `netip` IPv6 language with `%25` zones, plain-host escape and
  last-colon port rules), validated path/fragment escapes, raw queries,
  and control bytes legal only in fragments — airflow's `base_url` flips
  helm-verified (`url_parse_pattern_matches_the_go_verdicts` pins the Go
  verdict battery; the F87 IPv6 enumeration is now shared via
  `ipv6_pattern!`). (b) `trimPrefix`/`trimSuffix` escapes are TYPED
  (`LexicalEscape`): a single trim affix projects the capture language
  through the exact stripped-affix preimage (`^(?:P)(?:-jmx)?$`) instead
  of the contains-token exemption, so datadog's derived tag rejects
  mid-string `-jmx` spellings while suffixed versions trim-parse
  (`trim_suffix_projects_the_parser_domain_through_the_affix_preimage`,
  helm-verified). (c) The twenty-second round closed the multi-escape
  remainder: `regexReplaceAll "TOK.*$" X ""` records the typed
  `CutAtToken` erasure, and escape sets with at most one escape per edge
  position (leading affix, trailing affix, cut tail) compose as edge
  wraps in any application order — cilium's digest-strip/v-trim tag
  parse now projects `^(?:v)?(?:P)(?:@.*)?$` instead of the
  contains-token exemption. The `<0.9.0` comparator additionally
  projects through the same chain as a fail-position sound subset
  (`semver_transformed_operand`: the constraint pattern is
  `v?`-normalized and token-free, so both wraps are exact preimages):
  `v0.1.0` and digest-suffixed below-bound tags reject, `garbage` aborts
  the parse, and valid/digest/`latest` spellings render — helm-verified
  (`digest_strip_and_v_trim_compose_the_semver_bound_exactly`,
  `cilium_hubble_ui_tag_binds_the_transformed_semver_bound`). Escape
  mixes beyond one per edge keep the exemption fallback.
- **F80 residual — merge selection and provider attribution (bounded;
  eighteenth–nineteenth rounds).** The ordered-merge half landed in
  four pieces.
  (a) A definitely-empty literal destination (`mergeOverwrite (dict) a b`)
  drops out of the operand list, so fresh-dict merges keep the ordered
  layer form, and merge/`mergeOverwrite` truthiness decodes to the
  operands' disjunction (call-level via each operand's own reduction;
  a variable bound to `MergedLayers` gets the same disjunction lane, with
  undecodable layers abstaining to the approximate encoding instead of
  the old all-paths conjunction — cert-manager's `with (merge …)`
  nodeSelector gate now carries the exact or-condition). (b) A merge
  layer's sink typing moved out of the base lanes entirely (metadata
  field kinds included) onto synthesized layer arms: the whole payload
  binds under the layer's own truthiness plus every earlier layer's
  Helm-emptiness, per-key arms keep the finite `¬hasKey` refinement, and
  a sink whose provider fragment is unavailable falls back to its
  metadata string-map kind (keda's CRD annotations). (c) Decoded render
  gates ride the `ProviderSchemaUse` and scope the synthesized arms, so
  dormant states stay open — KPS `defaultRules.create/rules.*/disabled.*`
  false-spellings, keda `crds.install: false`, and velero's
  `deployNodeAgent: false` all accept junk while the live lanes reject
  (each polarity helm-verified; `fresh_dict_merge_layers_type_dynamic_members_with_shadow_refinement`
  pins the gen shape). KPS now rejects numeric members in per-alert
  `additionalRuleAnnotations` unconditionally-when-live and in per-group
  `additionalRuleGroupAnnotations` exactly where the per-alert layer is
  Helm-empty; the fully-shadowed corner stays accepted and the
  numeric-beside-unrelated-rule-keys corner stays open (dynamic-name
  per-key correlation is the documented F93 bound). Airflow's worker
  member lanes tightened as collateral (scalar `resources`, malformed
  `hostAliases`/`extraPorts` items now reject, helm-verified).
  The nil-scrub half landed in the nineteenth round. (d) The
  `removeNilFields` define shape is recognized by an exact ordered match
  of its action sequence (dict accumulator, one destructured range over
  DOT, self-recursive scrub for map members kept when nonempty, non-nil
  copy otherwise, `toYaml ACC`), and the call substitutes the operand's
  identity with a scrubbed marker (`HelperOutputMeta::nil_scrubbed`)
  instead of the opaque body summary. (e) Merged-member truthiness and
  `hasKey` decode through selector projections and scrubbed identities
  (the truthiness lane keys on the VALUE, not the expression spelling;
  undecodable layer sets fall through to the historic all-paths
  conjunction so ranged captures keep their existential encodings).
  (f) Binding-carried layer facts ride helper summaries into layered
  sink typing (`MergeLayersUse::via_binding`), bounded to
  scrub-involving merges — ordinary binding-carried merges keep the
  pre-layered routing their sibling dispatch arms rely on (bitnami's
  `tplvalues.render` string lane) — and a scrubbed layer entering a
  RANGE-member merge degrades to the opaque form so the per-set capture
  machinery keeps its arms. (g) The scrubbed layer's synthesized arms
  null-relax members recursively (the scrub drops nil members before
  the sink renders). The full chain — real `removeNilFields` +
  `workersMergeValues` + `airflowPodSecurityContext` — is pinned by
  `nil_scrubbed_merge_helper_layers_bind_candidate_provider_payloads`:
  string `runAsUser` rejects through either layer, the fully-shadowed
  corner stays open, and null members stay accepted.
  (h) The reroot chain landed in the twenty-second round, closing the
  chart flip: the scrub marker now survives on the set-free operand of a
  wildcard-involving merge (only the RANGE-member operand strips), nested
  `MergedLayers` flatten in precedence order for identity extraction
  (nesting is associative) in both the splice lowering and the
  binding-carried meta, a choice layer's constant-False `hasKey`
  alternatives drop as OR identities inside the layer union (the
  `concat (list (dict "name" "default")) sets` literal entry), and
  negated member-quantified guards encode as `anyOf[¬∀, ∀¬]` — the
  `∀ members violate` arm holds vacuously on airflow's default empty
  `sets: []`, so the deeper layers' synthesized arms fire there. Rerouted
  arms whose unlowerable row conditions HARD-NEGATE foreign-family
  selections keep the pre-layered routing instead (the deprecated
  `workers.securityContext` scalar stays open behind a live
  `securityContexts.pod` — `hard_negation_paths` in the signal builder).
  The real chart now rejects string `runAsUser` through the base and
  celery layers after the per-set reroot while the shadowed corner,
  null-scrubbed members, and the per-set capture arms all hold
  (`airflow_rerooted_worker_lanes_bind_layered_provider_payloads`,
  `rerooted_worker_set_merges_keep_layered_provider_payloads`,
  `per_set_merge_layers_bind_without_the_reroot`; the round-8/17 pins in
  `airflow_worker_set_overrides_bind_strict_member_kinds` and the
  fallback-priority pin stay green). REMAINING (documented quantifier
  bounds): the `∀¬` arm under-fires on mixed member sets (a set member
  supplying the key hides the sibling members' unshadowed iterations —
  accept-direction), and with `enableDefault: false` beside empty `sets`
  the ungated arm can still fire for renders that never happen (the
  pre-existing member-local-wildcard widening class). Adjudication notes:
  signoz's clickhouse `settings`/`profiles` payload reference loss
  (eighteenth round) stands — polarities unchanged; the nineteenth
  round's scrub short-circuit drops the summary-derived iterable arm on
  `workers.celery` (scalar spellings of the whole subtree now accept —
  a bounded widening; helm aborts ranging a scalar), and the
  bitnami/redis/keda condition spellings re-encode with zero acceptance
  flips across the probe batteries.
- **F104 residual — wrapper consumers before tree rewrite (seventeenth
  round; closes the residual).** The interpreter snapshots
  `strict_string_capture_paths()` — string contracts plus
  `ValueType(string)`/`ValuePattern` fail-capture subjects, branch
  conditions included, since engines guard their whole body with an
  idempotence flag exactly as conditional as the rewrite — at the FIRST
  values-root wrapper rewrite observed in a body; the snapshot rides
  summary → document → contract
  (`values_program_wrapper_exclusions`) and the gen wrapper pass skips
  the wrapper alternative at those exact property paths (pathless edges
  — items, additionalProperties, `$defs` — stay outside the exclusion
  namespace). nats: wrappers at `nameOverride`/`fullnameOverride`
  (consumed raw by `fullname | trunc`/`contains`) now reject while the
  tolerant pre-rewrite `.name` default selections and every post-rewrite
  consumer keep theirs — all helm-verified
  (`nats_pre_rewrite_strict_consumers_reject_wrapper_programs`). The
  root REPLACE wrapper also became representable: `wrap_document_root`
  unions the document's own value domain with the wrapper alternative,
  so `{"$tplYaml": …}` as the whole values document validates while the
  spread form still rejects.
- **F107 residual — falsy `dig` hosts behind decoded gates (seventeenth
  round; closes the residual).** helm 4's `dig` splits its contract and
  the analyzer now mirrors it exactly: the SUBJECT is type-asserted
  before any missing-key handling — a present-but-NULL subject aborts —
  so `eval_dig` records a `DigSubject` capture whose strict
  `Guard::HasKey` conjunct self-scopes the claim and lowers to the new
  null-intolerant `FailValueRequirement::SchemaTypeEvenNull`; an
  INTERMEDIATE step falls back to the dig default when nil but aborts on
  any other non-map (Helm-falsy scalars included), the exact `¬Absent`
  scope. Presence-scoped TYPE arms route the base to the guarded-only
  lane like the self-truthy case, so dormant states stay open. KPS:
  live null/junk/false `customRules` and `additionalRuleAnnotations`
  reject, maps render, and `defaultRules.create: false` keeps every
  spelling dormant — all helm-verified
  (`kube_prometheus_stack_dig_subjects_bind_the_even_null_contract`,
  `dig_subjects_reject_null_while_intermediate_nils_fall_back`);
  trivy-operator's nulled `trivy.resources` keeps rendering while its
  falsy non-nil spellings now reject, and cilium's five dig hosts
  reject null exactly (helm-verified each way). The
  `MEMBER_ACCESS_GUARD_FANOUT` factoring the reverted attempt needed
  turned out unnecessary — the self-scoping `HasKey` conjunct alone
  keeps unrelated typing intact. The vault HCL CONFIG placeholder fail
  stays open by design (Go `(?m)` has no Draft-07 pattern encoding).
  Adjudication note: loki's `rulerConfig`/`storage_config` dig subjects
  abort helm on null but their captures still abstain under ambient
  approximates — a documented widening.
- **F108 residual — NATS JSON Patch grammar through the helper range
  (seventeenth round; bounded).** With member identities riding the
  helper/json roundtrip (the F28/F51 machinery above), the
  `_jsonpatch.tpl` op grammar binds through `nats.loadMergePatch`:
  unknown `op`, missing `op`, and missing `path` reject on
  `service.patch`/`statefulSet.patch` members while valid operations,
  the empty default, and the `$tplYamlSpread` wrapper-item lane render —
  all helm-verified
  (`nats_jsonpatch_ops_bind_through_the_helper_range`). The engine's
  scaffolding fields (`fromKey`, `pathLastMap`, …) stay IR-internal and
  mint no schema properties, and sentinel-keyed evidence is scrubbed at
  contract finalization so the recursive walker's `$tplYaml` probes no
  longer seed root values properties. The twenty-second round closed the
  per-op remainder: helper-dict root fields no longer shadow same-named
  range variables (`locals_with_roots` keeps template bindings — the
  call dict's `patch` field was displacing the `$patch` member), and
  `value_has_key` decodes a JSON-roundtripped OutputPath identity (keys
  survive the roundtrip; any transform, branch condition, or key removal
  abstains), so the `and (or (eq .op …)) (not (hasKey . KEY))` fails
  negate to their exact `AnyOf[[FieldNotEquals…], [HasMember]]` arms:
  `copy`/`move` without `from` and `add`/`replace`/`test` without
  `value` reject on `service.patch` members while complete patches of
  every op render — all helm-verified
  (`nats_jsonpatch_per_op_requirements_bind_through_the_helper_range`,
  `per_op_requirement_binds_in_a_direct_range`,
  `per_op_requirement_binds_through_the_helper_roundtrip`).

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
  them; the audited checksum family is Completed above.
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
  Rejected; the representable singleton lane is Completed above.
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

# Un-spaced pipe misparse — fix + multi-agent adversarial review

Bug report (v0.0.6): `{{ toYaml .| nindent 8 }}` (no whitespace before `|`)
was silently misparsed, emitting an unsatisfiable root-level schema clause
("if `podLabels` is truthy it must be null-or-string") while Helm rendered
the chart correctly. Four downstream luup2 charts were blocked and carry
override workarounds pending this fix.

## Root cause (confirmed empirically, tree-sitter CLI on the vendored parser)

The vendored go-template grammar modeled command arguments as full
pipelines separated by a literal single-space token, so argument boundaries
depended on the exact whitespace byte:

- `{{ toYaml .| nindent 8 }}` → `toYaml((. | nindent), 8)` — `nindent`
  zero-arg, `8` re-attached to `toYaml`; downstream, "`.` piped into
  nindent" typed the `with`-subject as a string → the reported clause.
- `call_with_unfolded_pipe` (expr.rs) was a partial rescue that only
  covered pipe targets WITHOUT arguments (`X| quote`, `)| sha256sum`) —
  which is exactly why the corpus and unit suites stayed green while
  `X| nindent 8` broke.
- Sibling bugs from the same design: `{{ f a<TAB>b }}` silently parsed as
  `f(a(b))`; newline-separated arguments only parsed when the continuation
  line began with a space.

## The fix (grammar-level; Go's documented grammar, not another rescue layer)

`make_grammar.js` (vendored, locally owned; regenerate with the pinned
tree-sitter-cli 0.26.10 via `npm ci && npm run generate`):

1. `argument_list` now takes `_operand`s — Go's "Arguments" production:
   `_expression`, `parenthesized_pipeline`, or a niladic function name
   (`operand_call` aliased to `function_call`, so node shapes for valid
   input are unchanged). A bare pipeline can no longer be an argument, so
   an un-parenthesized `|` always splits the enclosing command — spacing
   is irrelevant by construction.
2. The argument separator became `token(/[ \t\r\n]+/)` (Go's isSpace).
3. `call_with_unfolded_pipe` and the Pipeline-flatten arm of
   `collect_pipeline_stages` were DELETED (structural path replaced the
   heuristic).

## Verification

- Grammar corpus: 95/95 (88 pre-existing unchanged; 7 new class-pinning
  cases: unspaced pipes ×4, tab/newline separators, range `=` form).
- Workspace unit suite: 1320/1320.
- **Differential oracle vs Go itself**: scratch harness (godump: Go
  `text/template/parse` with SkipFuncCheck; rsdump: canonical dump of
  `parse_action_expressions`; shared S-expr dialect) over 41,100 actions —
  every `{{…}}` action extracted from testdata/charts plus a synthetic
  adversarial battery and un-spacing mutants. Result: ZERO structural
  divergences; residue is harness artifacts (actions Go rejects because
  extraction loses context; one int-vs-float decimal rendering of 2^60).
- Whole-chart regression test (crates/helm-schema/tests/
  unspaced_pipe_regression.rs): pipe spacing never changes the emitted
  schema; the schema accepts the values Helm renders and still rejects a
  scalar where the CRD requires a map.
- Behavior pin (crates/helm-schema-cli/tests/chart_reloader.rs): reloader
  `podMonitor.tlsConfig` map accepted / scalar rejected, so a future
  fixture regeneration cannot silently re-pin the defect.

## Corpus fixture flips — adjudicated per the CLAUDE.md protocol

Six integration failures after the fix, all stale fixtures pinning the old
broken output; each flip probed old-vs-new with a compiled jsonschema
prober over composed defaults and adjudicated against `helm template`:

| chart | trigger | probe | old | new | helm ground truth |
|---|---|---|---|---|---|
| external-dns | `toYaml .| nindent 8` (serviceMonitor.tlsConfig ×2) | tlsConfig map | REJECT | ACCEPT | renders — false rejection FIXED |
| oauth2-proxy | same (metrics.serviceMonitor.tlsConfig) | tlsConfig map | REJECT | ACCEPT | renders — FIXED |
| prometheus | pushgateway subchart servicemonitor `.| nindent 6` | tlsConfig map | REJECT | ACCEPT | renders — FIXED |
| reloader | podMonitor/serviceMonitor/networkpolicy `.| nindent 6` | tlsConfig map | REJECT | ACCEPT | renders — FIXED |
| (all four) | — | tlsConfig scalar | ACCEPT | REJECT | renders an invalid CRD object; matches the typing the SPACED charts (loki, kube-prometheus-stack) always had — correct tighten |
| jenkins | `{{ tpl $val $| indent 4 }}` (JCasC configScripts) | configScripts int | ACCEPT | REJECT | **helm FAILS** ("wrong type for value; expected string; got float64") — correct tighten |
| lean lane: schema-emission-temporal-wrapper | temporal-0.62.0.tgz contains `toYaml .| nindent 8` (admintools) + pushgateway `.| nindent 6` | — | — | — | same class inside the vendored archive |

All five charts' own values.yaml still validate against the new schemas.

## Multi-agent adversarial review (cross-vendor, read-only)

Round 1 — opus-5 (Rust lens) + gpt-5.6-sol via codex (Go-fidelity lens),
fable-5 orchestrating with independent verification. Both reviewers
CONFIRMED the central fix and could not construct a Go-valid input that the
new grammar + conversion mistypes (21 distinct constructed attacks failed,
including the redis-ha `)| sha256sum` shape, method-call arguments,
trim-marker interactions, CRLF, recovery trees). Findings and dispositions:

- opus F1 (blocker): stale corpus fixtures — confirmed by the integration
  run; regenerated + adjudicated above; behavior pin added.
- opus F2 (major): traefik hex-float behavior change — traefik fixture did
  NOT flip (hex floats only feed mulf/divf arithmetic, not emitted shapes).
- opus F3 (minor): non-finite hex floats (NaN/inf) — FIXED
  (`is_finite().then_some`), tests added.
- opus F4 (minor): missing invariant test for "no bare Pipeline in
  Call.args" — ADDED (`call_arguments_are_never_bare_pipelines`).
- opus F5 (note): chart-level test never exercised fully-unspaced form —
  FIXED (separator substitution now covers `|`).
- codex 1 (blocker): `range $i, $v = …` (assignment form) mangled by
  error recovery — pre-existing, same class — FIXED in the grammar
  (`choice(token(':='), '=')`), corpus + expr tests added.
- codex 2 (major): identifiers rejected Unicode digits (Go uses
  unicode.IsDigit) — FIXED (`/\p{Nd}/`), test added.
- codex 3 (major): octal string escapes preserved verbatim — FIXED for
  ASCII bytes (helper names now resolve); non-ASCII octal bytes abstain
  verbatim (documented: Go assembles them byte-wise, a char-based decoder
  cannot represent a lone fragment).
- codex 4 (minor): `now.Year` operand degraded to Unknown — FIXED
  (identifier → zero-arg Call), test added.
- codex 5 (minor): i64::MIN literal failed to decode — FIXED (i128 path);
  complex/imaginary/rune literals remain Unknown by design (safe
  abstention, zero corpus incidence) — accepted divergence.
- codex 6 / opus F6 (notes): the grammar ACCEPTS some invalid-Go shapes
  (`f(a)`, `f"x"`, `{{$x=.}}`, `-}}` without preceding space, `. x` as a
  field) — accepted divergences: such charts fail `helm template`, so no
  false schema can reach a rendering chart; tightening them buys no
  precision for valid charts and costs grammar complexity.

### Round 2 (scoped: verify round-1 fixes hold, attack only the new surface)

Both vendors independently confirmed every round-1 fix held (codex: 10
constructed attacks including nested control flow, `else` branches, octal
boundary runs, integer bounds in all five bases; opus: 9 attack classes
including static analysis of the regenerated lexer character-range tables
proving the \p{Nd} change is a strict superset and the `=` edit forked no
new terminal). Verdicts: SHIP-WITH-FIXES from both. New findings, all
implemented:

- codex R2-1 / opus note 5 (converging finding): a Go-valid identifier
  STARTING with a Unicode digit (`.Values.١key`) hit error recovery and
  fabricated the WRONG values path `Values.key`. Fixed by principle, not
  by lexer surgery: `convert_pipeline` now abstains (`Unknown`) for any
  expression node carrying a recovery error (string literals excepted —
  their decoder already preserves undecodable content verbatim while
  keeping the string type). Leading-Nd identifiers now abstain instead of
  mistyping — the safe direction; full lexer support recorded as an
  accepted divergence (zero real-world incidence).
- codex R2-2: hex-float scaling under/overflowed through intermediate
  powers (`0x1p-1024` → 0 instead of a subnormal; `0x.1p1024` → needless
  Unknown; `0x0p10000` should be 0 as in Go). Fixed with stepwise
  power-of-two scaling (`scale_by_pow2`).
- opus R2-1: the non-finite guard only covered the hex spelling; decimal
  `1e400` still produced `Float(inf)` (Rust's `f64::from_str` returns
  Ok(inf) where Go errors). Guard hoisted to `parse_float_literal` for
  both spellings.
- opus R2-2: `\xC3`-style non-ASCII hex escapes decoded as Latin-1 code
  points (`"caf\xc3\xa9"` → "cafÃ©"), fabricating a wrong guard constant
  where the byte-identical octal spelling abstained. `\x` ≥ 0x80 now
  abstains verbatim, matching the octal policy (`\u`/`\U` are code points
  and still decode).
- opus R2-3: stale escape-list doc (`\0`) corrected.
- opus R2-4: the `=`-range regression is now pinned at the API where the
  damage occurred (`range_destructured_*`), in
  crates/helm-schema-ast/tests/range_structure.rs.

Post-round-2 differential sweep: 41,100 actions, 40,821 agree, zero
structural divergences, one decimal-rendering artifact (2^60 printed as
int by Go, shortest-float by Rust — values identical).

Convergence assessment: round 2 attacked the actual risk (both reviewers
constructed inputs against the central mechanism and the new edits, and
every attack failed), re-confirmed all round-1 fixes, and its new findings
were in a different, deliberately-hunted category (decoder edge fidelity),
now closed and test-pinned. Stopping here — a further round would re-list
the standing accepted divergences.

### Post-round-2 corpus find: Go tolerates a trailing pipe

The final integration enumeration flipped one more chart — datadog — and
adjudication traced it to `datadog-operator/templates/service_account.yaml`:
`{{- toYaml .Values.serviceAccount.annotations | nindent 4 | }}` — a
TRAILING pipe. Verified against Go directly: the pipeline parser closes a
pipeline when `}}` or `)` follows `|` (the trailing pipe is silently
dropped; `| |` and a leading `|` stay errors), and Helm renders the chart.
The grammar previously required an operand after `|`, so this shape went
through error recovery: before round 2 the partial conversion kept the
constraint by luck; round 2's abstention (correctly) dropped it, loosening
`operator.serviceAccount.annotations`. Fixed structurally: the pipe's
right-hand side is optional in `chained_pipeline`, a lone surviving stage
collapses to the stage itself, corpus (96/96) and expr equivalence tests
pin `x |` ≡ `x` for delimiter, paren, and `if`-header positions. With the
grammar fix the datadog fixture matches its ORIGINAL pin again — no flip.

The differential harness had missed this shape because its chart glob
skipped subchart template directories; with the glob fixed the sweep
covers 46,061 actions (subcharts included) with zero structural
divergences vs Go.

## Accepted divergences from Go (documented, deliberate)

1. Grammar accepts a superset of Go for inputs Go rejects (see codex 6 /
   opus F6 list) — analyzer-benign: such charts fail `helm template`.
2. Complex, imaginary, and rune literals degrade to `Unknown` (abstain).
3. Non-ASCII octal and `\x` string escapes preserved verbatim (abstain);
   Go assembles them into UTF-8 byte-wise.
4. Hex-float rounding on very long mantissas may differ in the last ULP;
   an exponent below the subnormal range underflows to 0 where Go reports
   a range error.
5. `TemplateExpr::Call.function` carries method-selector text (`.x.y.M`,
   `$.Files.Get`) — the existing modeling; capability decoding matches
   both `.Capabilities…` and `$.Capabilities…` spellings via `ends_with`.
6. Identifiers/fields STARTING with a non-ASCII Unicode digit
   (`.Values.١key`, valid in Go) abstain as `Unknown` rather than being
   typed (and, since round 2, can no longer be silently mistyped).
7. In Go, `range $i, $v = …` assigns pre-declared outer variables whose
   values survive the loop, while `:=` bindings are popped at `{{ end }}`;
   the analysis models both forms identically. A post-loop read of such a
   variable is the (never observed in the corpus) divergent case.

## Follow-ups (out of scope for this fix, recorded)

- `VariableDefinition`/`Assignment.name` includes `$` while
  `Variable` does not; every consumer compensates with
  `trim_start_matches('$')`. Docs corrected in this round; unifying the
  representation (and deleting the trims) is a separate cleanup.
- The layout parser's behavior for genuinely multi-line actions (now
  parseable thanks to the newline separator) is unexercised by the corpus;
  the expr layer is verified, the line-overlay path is not.
- `--trace-output` emits a Perfetto trace (by design); the reporter
  expected an analysis trace — a docs/UX nuance, not a defect.
- Reporter's four luup2 charts carry `helm-schema.yaml` +
  `values.helm-schema.override.json` workaround pairs to delete after this
  fix ships.

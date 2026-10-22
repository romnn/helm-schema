#!/usr/bin/env bash
# R0 rework probes: source-derived templates from the Helm/Go semantics review (astra, agentmux run
# 20261022T052741-f8a5b99b), confirmed against pinned Helm 4.2.3. Each probe is a one-template chart.
# Usage: plan/value-algebra-r0-probes/review.sh > plan/value-algebra-r0-probes/review-results.txt
set -u
HELM="${HELM:-/Users/roman/.local/share/mise/installs/helm/4.2.3/darwin-arm64/helm}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCRATCH="$ROOT/target/r0-probes/review"
rm -rf "$SCRATCH"; mkdir -p "$SCRATCH"

# chart NAME VALUES_YAML TEMPLATE: writes the chart and prints its directory.
chart() {
  local dir="$SCRATCH/$1"; mkdir -p "$dir/templates"
  printf 'apiVersion: v2\nname: %s\nversion: 0.1.0\n' "$1" > "$dir/Chart.yaml"
  printf '%s\n' "$2" > "$dir/values.yaml"
  printf 'v: %s\n' "$3" > "$dir/templates/cm.yaml"
  printf '%s' "$dir"
}

# render NAME DIR [extra helm args...]: renders and prints the record.
render() {
  local name="$1" dir="$2"; shift 2
  local out code
  out="$("$HELM" template p "$dir" --show-only templates/cm.yaml "$@" 2>&1)"; code=$?
  printf '### %s\nvalues.yaml: %s\ntemplate: %s\nargs: %s\nexit: %s\noutput:\n%s\n\n' \
    "$name" "$(tr '\n' ' ' < "$dir/values.yaml")" "$(cat "$dir/templates/cm.yaml")" "$*" "$code" \
    "$(printf '%s\n' "$out" | grep -v '^---$\|^# Source:\|^Use --debug\|^$' | sed -e 's/^/  /' -e "s#$SCRATCH#\$SCRATCH#g")"
}

probe() { local name="$1" values="$2" template="$3"; shift 3; render "$name" "$(chart "$name" "$values" "$template")" "$@"; }

"$HELM" version
echo

# 1. A stale guard over an alias write: the body never runs.
probe a01-stale-guard-alias '{}' '{{ $d := dict "c" false }}{{ $a := $d }}{{ $_ := set $a "c" true }}{{ if not (get $d "c") }}{{ $_ := ternary "a" "b" (dict) }}{{ end }}ok'
# 2. Slices: sortAlpha sorts a []string in place through a slice view; append copies.
probe a02-sortalpha-in-place '{}' '{{ $xs := splitList "," "b,a" }}{{ $ys := slice $xs 0 2 }}{{ $_ := sortAlpha $ys }}{{ index $xs 0 }}'
probe a02b-sortalpha-interface-list '{}' '{{ $xs := list "b" "a" }}{{ $_ := sortAlpha $xs }}{{ index $xs 0 }}'
probe a02c-append-copies '{}' '{{ $xs := list 1 }}{{ $ys := append $xs 2 }}{{ len $xs }}/{{ len $ys }}'
# 3. Concrete types beyond kind; method lookup on chartutil Values.
probe a03-set-on-string-map '{}' '{{ set (split "," "a,b") "_0" "x" }}'
probe a03b-index-string-uint8 '{}' '{{ typeOf (index "A" 0) }}'
probe a03c-values-method-wins '{}' '{{ $_ := set .Values "YAML" true }}{{ typeOf .Values.YAML }}/{{ typeOf (index .Values "YAML") }}'
# 4. Typed nil map; lookup chains, grouping, get and index on absence.
probe a04-typed-nil-map '{}' '{{ $m := fromJson "null" }}{{ $_ := required "r" $m }}{{ eq $m nil }}/{{ typeOf $m }}'
probe a04b-typed-nil-map-set '{}' '{{ $m := fromJson "null" }}{{ $_ := set $m "x" 1 }}'
probe a04c-missing-chain '{}' '{{ .Values.missing.x }}'
probe a04d-missing-grouped '{}' '{{ (.Values.missing).x | default "ok" }}'
probe a04e-get-index-absent '{}' '{{ print (get .Values "missing" | quote) "/" (index .Values "missing") "/" (typeOf (index .Values "missing")) | quote }}'
# 5. Selectors test emptiness after eager arguments; fixed map returns include error maps.
probe a05-coalesce-after-unset '{}' '{{ $d := dict "x" 1 }}{{ coalesce $d (unset $d "x") "fallback" }}'
probe a05b-required-fromyaml-scalar '{}' '{{ $_ := required "r" (fromYaml "true") }}{{ print (typeOf (fromYaml "true")) "/" (fromYaml "true") | quote }}'
probe a05c-mustfromjson-scalar '{}' '{{ typeOf (mustFromJson "true") }}'
# 6. Short-circuit special forms; parameter-by-parameter validation; literal resolution.
probe a06-and-short-circuit '{}' '{{ and false (fail "x") }}'
probe a06b-piped-fail-and '{}' '{{ fail "x" | and false }}'
probe a06c-required-message-first '{}' '{{ required 0 (fail "later") }}'
probe a06d-round-direct-literal '{}' '{{ round 1 0 1 }}'
probe a06e-round-grouped-literal '{}' '{{ round 1 0 (1) }}'
# 7. Go int parameters after a write, and coercing interface parameters.
probe a07-int-after-set 'x: 3' '{{ $_ := set .Values "x" (int .Values.x) }}{{ trunc .Values.x "abcdef" }}'
probe a07b-slice-coerces-float 'x: 1' '{{ slice (list "a" "b") .Values.x }}'
probe a07c-toyaml-fromyaml-float '{}' '{{ typeOf (index (fromYaml (toYaml (dict "n" (int 3)))) "n") }}'
probe a07d-set-json-float '{}' '{{ typeOf .Values.x }}' --set-json x=3
probe a07e-set-string '{}' '{{ typeOf .Values.x }}' --set-string x=3
# 8. tpl clones the namespace; include keeps <no value>, tpl strips it.
probe a08-tpl-namespace-clone '{}' '{{ define "h" }}outer{{ end }}{{ tpl "{{define \"h\"}}inner{{end}}{{include \"h\" .}}" . }}/{{ include "h" . }}'
probe a08b-no-value-boundaries '{}' '{{ define "n" }}<no value>{{ end }}{{ len (include "n" .) }}/{{ len (tpl "<no value>" .) }}'
probe a08c-helper-frame '{}' '{{ define "f" }}{{ $ | typeOf }}{{ end }}{{ $x := 1 }}{{ include "f" "arg" }}'
# 9. deepCopy copies per occurrence; map range iterates a sorted entry snapshot.
probe a09-deepcopy-breaks-sharing '{}' '{{ $m := dict "x" 0 }}{{ $c := deepCopy (dict "a" $m "b" $m) }}{{ $_ := set $c.a "x" 1 }}{{ $c.b.x }}'
probe a09b-pick-keeps-nested '{}' '{{ $m := dict "x" 0 }}{{ $p := pick (dict "a" $m) "a" }}{{ $_ := set $p.a "x" 1 }}{{ $m.x }}'
probe a09c-range-snapshot '{}' '{{ $d := dict "a" 1 "b" 2 }}{{ range $k, $v := $d }}{{ $_ := unset $d "b" }}{{ $k }}{{ end }}'
# 11. Subchart values views share the child's map with the parent.
dir="$(chart a11-subcharts-share '{}' '{{ $_ := set .Subcharts.child.Values "x" true }}{{ .Values.child.x }}')"
mkdir -p "$dir/charts/child/templates"
printf 'apiVersion: v2\nname: child\nversion: 0.1.0\n' > "$dir/charts/child/Chart.yaml"
printf 'y: 1\n' > "$dir/charts/child/values.yaml"
render a11-subcharts-share "$dir"

# 10. A nondeterministic helper: Helm both succeeds and aborts across runs, so no cache may fix one result.
dir="$(chart a10-randint-helper '{}' '{{ define "r" }}{{ randInt 0 2 }}{{ end }}{{ if eq (include "r" .) (include "r" .) }}{{ fail "same" }}{{ end }}ok')"
ok=0; fail=0
for _ in $(seq 1 30); do
  if "$HELM" template p "$dir" --show-only templates/cm.yaml > /dev/null 2>&1; then ok=$((ok+1)); else fail=$((fail+1)); fi
done
printf '### a10-randint-helper\ntemplate: %s\nruns: 30\nexit 0: %s\nexit 1: %s\n\n' "$(cat "$dir/templates/cm.yaml")" "$ok" "$fail"

# Round 2 of the semantics review (agentmux run 20261022T052741-f8a5b99b, turn 1).
probe b01-tpl-literal-writes '{}' '{{ tpl "{{ $_ := set .Values \"c\" true }}" . }}{{ .Values.c }}'
probe b02-index-slot-is-live '{}' '{{ $s := splitList "," "b,a" }}{{ $x := index $s 0 }}{{ $_ := sortAlpha $s }}{{ $x }}'
probe b02b-slice-range-reads-live-slots '{}' '{{ $s := splitList "," "b,a" }}{{ range $i, $v := $s }}{{ if eq $i 0 }}{{ $_ := sortAlpha $s }}{{ end }}{{ $v }}{{ end }}'
probe b03-release-is-mutable '{}' '{{ $_ := set .Release "Name" "changed" }}{{ .Release.Name }}'
probe b03b-chart-is-mutable '{}' '{{ $_ := set .Chart "Name" "changed" }}{{ .Chart.Name }}'
probe b04-round-float-cutoff 'cutoff: 0.5' '{{ round 1 0 .Values.cutoff }}'
probe b04b-round-int-precision 'p: 0' '{{ round 1 .Values.p }}'
probe b05-unset-nil-map '{}' '{{ eq (unset (fromJson "null") "x") nil }}'
probe b05b-index-absent-typed-map '{}' '{{ typeOf (index (split "," "a") "missing") | quote }}'
probe b05c-method-precedes-key 'Table: x' '{{ .Values.Table }}'
probe b07-pipeline-value-validated '{}' '{{ 0 | lower }}'
probe b07b-pipeline-final-variadic '{}' '{{ 1 | printf "%v-%v" 2 }}'
dir="$(chart b06-shuffle-helper '{}' '{{ define "r" }}{{ shuffle "ab" }}{{ end }}{{ eq (include "r" .) (include "r" .) }}')"
t=0; f=0
for _ in $(seq 1 30); do
  case "$("$HELM" template p "$dir" --show-only templates/cm.yaml 2>&1)" in *"v: true"*) t=$((t+1));; *"v: false"*) f=$((f+1));; esac
done
printf '### b06-shuffle-helper\ntemplate: %s\nruns: 30\nv: true: %s\nv: false: %s\n\n' "$(cat "$dir/templates/cm.yaml")" "$t" "$f"

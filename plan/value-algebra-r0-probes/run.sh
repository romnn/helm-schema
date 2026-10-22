#!/usr/bin/env bash
# R0 Helm probes: each probe is a one-template chart rendered by pinned Helm 4.2.3.
# Usage: plan/value-algebra-r0-probes/run.sh > plan/value-algebra-r0-probes/results.txt
set -u
HELM="${HELM:-/Users/roman/.local/share/mise/installs/helm/4.2.3/darwin-arm64/helm}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCRATCH="$ROOT/target/r0-probes"
mkdir -p "$SCRATCH"

# probe NAME VALUES_YAML TEMPLATE [extra helm args...]
probe() {
  local name="$1" values="$2" template="$3"; shift 3
  local dir="$SCRATCH/$name"
  rm -rf "$dir"; mkdir -p "$dir/templates"
  printf 'apiVersion: v2\nname: %s\nversion: 0.1.0\n' "$name" > "$dir/Chart.yaml"
  printf '%s\n' "$values" > "$dir/values.yaml"
  printf '%s\n' "$template" > "$dir/templates/cm.yaml"
  local out code
  out="$("$HELM" template p "$dir" --show-only templates/cm.yaml "$@" 2>&1)"; code=$?
  printf '### %s\nvalues.yaml: %s\ntemplate: %s\ncommand: helm template p %s --show-only templates/cm.yaml %s\nexit: %s\noutput:\n%s\n\n' \
    "$name" "$values" "$template" "\$SCRATCH/$name" "$*" "$code" \
    "$(printf '%s\n' "$out" | sed -e 's/^/  /' -e "s#$SCRATCH#\$SCRATCH#g")"
}

"$HELM" version
echo

# Q1: Go int vs Sprig int64 at int parameters.
probe int-trunc-add '{}' 'v: {{ trunc (add 1 2) "abcdef" }}'
probe int-trunc-len '{}' 'v: {{ trunc (len "abc") "abcdef" }}'
probe int-trunc-literal '{}' 'v: {{ trunc 3 "abcdef" }}'
probe int-trunc-int '{}' 'v: {{ trunc (int 3) "abcdef" }}'
probe int-trunc-int64 '{}' 'v: {{ trunc (int64 3) "abcdef" }}'
probe int-trunc-atoi '{}' 'v: {{ trunc (atoi "3") "abcdef" }}'
probe int-trunc-float-literal '{}' 'v: {{ trunc 3.0 "abcdef" }}'
probe int-trunc-grouped-float '{}' 'v: {{ trunc (3.0) "abcdef" }}'
probe int-trunc-values-yaml 'num: 3' 'v: {{ trunc .Values.num "abcdef" }}'
probe int-trunc-values-set '{}' 'v: {{ trunc .Values.num "abcdef" }}' --set num=3
probe int-repeat-add '{}' 'v: {{ repeat (add 1 1) "a" }}'
probe int-repeat-len '{}' 'v: {{ repeat (len "ab") "a" }}'
probe int-substr-add '{}' 'v: {{ substr 0 (add 1 1) "abc" }}'
probe int-indent-add '{}' 'v: {{ indent (add 1 1) "x" | quote }}'
probe int-abbrev-add '{}' 'v: {{ abbrev (add 4 0) "abcdefg" }}'
probe int-until-add '{}' 'v: {{ until (add 1 1) }}'
probe int-splitn-add '{}' 'v: {{ splitn "," (add 1 1) "a,b,c" }}'
probe int-index-add '{}' 'v: {{ index (list "a" "b") (add 0 1) }}'
probe int-eq-add-literal '{}' 'v: {{ eq (add 1 2) 3 }}'
probe int-eq-len-add '{}' 'v: {{ eq (len "abc") (add 1 2) }}'
probe int-eq-values-yaml-literal 'num: 3' 'v: {{ eq .Values.num 3 }}'
probe int-range-add '{}' 'v: [{{ range (add 1 1) }}{{ . }},{{ end }}]'
probe int-range-values-yaml 'num: 2' 'v: [{{ range .Values.num }}{{ . }},{{ end }}]'
probe int-range-values-set '{}' 'v: [{{ range .Values.num }}{{ . }},{{ end }}]' --set num=2
probe int-eq-values-set-literal '{}' 'v: {{ eq .Values.num 3 }}' --set num=3
probe int-typeof-values 'num: 3' 'v: {{ typeOf .Values.num }}/{{ typeOf (int .Values.num) }}/{{ typeOf (add .Values.num 0) }}'
probe int-typeof-values-set '{}' 'v: {{ typeOf .Values.num }}' --set num=3
probe int-trunc-int-of-values 'num: 3' 'v: {{ trunc (int .Values.num) "abcdef" }}'
probe int-typeof-add-len '{}' 'v: {{ typeOf (add 1 2) }}/{{ typeOf (len "a") }}/{{ typeOf (int 1) }}/{{ typeOf 1 }}'

# Q2: typeIs over omit.
probe typeis-omit-string '{}' 'v: {{ if typeIs "string" (omit (dict "a" 1) "b") }}x{{ else }}y{{ end }}'
probe typeis-omit-map '{}' 'v: {{ typeOf (omit (dict "a" 1) "b") }}/{{ kindOf (omit (dict "a" 1) "b") }}/{{ typeIs "map[string]interface {}" (omit (dict "a" 1) "b") }}'
probe typeis-omit-values 'm: {a: 1}' 'v: {{ typeOf (omit .Values.m "b") }}/{{ typeIs "string" (omit .Values.m "b") }}'

# Q3: piped required followed by default.
T3='v: {{ .Values.name | required "name is required" | default "fallback" }}'
probe req-default-string 'name: app' "$T3"
probe req-default-int 'name: 5' "$T3"
probe req-default-false 'name: false' "$T3"
probe req-default-zero 'name: 0' "$T3"
probe req-default-empty-map 'name: {}' "$T3"
probe req-default-empty-list 'name: []' "$T3"
probe req-default-empty-string 'name: ""' "$T3"
probe req-default-null 'name: null' "$T3"
probe req-default-absent '{}' "$T3"
probe req-default-map 'name: {a: 1}' "$T3"

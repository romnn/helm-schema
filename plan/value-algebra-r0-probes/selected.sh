#!/usr/bin/env bash
# R0 probe: ownership of the 15 W1 ternary127 cells selected-{0,2,3}-{4..8}.
# For each chart: generate the schema with the installed main CLI (same flags as the witness lane),
# then let pinned Helm validate the coalesced values (ADR 0054) with the closed root (ADR 0042 default)
# and with only the root `additionalProperties: false` removed.
# Usage: plan/value-algebra-r0-probes/selected.sh > plan/value-algebra-r0-probes/selected-results.txt
set -u
HELM="${HELM:-/Users/roman/.local/share/mise/installs/helm/4.2.3/darwin-arm64/helm}"
CLI="${CLI:-$HOME/.cargo/bin/helm-schema}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCRATCH="$ROOT/target/r0-probes/selected"
rm -rf "$SCRATCH"; mkdir -p "$SCRATCH"
echo "cli: $CLI sha256=$(shasum -a 256 "$CLI" | cut -c1-16)"
"$HELM" version
echo

declare -a TEMPLATES=(
  'selected-0|{{ $_ := ternary "a" "b" (ternary 0 false .Values.enabled) }}'
  'selected-2|{{ $_ := ternary "a" "b" (ternary "" false .Values.enabled) }}'
  'selected-3|{{ $_ := ternary "a" "b" (ternary 0 .Values.unknown .Values.enabled) }}'
)
declare -a CASES=('4|{"condition": true}' '5|{"condition": false}' '6|{"condition": 0}' '7|{"condition": "x"}' '8|{"condition": null}')

for entry in "${TEMPLATES[@]}"; do
  name="${entry%%|*}"; template="${entry#*|}"
  for variant in closed open; do
    dir="$SCRATCH/$variant/$name"; mkdir -p "$dir/templates"
    printf 'apiVersion: v2\nname: ternary-ratchet\nversion: 0.1.0\n' > "$dir/Chart.yaml"
    printf 'enabled: false\nunknown: false\n' > "$dir/values.yaml"
    printf '%s\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: probe\n' "$template" > "$dir/templates/probe.yaml"
    "$CLI" "$dir" --offline --no-k8s-schemas --declared-types annotate -o "$dir/values.schema.json" 2> "$dir/gen.stderr"
    if [ "$variant" = open ]; then
      python3 -I -c 'import json,sys; p=sys.argv[1]; s=json.load(open(p)); s.pop("additionalProperties"); json.dump(s,open(p,"w"),indent=2)' "$dir/values.schema.json"
    fi
  done
  echo "## $name: $template"
  echo "closed root additionalProperties: $(python3 -I -c 'import json,sys; s=json.load(open(sys.argv[1])); print(json.dumps(s.get("additionalProperties")), "properties:", sorted(s.get("properties",{})))' "$SCRATCH/closed/$name/values.schema.json")"
  for c in "${CASES[@]}"; do
    n="${c%%|*}"; override="${c#*|}"
    printf '%s\n' "$override" > "$SCRATCH/override-$n.json"
    printf '%s-%s override %s\n' "$name" "$n" "$override"
    for variant in none closed open; do
      dir="$SCRATCH/closed/$name"; extra=()
      [ "$variant" = open ] && dir="$SCRATCH/open/$name"
      [ "$variant" = none ] && extra=(--skip-schema-validation)
      out="$("$HELM" template control "$dir" -f "$SCRATCH/override-$n.json" ${extra[@]+"${extra[@]}"} 2>&1)"; code=$?
      printf '  schema=%-6s exit=%s %s\n' "$variant" "$code" "$(printf '%s' "$out" | grep -m1 -e '^- at' | sed "s#$SCRATCH#\$SCRATCH#g")"
    done
  done
  echo
done

#!/usr/bin/env bash
# Dump the fully COALESCED .Values document Helm actually validates a
# values.schema.json against: root values + every subchart's defaults under
# its key + propagated globals. Helm has no command for this, so render a
# throwaway template in a copy of the chart.
set -uo pipefail
CHART="$1"; OUT="$2"
TMP=$(mktemp -d)
cp -R "$CHART" "$TMP/c" || { echo "copy failed" >&2; exit 1; }
rm -f "$TMP/c/values.schema.json"
mkdir -p "$TMP/c/templates"
printf '{{- .Values | toYaml -}}\n' > "$TMP/c/templates/zzz-values-dump.yaml"
if ! timeout 300 helm template dump "$TMP/c" -s templates/zzz-values-dump.yaml > "$TMP/raw" 2>"$TMP/err"; then
  head -c 300 "$TMP/err" >&2; rm -rf "$TMP"; exit 1
fi
# strip helm's "---\n# Source: ..." banner
sed -e '1{/^---$/d;}' -e '/^# Source:/d' "$TMP/raw" > "$TMP/vals.yaml"
python3 -c "
import yaml,json,sys
v=yaml.safe_load(open('$TMP/vals.yaml'))
if v is None: v={}
json.dump(v, open('$OUT','w'))
" || { rm -rf "$TMP"; exit 1; }
rm -rf "$TMP"

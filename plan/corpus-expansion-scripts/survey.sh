#!/usr/bin/env bash
# Survey one chart: fetch, render with helm, generate a schema, self-validate.
# Emits exactly one JSON line on stdout. Never fails the caller.
set -uo pipefail

SUR=${CORPUS_SURVEY_DIR:?set CORPUS_SURVEY_DIR to the survey working directory}
PROBER=$SUR/prober/target/release/corpus-prober
BIN=$(command -v helm-schema)

NAME="$1"; VERSION="$2"; REPO="$3"; STARS="$4"
SLUG=$(printf '%s' "$NAME" | tr '/' '_')
WORK="$SUR/charts/$SLUG"
RES="$SUR/results/$SLUG"
mkdir -p "$RES"

esc() { python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'; }

emit() {
  printf '{"name":%s,"version":%s,"repo":%s,"stars":%s,"stage":"%s"%s}\n' \
    "$(printf '%s' "$NAME" | esc)" "$(printf '%s' "$VERSION" | esc)" \
    "$(printf '%s' "$REPO" | esc)" "$STARS" "$1" "${2:-}"
}

# ---- 1. fetch -------------------------------------------------------------
if [ ! -d "$WORK" ]; then
  rm -rf "$WORK.tmp"; mkdir -p "$WORK.tmp"
  if ! timeout 240 helm pull --repo "$REPO" "$NAME" --version "$VERSION" \
        --untar --untardir "$WORK.tmp" > "$RES/pull.log" 2>&1; then
    emit fetch_failed ",\"error\":$(head -c 300 "$RES/pull.log" | esc)"
    rm -rf "$WORK.tmp"; exit 0
  fi
  inner=$(find "$WORK.tmp" -maxdepth 2 -name Chart.yaml -print -quit)
  if [ -z "$inner" ]; then emit no_chart_yaml; rm -rf "$WORK.tmp"; exit 0; fi
  mv "$(dirname "$inner")" "$WORK"; rm -rf "$WORK.tmp"
fi

# ---- 2. helm template on chart defaults -----------------------------------
if timeout 300 helm template survey-release "$WORK" > "$RES/rendered.yaml" 2> "$RES/helm.err"; then
  HELM_OK=true; HELM_ERR=""
else
  HELM_OK=false; HELM_ERR=$(head -c 300 "$RES/helm.err" | tr '\n' ' ')
fi

# ---- 3. generate the schema ----------------------------------------------
START=$(python3 -c 'import time;print(time.time())')
timeout 900 "$BIN" "$WORK" --exclude-tests \
  --k8s-version v1.29.0-standalone-strict \
  --k8s-schema-cache-dir "$SUR/cache/k8s" \
  --crd-catalog-cache-dir "$SUR/cache/crds" \
  -o "$RES/schema.json" > "$RES/gen.out" 2> "$RES/gen.err"
GEN_EXIT=$?
END=$(python3 -c 'import time;print(time.time())')
SECS=$(python3 -c "print(round($END-$START,2))")

COMMON=",\"helm_ok\":$HELM_OK,\"helm_err\":$(printf '%s' "$HELM_ERR" | esc),\"gen_exit\":$GEN_EXIT,\"gen_secs\":$SECS"

if [ "$GEN_EXIT" -eq 124 ]; then emit generate_timeout "$COMMON"; exit 0; fi
if [ "$GEN_EXIT" -ne 0 ]; then
  emit generate_failed "$COMMON,\"error\":$(head -c 600 "$RES/gen.err" | esc)"; exit 0
fi
SIZE=$(wc -c < "$RES/schema.json" | tr -d ' ')

# ---- 4. self-validation ---------------------------------------------------
PROBE=$(timeout 300 "$PROBER" "$WORK/values.yaml" "$RES/schema.json" 2>/dev/null)
[ -z "$PROBE" ] && PROBE='{"status":"prober_crash"}'
printf '%s\n' "$PROBE" > "$RES/probe.json"
STATUS=$(printf '%s' "$PROBE" | python3 -c 'import json,sys; print(json.load(sys.stdin)["status"])' 2>/dev/null || echo prober_crash)
emit ok "$COMMON,\"schema_bytes\":$SIZE,\"self_validation\":\"$STATUS\""

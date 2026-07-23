#!/usr/bin/env bash
# Regenerate the documentation schema snippets under docs/assets/schemas/.
#
# Each snippet is the real output of `helm-schema` run against a small, committed
# example chart under docs/examples/. The docs render the example's templates and
# values verbatim (via a Hugo module mount) alongside the generated schema, so a
# broken example or a changed inference result shows up as a docs diff and gets
# noticed — the docs cannot drift from what the tool actually produces.
#
# The snippets are committed, so the Hugo site builds without the binary; run
# this only to regenerate them. The `schema` and `io` shortcodes inline each one.
#
# Readability: the docs show the FULLY INLINED schema (--no-minimize --inline-refs)
# so every constraint is visible in place, with no `$ref` indirection to opaque
# interned definitions to chase. A fully-inlined schema can leave an unreferenced
# `$defs` block behind (a building block that is now inlined everywhere it is
# used); strip_orphan_defs drops it so the snippet is self-contained. This is a
# cosmetic, meaning-preserving cleanup of doc snippets only — the tool's real
# default output is minimized (see docs/.../reference/output.md).
#
# K8s-backed examples pin an explicit --k8s-version so their upstream types and
# descriptions are reproducible (released schema bundles don't change); the pinned
# version's schemas are fetched on a cold cache and then served locally.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin="$repo/target/release/helm-schema"
examples="$repo/docs/examples"
out="$repo/docs/assets/schemas"

# Pin the K8s schema version so the generated descriptions/types are stable.
K8S_VERSION="v1.35.0"

[[ -x "$bin" ]] || cargo build --release --package helm-schema-cli --manifest-path "$repo/Cargo.toml"
mkdir -p "$out"

# Drop any `$defs` entry no `$ref` points at, and remove `$defs` if it empties.
strip_orphan_defs() {
  python3 - "$1" <<'PY'
import json, sys
p = sys.argv[1]
doc = json.load(open(p))
refs = set()
def walk(n):
    if isinstance(n, dict):
        r = n.get("$ref")
        if isinstance(r, str) and r.startswith("#/$defs/"):
            refs.add(r.rsplit("/", 1)[1])
        for v in n.values():
            walk(v)
    elif isinstance(n, list):
        for v in n:
            walk(v)
walk(doc)
defs = doc.get("$defs")
if isinstance(defs, dict):
    for k in [k for k in defs if k not in refs]:
        del defs[k]
    if not defs:
        del doc["$defs"]
json.dump(doc, open(p, "w"), indent=2, sort_keys=True)
open(p, "a").write("\n")
PY
}

# Generate one schema, fully inlined. Usage: gen <name> <example-dir> [args...]
gen() {
  local name="$1" dir="$examples/$2"; shift 2
  "$bin" "$dir" "$@" --no-minimize --inline-refs --output "$out/$name.json"
  strip_orphan_defs "$out/$name.json"
  echo "wrote $out/$name.json"
}

# K8s-backed examples — types and field descriptions come from the upstream
# Kubernetes JSON schema for the resource each value lands in.
gen hero      hero      --k8s-version "$K8S_VERSION"
gen quickstart quickstart --k8s-version "$K8S_VERSION"

# Template-only examples — no Kubernetes schemas, so the shape reflects the
# template's control flow and default-literal inference alone (fully offline,
# deterministic).
gen guards    guards    --no-k8s-schemas
gen defaults  defaults  --no-k8s-schemas

#!/usr/bin/env python3
"""Find leaf constraints that the chart's own declared default violates.

Walks each pinned corpus schema tracking the values path through `properties`
chains (allOf/anyOf/oneOf/then transparent; if/not skipped). At every node
carrying a value-constraining facet, validates the declared default at that
path against the node in isolation. anyOf/oneOf members are exempt (a
sibling member may accept the default); only conjunctive positions count.
"""
import json
import os
import yaml
import jsonschema

ROOT = "/home/roman/dev/helm-schema"
SCHEMAS = os.path.join(ROOT, "testdata/chart-corpus-schemas")
FACETS = (
    "pattern", "enum", "const", "minimum", "maximum", "exclusiveMinimum",
    "exclusiveMaximum", "minItems", "maxItems", "minLength", "maxLength",
    "multipleOf",
)

MISSING = object()


def default_at(vals, path):
    node = vals
    for seg in path:
        if not isinstance(node, dict) or seg not in node:
            return MISSING
        node = node[seg]
    return node


def walk(node, path, ptr, vals, out, disjunctive):
    if isinstance(node, list):
        for i, child in enumerate(node):
            walk(child, path, f"{ptr}/{i}", vals, out, disjunctive)
        return
    if not isinstance(node, dict):
        return
    if not disjunctive and any(f in node for f in FACETS):
        default = default_at(vals, path)
        if default is not MISSING and default is not None:
            leaf = {f: node[f] for f in FACETS if f in node}
            if "type" in node:
                leaf["type"] = node["type"]
            validator = jsonschema.Draft7Validator(leaf)
            errors = [e.message for e in validator.iter_errors(default)]
            if errors:
                out.append((ptr, ".".join(path), errors[0][:110]))
    for key, child in node.items():
        if key == "properties" and isinstance(child, dict):
            for pk, pv in child.items():
                walk(pv, path + [pk], f"{ptr}/properties/{pk}", vals, out, False)
        elif key == "allOf" and isinstance(child, list):
            for i, arm in enumerate(child):
                walk(arm, path, f"{ptr}/allOf/{i}", vals, out, False)
        elif key in ("anyOf", "oneOf") and isinstance(child, list):
            for i, arm in enumerate(child):
                walk(arm, path, f"{ptr}/{key}/{i}", vals, out, True)
        elif key in ("then", "else") and isinstance(child, dict):
            walk(child, path, f"{ptr}/{key}", vals, out, disjunctive)


for chart in sorted(os.listdir(SCHEMAS)):
    if not chart.endswith(".schema.json"):
        continue
    name = chart[: -len(".schema.json")]
    values_path = os.path.join(ROOT, "testdata/charts", name, "values.yaml")
    if not os.path.exists(values_path):
        continue
    try:
        vals = yaml.safe_load(open(values_path)) or {}
    except yaml.YAMLError:
        continue
    schema = json.load(open(os.path.join(SCHEMAS, chart)))
    out = []
    walk(schema, [], "", vals, out, False)
    for ptr, vpath, err in out:
        print(f"{name}: {vpath} at {ptr}: {err}")

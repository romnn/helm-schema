#!/usr/bin/env python3
"""Find closed schema objects that reject the chart's own declared defaults.

Walks each pinned corpus schema tracking the values path through `properties`
chains (treating allOf/anyOf/oneOf/then as transparent, skipping `if`/`not`
guard subtrees). At every node with `additionalProperties: false`, compares
the node's property set against the chart's declared default keys at that
values path. Reports closed nodes missing declared keys — schema branches
that would reject the chart's own defaults whenever their guard matches.
"""
import json
import os
import yaml

ROOT = "/home/roman/dev/helm-schema"
SCHEMAS = os.path.join(ROOT, "testdata/chart-corpus-schemas")


def default_at(vals, path):
    node = vals
    for seg in path:
        if not isinstance(node, dict) or seg not in node:
            return None
        node = node[seg]
    return node


def walk(node, path, ptr, vals, out):
    if isinstance(node, list):
        for i, child in enumerate(node):
            walk(child, path, f"{ptr}/{i}", vals, out)
        return
    if not isinstance(node, dict):
        return
    props = node.get("properties")
    if node.get("additionalProperties") is False and isinstance(props, dict):
        default = default_at(vals, path)
        if isinstance(default, dict):
            missing = sorted(set(default) - set(props))
            if missing:
                out.append((ptr, ".".join(path) or "<root>", missing))
    for key, child in node.items():
        if key == "properties" and isinstance(child, dict):
            for pk, pv in child.items():
                walk(pv, path + [pk], f"{ptr}/properties/{pk}", vals, out)
        elif key in ("allOf", "anyOf", "oneOf") and isinstance(child, list):
            for i, arm in enumerate(child):
                walk(arm, path, f"{ptr}/{key}/{i}", vals, out)
        elif key in ("then", "else") and isinstance(child, dict):
            walk(child, path, f"{ptr}/{key}", vals, out)
        # `if`, `not`, `items`, `additionalProperties` subtrees: guards or
        # non-values positions; skip for path-tracked closure analysis.


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
    walk(schema, [], "", vals, out)
    for ptr, vpath, missing in out:
        show = missing if len(missing) <= 6 else missing[:6] + [f"...+{len(missing)-6}"]
        print(f"{name}: {vpath} closed at {ptr or '<root>'} missing declared keys {show}")

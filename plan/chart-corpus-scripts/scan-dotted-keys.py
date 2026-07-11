#!/usr/bin/env python3
"""Scan corpus charts for values keys containing '.' and check schema handling.

For each dotted key found in a chart's values.yaml (any depth), check whether
the pinned schema has the literal key at that location (correct) or a
fabricated split path / nothing (the grafana.ini bug class).
"""
import json
import os
import yaml

ROOT = "/home/roman/dev/helm-schema"
SCHEMAS = os.path.join(ROOT, "testdata/chart-corpus-schemas")

def find_dotted(node, path, out):
    if isinstance(node, dict):
        for k, v in node.items():
            if isinstance(k, str) and "." in k:
                out.append((path, k))
            find_dotted(v, path + [str(k)], out)


def schema_node_at(schema, path):
    node = schema
    for seg in path:
        props = node.get("properties") if isinstance(node, dict) else None
        if not isinstance(props, dict) or seg not in props:
            return None
        node = props[seg]
    return node


for chart in sorted(os.listdir(SCHEMAS)):
    if not chart.endswith(".schema.json"):
        continue
    name = chart[: -len(".schema.json")]
    values_path = os.path.join(ROOT, "testdata/charts", name, "values.yaml")
    if not os.path.exists(values_path):
        continue
    try:
        vals = yaml.safe_load(open(values_path)) or {}
    except yaml.YAMLError as e:
        print(f"{name}: values.yaml parse error: {e}")
        continue
    dotted = []
    find_dotted(vals, [], dotted)
    if not dotted:
        continue
    schema = json.load(open(os.path.join(SCHEMAS, chart)))
    for parent, key in dotted:
        pnode = schema_node_at(schema, parent)
        status = []
        if pnode is None:
            status.append("parent-missing")
        else:
            props = pnode.get("properties") or {}
            if key in props:
                status.append("literal-ok")
            else:
                status.append("literal-MISSING")
                head = key.split(".")[0]
                if head in props:
                    status.append(f"split-head '{head}' present (fabricated?)")
                closed = pnode.get("additionalProperties") is False
                status.append("parent-closed" if closed else "parent-open")
        print(f"{name}: {'.'.join(parent) or '<root>'} :: '{key}' -> {', '.join(status)}")

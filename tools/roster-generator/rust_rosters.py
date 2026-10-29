"""Read and edit roster initializers through ast-grep's Rust parser."""

import json
import subprocess
from pathlib import Path


KINDS = {
    "KNOWN_FALSE_ACCEPTANCES": "KnownFalseAcceptances",
    "KNOWN_CONTEXT_FALSE_ACCEPTANCES": "ContextFalseAcceptance",
    "KNOWN_UNDECIDED_ACCEPTANCES": "KnownUndecidedAcceptances",
    "KNOWN_CONTEXT_UNDECIDED_ACCEPTANCES": "ContextUndecidedAcceptance",
    "KNOWN_INHERITED_FAILURES": "KnownInheritedFailure",
    "KNOWN_EXACT_FALSE_ACCEPTANCES": "ExactFalseAcceptance",
    "KNOWN_EXACT_UNDECIDED_ACCEPTANCES": "ExactUndecidedAcceptance",
}
FILES = (
    "known_false_acceptances.rs",
    "known_undecided_acceptances.rs",
    "known_inherited_failures.rs",
)
COMMON = Path("crates/helm-schema/tests/common")


def ast(path, pattern=None, kind=None):
    command = ["ast-grep", "run", "-l", "rust", "--json=compact"]
    command += ["-p", pattern] if pattern else ["--kind", kind]
    result = subprocess.run(command + [str(path)], capture_output=True, text=True)
    if result.returncode not in (0, 1):
        raise ValueError(result.stderr)
    return json.loads(result.stdout)


def span(node):
    return node["range"]["byteOffset"]["start"], node["range"]["byteOffset"]["end"]


def inside(node, parent):
    start, end = span(node)
    outer_start, outer_end = span(parent)
    return outer_start <= start and end <= outer_end


def string(literal):
    if literal.startswith("r"):
        return literal[literal.index('"') + 1:literal.rindex('"')]
    if not literal.startswith('"') or not literal.endswith('"'):
        raise ValueError(f"not a Rust string: {literal}")
    value, index, result = literal[1:-1], 0, []
    escapes = {"n": "\n", "r": "\r", "t": "\t", "0": "\0", "\\": "\\", '"': '"', "'": "'"}
    while index < len(value):
        char = value[index]
        index += 1
        if char != "\\":
            result.append(char)
            continue
        char = value[index]
        index += 1
        if char in "\r\n":
            while index < len(value) and value[index].isspace():
                index += 1
        elif char == "x":
            result.append(chr(int(value[index:index + 2], 16)))
            index += 2
        elif char == "u":
            if value[index] != "{":
                raise ValueError("invalid Rust Unicode escape")
            end = value.index("}", index)
            result.append(chr(int(value[index + 1:end].replace("_", ""), 16)))
            index = end + 1
        else:
            result.append(escapes[char])
    return "".join(result)


def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))


def context_case(chart, document, overlay, version):
    return f"{chart}: document={document} transport=values-file kube={version} overlay={canonical(overlay)}"


def identity(row):
    keys = ["kind", "chart", "case"]
    if row["kind"] in ("KnownFalseAcceptances", "ContextFalseAcceptance", "ExactFalseAcceptance"):
        keys += ["rejection", "baseline", "family"]
    elif row["kind"] in ("KnownUndecidedAcceptances", "ContextUndecidedAcceptance", "ExactUndecidedAcceptance"):
        keys += ["uncertain"]
    else:
        keys += ["failure_class", "reason", "input", "lint_diagnostics_sha256", "owner", "observed_on", "kubernetes_version", "rejection", "baseline", "kubernetes_errors"]
    return canonical({key: row.get(key) for key in keys})


class Source:
    def __init__(self, path):
        self.path = path
        self.source = path.read_bytes()
        self.nodes = ast(path, "$TYPE { $$$FIELDS }")
        self.strings = sorted(ast(path, kind="string_literal") + ast(path, kind="raw_string_literal"), key=span)
        self.constants = {
            node["metaVariables"]["single"]["NAME"]["text"]: node["metaVariables"]["single"]["VALUE"]
            for node in ast(path, "pub(crate) const $NAME: $TYPE = $VALUE;")
        }

    def fields(self, node):
        return {
            field["text"].split(":", 1)[0].strip(): field
            for field in node["metaVariables"]["multi"]["FIELDS"]
            if ":" in field["text"] and field["text"].split(":", 1)[0].strip().isidentifier()
        }

    def literals(self, node):
        return [string(item["text"]) for item in self.strings if inside(item, node)]

    def field_string(self, node, key):
        values = self.literals(self.fields(node)[key])
        if len(values) != 1:
            raise ValueError(f"expected one {key} string, got {values}")
        return values[0]

    def label(self, node, key):
        return self.fields(node)[key]["text"].split(":", 1)[1].strip().rsplit("::", 1)[-1]

    def sub(self, node, typename):
        return [item for item in self.nodes if item is not node and inside(item, node)
                and item["metaVariables"]["single"]["TYPE"]["text"] == typename]

    def rows(self, node, kind):
        row = {"kind": kind, "chart": self.field_string(node, "chart")}
        fields = self.fields(node)
        for key in ("rejection", "baseline", "family"):
            if key in fields:
                row[key] = self.label(node, key)
        if "uncertain" in fields:
            row["uncertain"] = self.literals(fields["uncertain"])
        if kind in ("KnownFalseAcceptances", "KnownUndecidedAcceptances"):
            rows = []
            for probe in self.sub(node, "Probe"):
                item = dict(row, path=self.field_string(probe, "path"), value=self.field_string(probe, "value"))
                item["case"] = f"{item['chart']}: {item['path']} <- {item['value']}"
                rows.append((item, probe))
            return rows
        if kind in ("ExactFalseAcceptance", "ExactUndecidedAcceptance"):
            row["probe"] = self.field_string(node, "probe")
            row["case"] = f"{row['chart']}: {row['probe']}"
            return [(row, node)]
        selectors = self.sub(node, "ContextSelector")
        if selectors:
            selector = selectors[0]
            row.update(document=self.label(selector, "document"),
                       overlay=json.loads(self.field_string(selector, "overlay")),
                       kubernetes_version=self.field_string(selector, "kubernetes_version"))
            row["case"] = context_case(row["chart"], row["document"], row["overlay"], row["kubernetes_version"])
        else:
            selector = self.sub(node, "FailureSelector::ExactProbe")
            if selector:
                selector = selector[0]
                name = self.field_string(selector, "name")
            else:
                selector = self.sub(node, "FailureSelector::Probe")[0]
                probe = self.sub(selector, "Probe")[0]
                name = f"{self.field_string(probe, 'path')} <- {self.field_string(probe, 'value')}"
            row["kubernetes_version"] = self.field_string(selector, "kubernetes_version")
            row["case"] = f"{row['chart']}: {name}"
        if kind == "KnownInheritedFailure":
            for key in ("owner", "observed_on"):
                row[key] = self.field_string(node, key)
            failure = self.sub(node, "FailureClass::Adjudication")
            if failure:
                row.update(failure_class="Adjudication", reason=self.field_string(failure[0], "reason"))
            else:
                failure = self.sub(node, "FailureClass::OracleFalseAcceptance")[0]
                row.update(failure_class="OracleFalseAcceptance", rejection=self.label(failure, "rejection"),
                           baseline=self.label(failure, "baseline"),
                           kubernetes_errors=self.literals(self.fields(failure)["kubernetes_errors"]))
            inputs = self.sub(node, "FailureInput")[0]
            row["input"] = {key: self.field_string(inputs, key) for key in ("values_file_sha256", "controls_sha256")}
            row["lint_diagnostics_sha256"] = self.literals(fields["lint_diagnostics_sha256"]) or None
        return [(row, node)]

    def entries(self, constant):
        value = self.constants[constant]
        previous, _ = span(value)
        previous += 2
        entries = []
        for node in sorted(self.sub(value, KINDS[constant]), key=span):
            start, end = span(node)
            prefix = self.source[previous:start].decode().lstrip(", \t\r\n")
            entries.append((prefix, node, self.rows(node, KINDS[constant])))
            previous = end
        return entries

    def rewrite(self, stale, additions):
        changes = []
        removed = []
        for constant in self.constants.keys() & KINDS.keys():
            entries = []
            for prefix, node, rows in self.entries(constant):
                start, end = span(node)
                text = self.source[start:end]
                deletes = []
                kept = []
                for row, child in rows:
                    if identity(row) in stale:
                        removed.append(row)
                        child_start, child_end = span(child)
                        child_end += int(self.source[child_end:child_end + 1] == b",")
                        deletes.append((child_start - start, child_end - start))
                    else:
                        kept.append(row)
                if not kept:
                    continue
                for child_start, child_end in sorted(deletes, reverse=True):
                    text = text[:child_start] + text[child_end:]
                key = (kept[0]["chart"], identity(kept[0]))
                entries.append((key, prefix + text.decode()))
            entries.extend(additions.get(constant, []))
            rendered = "&[\n" + "".join(text + ",\n" for _, text in sorted(entries)) + "]"
            changes.append((*span(self.constants[constant]), rendered.encode()))
        output = self.source
        for start, end, replacement in sorted(changes, reverse=True):
            output = output[:start] + replacement + output[end:]
        return output, removed

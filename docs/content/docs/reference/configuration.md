---
title: Configuration
weight: 2
---

# Configuration

`helm-schema.yaml` lets a root chart carry its schema-emission policy with the
chart. The format is versioned and strict: malformed YAML, unknown fields,
unsupported versions, and invalid policy combinations are errors.

```yaml
version: 1
profile: lean
emission:
  root-anchored-conditionals: off
  local-conditionals: on
  terminal-clauses: off
  kind-partitions: off
```

All fields except `version` are optional. `profile` is `full` or `lean`; each
emission value is `on` or `off`.

## Discovery and trust boundary

Automatic discovery reads only `helm-schema.yaml` at the root of the input
chart. Dependency and subchart config files are never policy inputs. Directory
and packaged `.tgz`/`.tar.gz` charts use the same root-source boundary.

`--config PATH` selects an explicit file instead. Relative paths are resolved
from the invocation working directory. `--no-config` disables configuration,
including automatic discovery. `--print-effective-config` prints every policy
field and its source, then exits before template analysis or provider/network
setup.

## Precedence

| priority | source | notes |
|---:|---|---|
| 1 | CLI emission override | `--root-anchored-conditionals`, `--local-conditionals`, `--terminal-clauses`, or `--kind-partitions` |
| 2 | config `emission` field | Applied only when the CLI did not select a profile explicitly. |
| 3 | selected profile preset | From config `profile`, CLI `--profile`, or the built-in default. |
| 4 | built-in default | `full`. |

An explicit CLI `--profile` resets all file-level emission deltas. This makes
`--profile lean` mean standard lean even when the chart config customizes lean.
CLI emission overrides still apply after that reset.

## Retention contract

| profile | root ordinary | local ordinary | terminals | kind partitions |
|---|---:|---:|---:|---:|
| `full` | on | on | on | on |
| `lean` | off | on | off | off |

Every policy retains Mandatory facts. These include unconditional base and
provider constraints, presence/not-null facts, default preservation, scalar
spelling behavior, host preparation, and completion/canonicalization behavior.
The four configurable controls are W-class: switching one off only removes
refinements and can only widen schema acceptance.

Kind partitions also require their anchor lane. Enabling kind partitions while
both root and local conditional lanes are off is invalid and fails before chart
analysis.

## Diagnostics and output identity

When an automatically discovered file weakens the invocation relative to the
same command without that file, one diagnostic lists all disabled controls.
CLI-only weakening is not attributed to chart configuration.

Final schemas include `x-helm-schema-policy`, recording the requested profile,
fully resolved controls, narrowing/output modifiers, and a deterministic policy
fingerprint. Explicit library policies record a null requested profile.

# goyamlsweep

This is the YAML differential oracle, separate from helmsweep's adjudication binary.
It calls go-yaml v2.4.3, sigs.k8s.io/yaml v1.6.0, Sprig v3.3.0 and Helm v4.2.3 directly.
Its Go toolchain and complete module requirement pins match helmsweep.

`task build:goyamlsweep` stages a digest-verified go-yaml copy with an additive private-reader
observation adapter, builds `<target>/goyamlsweep`, and verifies linked versions.
Original module files remain unchanged.
Helm is linked from its pristine release module; the oracle does not modify helmsweep sources,
its module graph, or its output binary.
The linked Helm release identity uses the same release linker flags as helmsweep.

`task test:goyamlsweep` runs the staged Go tests.
`task lint:goyamlsweep` checks the staging program and staged oracle; `task lint` includes it.
The integration test task builds this oracle; default Rust unit tests do not require it.

The executable accepts one JSON request and returns one JSON response per line on stdin/stdout.
No arguments are needed; `yaml` is accepted as an alias, and `version` prints verified pins.

- `read`: `data` is a base64 byte string; observes v2, sigs generic/map/array/header targets and parser events.
- `read_batch`: `data` is an array of base64 strings; returns the corresponding read replies.
- `v2_batch`: omits the extra map/array/header targets.
- `tree_batch`: also omits event payloads while retaining tree, error and first-document end.
- `stages`: `text` and nonnegative `width` observe SplitManifests, indent, nindent, trim, quote and strip.
- `stages_batch` and `split_batch`: `data` is an array of strings.
- `unicode`: `from`/`to` are a half-open range of at most 4,096 code points; surrogate points are omitted.
  Each reply item contains `codepoint`, Sprig `quote`, and Go `unicode.IsSpace` as `space`.
- `render`: `chart` is an absolute local chart path and `kube_version` defaults to `1.29.0`.
  It returns `{stdout,error}`, using the pinned client-only Helm action, release `t`, namespace `default`,
  chart defaults and schema validation disabled, without reading CLI configuration or invoking system programs.

The canonical v2 tree retains Go's concrete scalar types and invalid string bytes.
The additive adapter observes actual decoder error ordering separately from a raw event walk.

# Helm's pinned YAML reader

This crate ports go-yaml v2.4.3's byte reader, scanner, parser, generic decoder and resolver.
There is no analyser or generator consumer.
Ported functions cite their pinned Go source locations.
Upstream notices and licenses are included beside this file.

`Reader::step(char, sink)` steps the scanner immediately.
`push_bytes(bytes, sink)` shares that scanner and retains only an incomplete encoding prefix.
Both use a content-free validator for Go's 512-byte raw window.
Events are provisional until the window or EOF commits completion: a later forbidden character
in the window overrides a pending Done, scanner/parser error or event-consumer error.
`read_document` constructs aliases as events arrive, then resolves and decodes the tree only
when the complete document succeeds.
The physical decoded-cache origin and EOF refill registers preserve Go's BOM behavior.

`Reader` and `ReaderControl` implement `Clone + Eq + Hash`.
The merge key omits consumed literal payloads and presentation positions while retaining
semantic tags/directives, simple-key registers, held-token roles and validator phase/outcome.
The byte decoder's incomplete encoding prefix remains input, not a raw text window.

`Value::RawString` retains invalid UTF-8 binary strings until JSON encoding.
Valid binary strings use `String`, so Go string-key identity is preserved.
Distinct typed YAML keys can coerce to the same JSON key; Go ranges an unordered map before
assignment, so its winner is unspecified.
The concrete converter uses entry order, and differential laws derive the permitted collision
outcomes from the original typed Go map.

`stages` contains independent streaming UTF-8 transducers for Helm's splitter, Sprig's
indent/nindent/trim/quote and literal `<no value>` removal.
Reader completion or failure never resets or stops a stage.

## Oracle and proofs

`task build:goyamlsweep` builds the separate pinned JSONL oracle in the same `CARGO_TARGET_DIR`.
It verifies Go 1.26.5, go-yaml v2.4.3, sigs YAML v1.6.0, Sprig v3.3.0 and Helm v4.2.3.
Its observation adapter is added to a digest-verified private YAML source copy.
The adjudication driver `tools/helmsweep` is unchanged.

All oracle tests live in integration binaries; the default unit profile needs no Go or oracle.
Run `cargo nextest run -P integration -p helm-schema-goyaml --test-threads 4` through the round's
memory-cap wrapper with `CARGO_BUILD_JOBS=4`.

P1 checks all 17,895,697 strings through length 6 over the 16 structural symbols, including
all length-5 inputs, and all 2,625,641 strings through length 4 over a 40-symbol wide alphabet.
The wide alphabet includes all decimal digits, letters, tab, CR, anchors, tags, directives,
quotes and escape punctuation.
Oracle response hashes use 65,536-input chunks for compact drift localization.
Set `GOYAML_REGENERATE_HASHES=1` only for a deliberate re-pin, then rerun without it to verify.

P1 reads `SCHEMA_HELM_INVOCATION_CACHE` when set; otherwise it renders vendored defaults through
the pinned Go oracle, without a system Helm executable or timeout program.
It compares Go-split manifests and deterministic byte replacements, deletions and insertions.
P2 compares immediate character steps with byte partitions and checks equal-control futures
for every prefix/suffix through length 3 over 18 symbols, plus real corpus prefix pairs.
P5 feeds the actual stage structs character by character, exhausts splitter/stage alphabets,
and checks quote and Unicode whitespace on every Unicode scalar value against Go.

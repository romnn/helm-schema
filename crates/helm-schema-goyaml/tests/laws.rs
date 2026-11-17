//! Differential laws use the pinned Go code for every expected result.

mod common;
mod oracle;

use indoc::{formatdoc, indoc};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_goyaml::{Reader, stages};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use test_util::prelude::sim_assert_eq;

use common::{STRUCTURAL, WIDE, compare, encoded, words};
use oracle::Oracle;

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|byte| {
            [
                char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'),
                char::from_digit(u32::from(byte & 15), 16).unwrap_or('0'),
            ]
        })
        .collect()
}

#[derive(Clone, Copy)]
enum ReadMode {
    AllTargets,
    Events,
    Trees,
}

fn read_batch(
    oracle: &mut Oracle,
    batch: &mut Vec<Vec<u8>>,
    mode: ReadMode,
    digest: &mut Sha256,
) -> eyre::Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let response = oracle.request(&json!({"op":match mode { ReadMode::AllTargets => "read_batch", ReadMode::Events => "v2_batch", ReadMode::Trees => "tree_batch" },"data":batch.iter().map(|input| encoded(input)).collect::<Vec<_>>()}))?;
    let replies = response
        .as_array()
        .ok_or_eyre("oracle batch is not an array")?;
    sim_assert_eq!(have: replies.len(), want: batch.len());
    for (input, reply) in batch.iter().zip(replies) {
        compare(input, reply, matches!(mode, ReadMode::AllTargets))?;
        digest.update(serde_json::to_vec(reply)?);
        digest.update(b"\n");
    }
    batch.clear();
    Ok(())
}

/// P1 enumerates the complete bounded structural language and records oracle chunk hashes.
#[test]
fn p1_exhaustive() -> eyre::Result<()> {
    let mut oracle = Oracle::start()?;
    let max = 6;
    let mut batch = Vec::new();
    let mut digest = Sha256::new();
    let mut chunks = Vec::new();
    let mut count = 0_u64;
    words(STRUCTURAL, max, |input| {
        batch.push(input.to_vec());
        count += 1;
        if batch.len() == 512 {
            read_batch(&mut oracle, &mut batch, ReadMode::Events, &mut digest)?;
            if count.is_multiple_of(65_536) {
                chunks.push(format!("{count}\t{}", hex(&digest.finalize_reset())));
            }
        }
        Ok(())
    })?;
    read_batch(&mut oracle, &mut batch, ReadMode::Events, &mut digest)?;
    if !count.is_multiple_of(65_536) {
        chunks.push(format!("{count}\t{}", hex(&digest.finalize())));
    }
    let hash_text = format!("{}\n", chunks.join("\n"));

    let expected_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/p1-chunks-length{max}.sha256"));
    if std::env::var_os("GOYAML_REGENERATE_HASHES").is_some() {
        fs::write(&expected_path, &hash_text)?;
    }
    let expected = fs::read_to_string(expected_path)?;
    let expected: Vec<_> = expected.lines().collect();
    sim_assert_eq!(have: chunks.iter().map(String::as_str).collect::<Vec<_>>(), want: expected);
    eprintln!(
        "P1 structural alphabet={} max={max} inputs={count} chunks={}",
        STRUCTURAL.len(),
        chunks.len()
    );
    oracle.finish()
}

/// The wide P1 language covers letters, all decimal digits, tabs, CR and tag/anchor syntax.
#[test]
fn p1_wide_exhaustive() -> eyre::Result<()> {
    let mut oracle = Oracle::start()?;
    let mut batch = Vec::new();
    let mut digest = Sha256::new();
    let mut chunks = Vec::new();
    let mut count = 0_u64;
    words(WIDE, 4, |input| {
        batch.push(input.to_vec());
        count += 1;
        if batch.len() == 512 {
            read_batch(&mut oracle, &mut batch, ReadMode::Events, &mut digest)?;
            if count.is_multiple_of(65_536) {
                chunks.push(format!("{count}\t{}", hex(&digest.finalize_reset())));
            }
        }
        Ok(())
    })?;
    read_batch(&mut oracle, &mut batch, ReadMode::Events, &mut digest)?;
    if !count.is_multiple_of(65_536) {
        chunks.push(format!("{count}\t{}", hex(&digest.finalize())));
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/p1-wide-chunks.sha256");
    if std::env::var_os("GOYAML_REGENERATE_HASHES").is_some() {
        fs::write(&path, format!("{}\n", chunks.join("\n")))?;
    }
    let expected = fs::read_to_string(path)?;
    sim_assert_eq!(have: chunks.join("\n"), want: expected.trim_end());
    eprintln!(
        "P1 wide alphabet={} max=4 inputs={count} chunks={}",
        WIDE.len(),
        chunks.len()
    );
    oracle.finish()
}

fn collect_renders(root: &Path, output: &mut Vec<PathBuf>, depth: usize) -> eyre::Result<()> {
    if depth > 8 {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_renders(&path, output, depth + 1)?;
        } else if path.file_name().is_some_and(|name| name == "stdout") {
            output.push(path);
        }
    }
    Ok(())
}

fn corpus() -> eyre::Result<Vec<Vec<u8>>> {
    let Some(cache) = std::env::var_os("SCHEMA_HELM_INVOCATION_CACHE") else {
        return default_renders();
    };
    let mut paths = Vec::new();
    if let Err(err) = collect_renders(Path::new(&cache), &mut paths, 0) {
        eprintln!("adjudication cache unreadable: {err}; rendering vendored defaults");
        return default_renders();
    }
    paths.sort();
    if paths.is_empty() {
        return default_renders();
    }
    let total = paths.len();
    let mut seen = BTreeSet::new();
    let mut renders = Vec::new();
    for path in paths {
        let input = match fs::read(path) {
            Ok(input) => input,
            Err(err) => {
                eprintln!("adjudication cache unreadable: {err}; rendering vendored defaults");
                return default_renders();
            }
        };
        if !input.is_empty() && seen.insert(Sha256::digest(&input).to_vec()) {
            renders.push(input);
        }
    }
    eprintln!(
        "corpus cached renders={total} distinct nonempty={}",
        renders.len()
    );
    Ok(renders)
}

fn default_renders() -> eyre::Result<Vec<Vec<u8>>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/charts");
    let mut charts = fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    charts.sort();
    let mut renders = Vec::new();
    let mut failed = 0;
    let mut oracle = Oracle::start()?;
    for chart in charts {
        if !chart.join("Chart.yaml").is_file() {
            continue;
        }
        let response = oracle.request(
            &json!({"op":"render", "chart":chart.canonicalize()?, "kube_version":"1.29.0"}),
        )?;
        if response
            .get("error")
            .is_some_and(serde_json::Value::is_null)
        {
            renders.push(
                response
                    .get("stdout")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_eyre("render response has no stdout")?
                    .as_bytes()
                    .to_vec(),
            );
        } else {
            failed += 1;
            eprintln!("unrenderable default {}: {}", chart.display(), response);
        }
    }
    oracle.finish()?;
    if renders.is_empty() {
        eyre::bail!("no readable cached manifests or renderable corpus defaults");
    }
    eprintln!(
        "corpus default renders={} unrenderable={failed}",
        renders.len()
    );
    Ok(renders)
}

fn typed_inputs() -> Vec<Vec<u8>> {
    let mut inputs = Vec::new();
    let cases: &[&[u8]] = &[
        b"yes",
        b"on",
        b"off",
        b"~",
        b"0o17",
        b"0b101",
        b"09",
        b"1_000",
        b"1e3",
        b".nan",
        b"+.inf",
        b"18446744073709551615",
        b"kind: 5\nmetadata:\n  name: on\n  annotations:\n    a: false\n",
        b"kind: []",
        b"kind: null",
        b"null",
        b"# empty\n",
        b"a: b\n...\n[open",
        b"&a [*a]",
        b"a: *x\nb: [}",
        b"a: !!bool wrong\nb: [}",
        b"*x\n: bad [",
        b"a: &a {b: 1}\nc: {<<: *a, b: 2}",
        b"%YAML 1.2\n---\na",
        b"%TAG !e! tag:example.com,2020:\n---\n!e!foo x",
        b"!!float 12",
        b"!!binary YQ==",
        b"!!timestamp 2020-1-1",
        b"!!bool wrong",
        b"!%21",
        b"!<!!bool> yes",
        b"!<tag:yaml.org,2002:> null",
        b"!<bool> yes",
        b"!<binary> invalid",
        b"!<!!binary> YQ==",
        b"!<%C0%80> x",
        b"!<%ED%A0%80> x",
        b"!<%E1%80%41> x",
        b"%TAG !e! %F4%90%80%80\n---\n!e!foo x",
        b"{null: value}",
        b"{[a]: b}",
        b"\xff",
        b"\xc0\x80",
        b"\xef\xbb\xbfa: b",
        b"\xff\xfea\0:\0 \0b\0",
        b"a: b\r\nc: d\r\n",
        b"|\n  a\n",
        b">-\n  a\n\n  b\n",
    ];
    for input in cases {
        inputs.push(input.to_vec());
    }
    for scalar in [
        ".1_2",
        ".1__2",
        ".1_",
        "1__0",
        "+18446744073709551615",
        "0x8000000000000000",
        "0x10000000000000000",
        "-0b100",
        "0o8",
        "+.5",
        "1.",
        "1e999",
        "-0",
        "-.0",
        ".NaN",
        "-.Inf",
        "1:20",
        "2001-2-3",
        "2000-2-29",
        "1900-2-29",
        "2001-2-3T4:5:6Z",
        "2001-2-3t4:5:6.123Z",
        "2001-2-3 4:5:6.123",
        "2001-2-3T04:05:06+24:60",
        "2001-02-03T04:05:06.Z",
        "2001-02-03T04:05:06,1Z",
    ] {
        inputs.push(scalar.as_bytes().to_vec());
        inputs.push(format!("!!float {scalar}").into_bytes());
        inputs.push(format!("!!timestamp {scalar}").into_bytes());
        inputs.push(format!("kind: {scalar}").into_bytes());
    }
    inputs.extend(extra_typed_inputs());
    inputs
}

fn extra_typed_inputs() -> Vec<Vec<u8>> {
    let mut inputs = Vec::new();
    for input in [
        "0b+1",
        "0b-1",
        "0b_+101",
        "0b-_101",
        "!!float 0b-101",
        "!!timestamp 2001-12-14  21:59:43",
        "!!binary 4YBB",
        "!!binary Zh==",
        "!!binary Zm9=",
        "{!!binary /w==: a, !!binary /g==: b}",
        "{!!binary /w==: b, '�': a}",
        "metadata: {unknown: .nan}",
        "metadata: {18446744073709551615: a}",
        "\u{212a}ind: 5",
        "apiVerſion: 5",
        "metadata: {annotationſ: {a: 5}}",
        "metadata: {name: a, Name: b}",
        "metadata: {annotations: {a: x}, Annotations: {b: y}}",
        "{metadata: {name: a}, Metadata: {annotations: {b: y}}}",
        "{Kind: a, kind: ''}",
        "{Kind: a, kind: null}",
        "{Metadata: {name: a}, metadata: null}",
        "{Metadata: {name: a}, metadata: {name: null}}",
        "{-.0: a, .0: b}",
        "{.0: a, -.0: b}",
        "kind: 1e100",
        "kind: -1e100",
        "{1e100: a}",
    ] {
        inputs.push(input.as_bytes().to_vec());
    }
    let repeated = ["x"; 197].join(",");
    let aliases = ["*a"; 203].join(",");
    inputs.push(
        formatdoc!(
            "
        a: &a [{repeated}]
        b: [{aliases}]
    "
        )
        .into_bytes(),
    );
    inputs
}

/// P1 checks YAML 1.1 resolution and all of sigs' concrete target conversions.
#[test]
fn p1_typed() -> eyre::Result<()> {
    let mut oracle = Oracle::start()?;
    let mut inputs = typed_inputs();
    let count = inputs.len();
    read_batch(
        &mut oracle,
        &mut inputs,
        ReadMode::AllTargets,
        &mut Sha256::new(),
    )?;
    eprintln!("P1 typed inputs={count}");
    oracle.finish()
}

/// P1 checks Go's allowed outcomes when distinct YAML keys coerce to the same JSON key.
#[test]
fn p1_coercion_collisions() -> eyre::Result<()> {
    let mut oracle = Oracle::start()?;
    let cases = [
        "{1: a, '1': b}",
        "{true: a, 'true': b}",
        "{.nan: a, '.nan': b}",
        "{outer: {1: a, '1': b}}",
        "[{1: a, '1': b}]",
        "{.nan: a, .nan: b}",
    ];
    let mut inputs = (0..32)
        .flat_map(|_| cases.iter().map(|text| text.as_bytes().to_vec()))
        .collect();
    read_batch(
        &mut oracle,
        &mut inputs,
        ReadMode::AllTargets,
        &mut Sha256::new(),
    )?;
    eprintln!("P1 unordered coercion inputs={}", cases.len() * 32);
    oracle.finish()
}

/// P1 exercises every cached manifest and deterministic byte mutations.
#[test]
fn p1_corpus_and_mutations() -> eyre::Result<()> {
    let mut oracle = Oracle::start()?;
    let mut batch = Vec::new();
    let mut digest = Sha256::new();
    let mut count = 0_u64;
    let mut submit = |input: Vec<u8>| -> eyre::Result<()> {
        batch.push(input);
        count += 1;
        if batch.len() == 128 {
            read_batch(&mut oracle, &mut batch, ReadMode::Trees, &mut digest)?;
        }
        Ok(())
    };
    let mut split_oracle = Oracle::start()?;
    let mut seen_chunks = BTreeSet::new();
    let mut manifests = 0_u64;
    for input in corpus()? {
        // Split with Go itself, including separators embedded in block scalars and quotes.
        let response = split_oracle
            .request(&json!({"op":"stages","text":String::from_utf8_lossy(&input),"width":2}))?;
        for chunk in response["split"]
            .as_array()
            .ok_or_eyre("missing Go split chunks")?
        {
            let chunk = chunk.as_str().ok_or_eyre("non-string Go chunk")?.as_bytes();
            manifests += 1;
            let hash = Sha256::digest(chunk);
            if !seen_chunks.insert(hash.to_vec()) {
                continue;
            }
            submit(chunk.to_vec())?;
            // Mutations are keyed by the bytes, so duplicate cached manifests have identical coverage.
            let selector = hash.iter().take(8).fold(0_usize, |acc, byte| {
                acc.wrapping_mul(257).wrapping_add(usize::from(*byte))
            });
            let position = selector % chunk.len().max(1);
            if !chunk.is_empty() {
                let byte = [0_u8, b':', b'\t', b'"', b'[', b'}', b'#', b'\r', 0xff]
                    .get(selector % 9)
                    .copied()
                    .ok_or_eyre("invalid mutation byte")?;
                let mut changed = chunk.to_vec();
                changed[position] = byte;
                submit(changed)?;
                let mut changed = chunk.to_vec();
                changed.remove(position);
                submit(changed)?;
            }
            let mut changed = chunk.to_vec();
            changed.insert(position, b'\n');
            submit(changed)?;
        }
    }
    split_oracle.finish()?;
    read_batch(&mut oracle, &mut batch, ReadMode::Trees, &mut digest)?;
    eprintln!(
        "P1 cached manifests={manifests} distinct={} total with byte mutations={count}",
        seen_chunks.len()
    );
    oracle.finish()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Run {
    reader: Reader,
    events: Vec<helm_schema_goyaml::Event>,
    error: Option<helm_schema_goyaml::ErrorKind>,
    done: bool,
    end: Option<helm_schema_goyaml::Mark>,
}

fn run_bytes(input: &[u8], cuts: &[usize]) -> Run {
    let mut reader = Reader::default();
    let mut events = Vec::new();
    let mut begin = 0;
    let mut sink = |event: &helm_schema_goyaml::Event| {
        events.push(event.clone());
        Ok(())
    };
    let result = (|| {
        for &end in cuts.iter().chain(std::iter::once(&input.len())) {
            let Some(chunk) = input.get(begin..end) else {
                return Err(helm_schema_goyaml::Error {
                    kind: helm_schema_goyaml::ErrorKind::Reader,
                    line: 0,
                    message: "invalid test partition".to_owned(),
                });
            };
            reader.push_bytes(chunk, &mut sink)?;
            begin = end;
        }
        reader.finish(&mut sink)
    })();
    Run {
        reader: reader.clone(),
        events,
        error: result.err().map(|err| err.kind()),
        done: reader.is_done(),
        end: reader.document_end(),
    }
}

fn run_characters(input: &str) -> Run {
    let mut reader = Reader::default();
    let mut events = Vec::new();
    let mut sink = |event: &helm_schema_goyaml::Event| {
        events.push(event.clone());
        Ok(())
    };
    let result = (|| {
        for character in input.chars() {
            reader.step(character, &mut sink)?;
        }
        reader.finish(&mut sink)
    })();
    Run {
        reader: reader.clone(),
        events,
        error: result.err().map(|err| err.kind()),
        done: reader.is_done(),
        end: reader.document_end(),
    }
}

/// P2 compares immediate character stepping with byte partitions at every two-cut boundary.
#[test]
fn p2_chunk_partitions() -> eyre::Result<()> {
    let mut partitions = 0_u64;
    words(STRUCTURAL, 3, |input| {
        let text = std::str::from_utf8(input)?;
        let expected = run_characters(text);
        for first in 0..=input.len() {
            for second in first..=input.len() {
                sim_assert_eq!(have: run_bytes(input, &[first,second]), want: expected.clone());
                partitions += 1;
            }
        }
        Ok(())
    })?;
    for input in [
        b"a: b\r\nc: d\r\n".as_slice(),
        b"---\na: 'quote'\n...\n[open",
        b"a: \"x\\u1234\"\n",
        "a: \u{2028}b".as_bytes(),
        b"\xff\xfea\0:\0 \0b\0",
    ] {
        let expected = run_bytes(input, &[]);
        if let Ok(text) = std::str::from_utf8(input) {
            sim_assert_eq!(have: run_characters(text), want: expected.clone());
        }
        for first in 0..=input.len() {
            for second in first..=input.len() {
                sim_assert_eq!(have: run_bytes(input, &[first,second]), want: expected.clone());
                partitions += 1;
            }
        }
    }
    let mut seed = 0x7b2_7b2_u64;
    for input in corpus()? {
        let expected = run_bytes(&input, &[]);
        if let Ok(text) = std::str::from_utf8(&input) {
            sim_assert_eq!(have: run_characters(text), want: expected.clone());
        }
        let mut cuts = Vec::new();
        for offset in 0..input.len() {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            if seed.is_multiple_of(31) {
                cuts.push(offset);
            }
        }
        sim_assert_eq!(have: run_bytes(&input, &cuts), want: expected);
        partitions += 1;
    }
    // A hole can inspect scanner progress before the first raw validation window ends.
    let mut reader = Reader::default();
    let mut events = Vec::new();
    for character in "a: [x".chars() {
        reader.step(character, &mut |event| {
            events.push(event.clone());
            Ok(())
        })?;
    }
    assert!(
        !events.is_empty(),
        "character stepping must reach the scanner before EOF or512bytes"
    );
    eprintln!("P2 immediate/partition comparisons={partitions}");
    Ok(())
}

/// Splitter progress is independent of a reader's completed or failed first document.
#[test]
fn p5_splitter_continues_after_reader_stops() -> eyre::Result<()> {
    let mut oracle = Oracle::start()?;
    for prefix in [
        indoc!(
            "
            a: b
            ...
            ignored: [
        "
        ),
        indoc!(
            "
            a: [}
            continuation
        "
        ),
    ] {
        // Go validates one raw cache before delivering a document event.
        let padding = "x".repeat(600);
        let text = formatdoc!(
            "
            {prefix}{padding}
            ---next: yes
        "
        );
        let reply = oracle.request(&json!({"op":"stages","text":text,"width":0}))?;
        let mut splitter = stages::Splitter::default();
        let mut chunks = Vec::new();
        let mut chunk = String::new();
        let mut reader = Some(Reader::default());
        let mut stopped = false;
        for ch in text.chars() {
            for event in splitter.push(ch) {
                match event {
                    stages::SplitEvent::Text(text) => {
                        chunk.push_str(&text);
                        if let Some(active) = &mut reader {
                            for ch in text.chars() {
                                if active.step(ch, &mut |_| Ok(())).is_err() || active.is_done() {
                                    reader = None;
                                    stopped = true;
                                    break;
                                }
                            }
                        }
                    }
                    stages::SplitEvent::End => {
                        chunks.push(std::mem::take(&mut chunk));
                        reader = Some(Reader::default());
                    }
                }
            }
        }
        for event in splitter.finish() {
            match event {
                stages::SplitEvent::Text(text) => chunk.push_str(&text),
                stages::SplitEvent::End => chunks.push(std::mem::take(&mut chunk)),
            }
        }
        if !stopped {
            eyre::bail!("reader did not stop before the later separator in {text:?}");
        }
        sim_assert_eq!(have: json!(chunks), want: reply["split"].clone());
    }
    oracle.finish()
}

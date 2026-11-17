//! Stage laws compare the composed streaming state machines with the pinned Go functions.

mod oracle;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_goyaml::stages::{self, Indent, Quote, SplitEvent, Splitter, StripNoValue, Trim};
use serde::Deserialize;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use oracle::Oracle;

const SPLITTER: &[char] = &[
    ' ', '\n', '\t', '\r', '-', 'a', '.', '#', 'x', '\u{c}', '\u{85}',
];
const STAGE: &[char] = &[
    ' ', '\t', '\n', '\r', '\u{c}', '\u{85}', '<', 'n', 'o', 'v', 'a', 'l', 'u', 'e', '>', 'x',
    '\'', '"', '\\',
];

fn words(
    alphabet: &[char],
    max: usize,
    mut visit: impl FnMut(&str) -> eyre::Result<()>,
) -> eyre::Result<u64> {
    let mut count = 0;
    for length in 0..=max {
        let mut digits = vec![0; length];
        loop {
            let text = digits
                .iter()
                .map(|index| {
                    alphabet
                        .get(*index)
                        .copied()
                        .ok_or_eyre("invalid stage alphabet index")
                })
                .collect::<eyre::Result<String>>()?;
            visit(&text)?;
            count += 1;
            let mut carry = true;
            for digit in digits.iter_mut().rev() {
                *digit += 1;
                if *digit < alphabet.len() {
                    carry = false;
                    break;
                }
                *digit = 0;
            }
            if carry {
                break;
            }
        }
    }
    Ok(count)
}

fn split_stream(text: &str) -> Vec<String> {
    let mut stage = Splitter::default();
    let mut chunks = Vec::new();
    let mut pending = String::new();
    let mut consume = |event| match event {
        SplitEvent::Text(text) => pending.push_str(&text),
        SplitEvent::End => chunks.push(std::mem::take(&mut pending)),
    };
    for character in text.chars() {
        for event in stage.push(character) {
            consume(event);
        }
    }
    for event in stage.finish() {
        consume(event);
    }

    chunks
}

fn stage_stream(text: &str, width: usize) -> Value {
    let mut indent = Indent::new(width, false);
    let mut nindent = Indent::new(width, true);
    let mut trim = Trim::default();
    let mut quote = Quote::default();
    let mut strip = StripNoValue::default();
    let mut indented = String::new();
    let mut nindented = String::new();
    let mut trimmed = String::new();
    let mut quoted = String::new();
    let mut stripped = String::new();
    for character in text.chars() {
        indented.push_str(&indent.push(character));
        nindented.push_str(&nindent.push(character));
        trimmed.push_str(&trim.push(character));
        quoted.push_str(&quote.push(character));
        stripped.push_str(&strip.push(character));
    }
    indented.push_str(&indent.finish());
    nindented.push_str(&nindent.finish());
    trimmed.push_str(&trim.finish());
    quoted.push_str(&quote.finish());
    stripped.push_str(&strip.finish());
    json!({"split":split_stream(text),"indent":indented,"nindent":nindented,"trim":trimmed,"quote":quoted,"strip":stripped})
}

fn split_batch(oracle: &mut Oracle, batch: &mut Vec<String>) -> eyre::Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let response = oracle.request(&json!({"op":"split_batch","data":batch}))?;
    let replies = response
        .as_array()
        .ok_or_eyre("split oracle response is not an array")?;
    sim_assert_eq!(have: replies.len(), want: batch.len());
    for (text, expected) in batch.iter().zip(replies) {
        let actual = json!(split_stream(text));
        if actual != *expected {
            eyre::bail!("split input {text:?}: port {actual}; Go {expected}");
        }
    }
    batch.clear();
    Ok(())
}

/// Every held whitespace and dash combination is driven through Splitter itself.
#[test]
fn p5_splitter_struct_exhaustive() -> eyre::Result<()> {
    let mut oracle = Oracle::start()?;
    let mut batch = Vec::new();
    let count = words(SPLITTER, 7, |text| {
        batch.push(text.to_owned());
        if batch.len() == 512 {
            split_batch(&mut oracle, &mut batch)?;
        }
        Ok(())
    })?;
    split_batch(&mut oracle, &mut batch)?;
    eprintln!(
        "P5 streaming splitter alphabet={} max=7 inputs={count}",
        SPLITTER.len()
    );
    oracle.finish()
}

fn stage_batch(oracle: &mut Oracle, batch: &mut Vec<String>) -> eyre::Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    for width in [0, 1, 2, 8] {
        let response = oracle.request(&json!({"op":"stages_batch","data":batch,"width":width}))?;
        let replies = response
            .as_array()
            .ok_or_eyre("stage oracle response is not an array")?;
        sim_assert_eq!(have: replies.len(), want: batch.len());
        for (text, expected) in batch.iter().zip(replies) {
            let actual = stage_stream(text, width);
            if actual != *expected {
                eyre::bail!("stage input {text:?} width={width}: port {actual}; Go {expected}");
            }
        }
    }
    batch.clear();
    Ok(())
}

/// The concrete stage states, including their EOF behavior, have Go as their only expectation.
#[test]
fn p5_stage_structs_exhaustive() -> eyre::Result<()> {
    let mut oracle = Oracle::start()?;
    let mut batch = Vec::new();
    let count = words(STAGE, 4, |text| {
        batch.push(text.to_owned());
        if batch.len() == 512 {
            stage_batch(&mut oracle, &mut batch)?;
        }
        Ok(())
    })?;
    stage_batch(&mut oracle, &mut batch)?;
    // Complete and partial literal matches exceed the exhaustive language's four-character bound.
    for text in [
        "<no value>",
        "<no value><no value>",
        "<no val<no value>ue>",
        "a<no value>b",
        "<<no value>>",
    ] {
        batch.push(text.to_owned());
    }
    let needle = "<no value>";
    for boundary in 0..=needle.len() {
        batch.push(
            needle
                .get(..boundary)
                .ok_or_eyre("invalid literal prefix boundary")?
                .to_owned(),
        );
        batch.push(
            needle
                .get(boundary..)
                .ok_or_eyre("invalid literal suffix boundary")?
                .to_owned(),
        );
    }
    let extra = batch.len();
    stage_batch(&mut oracle, &mut batch)?;
    eprintln!(
        "P5 streaming stages alphabet={} max=4 inputs={count} extras={extra} widths=4 comparisons={}",
        STAGE.len(),
        (count + u64::try_from(extra)?) * 4
    );
    oracle.finish()
}

#[derive(Deserialize)]
struct UnicodeReply {
    codepoint: u32,
    quote: String,
    space: bool,
}

/// Every Unicode scalar is checked against Go's quote and Unicode whitespace implementation.
#[test]
fn p5_every_unicode_scalar() -> eyre::Result<()> {
    let mut oracle = Oracle::start()?;
    let mut count = 0_u64;
    for from in (0..0x0011_0000_u32).step_by(4096) {
        let to = (from + 4096).min(0x0011_0000);
        let response = oracle.request(&json!({"op":"unicode","from":from,"to":to}))?;
        let replies: Vec<UnicodeReply> = serde_json::from_value(response)?;
        let mut replies = replies.into_iter();
        for codepoint in from..to {
            let Some(character) = char::from_u32(codepoint) else {
                continue;
            };
            let expected = replies.next().ok_or_eyre("Go omitted a Unicode scalar")?;
            sim_assert_eq!(have: expected.codepoint, want: codepoint);
            let mut quote = Quote::default();
            let mut actual = quote.push(character);
            actual.push_str(&quote.finish());
            if actual != expected.quote || stages::is_space(character) != expected.space {
                eyre::bail!(
                    "unicode U+{codepoint:04X}: port quote={actual:?} space={}; Go quote={:?} space={}",
                    stages::is_space(character),
                    expected.quote,
                    expected.space
                );
            }
            count += 1;
        }
        sim_assert_eq!(have: replies.next().is_none(), want: true);
    }
    eprintln!("P5 Unicode scalars={count}; quote and is_space both checked against Go");
    oracle.finish()
}

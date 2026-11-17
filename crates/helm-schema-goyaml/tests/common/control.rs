use std::collections::HashMap;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_goyaml::{
    Error, ErrorKind, Event, EventKind, Mark, Reader, ReaderControl, ScalarStyle,
};
use indoc::{formatdoc, indoc};
use serde::Deserialize;
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use crate::oracle::Oracle;

use super::{STRUCTURAL, encoded, words};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Role {
    StreamStart,
    StreamEnd,
    DocumentStart { implicit: bool },
    DocumentEnd { implicit: bool },
    Alias,
    Scalar { style: String, implicit: bool },
    SequenceStart { implicit: bool },
    SequenceEnd,
    MappingStart { implicit: bool },
    MappingEnd,
}

fn role(event: &Event) -> Role {
    match &event.kind {
        EventKind::StreamStart => Role::StreamStart,
        EventKind::StreamEnd => Role::StreamEnd,
        EventKind::DocumentStart { implicit } => Role::DocumentStart {
            implicit: *implicit,
        },
        EventKind::DocumentEnd { implicit } => Role::DocumentEnd {
            implicit: *implicit,
        },
        EventKind::Alias(_) => Role::Alias,
        EventKind::Scalar {
            anchor: _,
            tag: _,
            value: _,
            style,
            implicit,
        } => Role::Scalar {
            style: match style {
                ScalarStyle::Plain => "plain",
                ScalarStyle::SingleQuoted => "single_quoted",
                ScalarStyle::DoubleQuoted => "double_quoted",
                ScalarStyle::Literal => "literal",
                ScalarStyle::Folded => "folded",
            }
            .to_owned(),
            implicit: *implicit,
        },
        EventKind::SequenceStart {
            anchor: _,
            tag: _,
            implicit,
        } => Role::SequenceStart {
            implicit: *implicit,
        },
        EventKind::SequenceEnd => Role::SequenceEnd,
        EventKind::MappingStart {
            anchor: _,
            tag: _,
            implicit,
        } => Role::MappingStart {
            implicit: *implicit,
        },
        EventKind::MappingEnd => Role::MappingEnd,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Future {
    roles: Vec<Role>,
    error: Option<ErrorKind>,
    done: bool,
    end_offset: Option<i128>,
}

fn future(prefix: &str, snapshot: &Reader, suffix: &str) -> Future {
    let mut reader = snapshot.clone();
    let mut roles = Vec::new();
    let prior_end = reader.document_end();
    let mut sink = |event: &Event| {
        roles.push(role(event));
        Ok(())
    };
    let result = (|| {
        for character in suffix.chars() {
            reader.step(character, &mut sink)?;
        }
        reader.finish(&mut sink)
    })();
    let end_offset = if prior_end.is_none() {
        reader
            .document_end()
            .map(|mark| mark.index as i128 - prefix.chars().count() as i128)
    } else {
        None
    };
    Future {
        roles,
        error: result.err().map(|err| err.kind()),
        done: reader.is_done(),
        end_offset,
    }
}

struct Prefix {
    text: String,
    reader: Reader,
    roles: Vec<Role>,
}

fn prefix(text: String) -> Prefix {
    let mut reader = Reader::default();
    let mut roles = Vec::new();
    for character in text.chars() {
        if reader
            .step(character, &mut |event| {
                roles.push(role(event));
                Ok(())
            })
            .is_err()
        {
            // Committed errors are valid merge states; subsequent input cannot change them.
            break;
        }
    }
    Prefix {
        text,
        reader,
        roles,
    }
}

fn check_group(group: &[Prefix], suffixes: &[String]) -> eyre::Result<u64> {
    let Some((representative, members)) = group.split_first() else {
        return Ok(0);
    };
    let mut comparisons = 0;
    for suffix in suffixes {
        let expected = future(&representative.text, &representative.reader, suffix);
        for member in members {
            let actual = future(&member.text, &member.reader, suffix);
            if actual != expected {
                eyre::bail!(
                    "equal controls have unequal futures: prefixes {:?}/{:?}; suffix {suffix:?}; actual {actual:?}; expected {expected:?}",
                    member.text,
                    representative.text
                );
            }
            comparisons += 1;
        }
    }
    Ok(comparisons)
}

/// Every bounded prefix in a control class has the representative's future for every suffix.
#[test]
fn p2_equal_controls_have_equal_futures() -> eyre::Result<()> {
    let suffixes = suffixes(3);
    let mut groups: HashMap<ReaderControl, Vec<Prefix>> = HashMap::new();
    let mut alphabet = STRUCTURAL.to_vec();
    alphabet.extend(*b"\ta");
    let prefixes = words(&alphabet, 3, |bytes| {
        let prefix = prefix(String::from_utf8(bytes.to_vec())?);
        groups
            .entry(prefix.reader.control())
            .or_default()
            .push(prefix);
        Ok(())
    })?;
    let mut comparisons = 0;
    let mut nontrivial = 0;
    for group in groups.values().filter(|group| group.len() > 1) {
        nontrivial += 1;
        comparisons += check_group(group, &suffixes)?;
    }
    assert!(
        nontrivial > 0 && comparisons > 0,
        "the merge-key law must compare distinct prefixes"
    );
    eprintln!(
        "P2 control prefixes={prefixes}; suffixes={}; classes={}; nontrivial={nontrivial}; pair-suffix comparisons={comparisons}",
        suffixes.len(),
        groups.len()
    );
    Ok(())
}

/// Real render prefixes with equal controls have equal continuations, including EOF.
#[test]
fn p2_corpus_control_futures() -> eyre::Result<()> {
    let mut groups: HashMap<ReaderControl, Vec<Prefix>> = HashMap::new();
    for render in crate::corpus()? {
        let text = std::str::from_utf8(&render)?;
        for length in [16, 32, 64, 128, 256] {
            let text: String = text.chars().take(length).collect();
            if text.chars().count() != length {
                continue;
            }
            let prefix = prefix(text);
            let group = groups.entry(prefix.reader.control()).or_default();
            if group.len() < 2 && !group.iter().any(|existing| existing.text == prefix.text) {
                group.push(prefix);
            }
        }
    }
    let suffixes = suffixes(3);
    let mut compared = 0;
    let mut pairs = 0;
    let mut paired: Vec<_> = groups.values().filter(|group| group.len() > 1).collect();
    paired.sort_by(|left, right| {
        left.first()
            .map(|prefix| &prefix.text)
            .cmp(&right.first().map(|prefix| &prefix.text))
    });
    for group in paired.into_iter().take(64) {
        pairs += 1;
        compared += check_group(group, &suffixes)?;
    }
    assert!(
        pairs > 0,
        "the corpus must contribute distinct equal-control prefixes"
    );
    sim_assert_eq!(have: compared, want: pairs * u64::try_from(suffixes.len())?);
    eprintln!(
        "P2 corpus control pairs={pairs}; suffixes={}; pair-suffix comparisons={compared}",
        suffixes.len()
    );
    Ok(())
}

/// Includes invalid raw characters, BOMs and tails on both sides of the next raw-window edge.
fn suffixes(max: usize) -> Vec<String> {
    let mut alphabet: Vec<char> = STRUCTURAL.iter().map(|byte| char::from(*byte)).collect();
    alphabet.extend(['\t', 'a', '\u{1}', '\u{feff}']);
    let mut suffixes = vec![String::new()];
    let mut level = vec![String::new()];
    for _ in 0..max {
        let mut next = Vec::new();
        for prefix in &level {
            for character in &alphabet {
                next.push(format!("{prefix}{character}"));
            }
        }
        suffixes.extend(next.iter().cloned());
        level = next;
    }
    for padding in 500..=515 {
        for tail in [
            "\u{1}",
            "\u{feff}",
            indoc!(
                "

                \u{feff}x"
            ),
            indoc!(
                "

                ...
                \u{1}"
            ),
        ] {
            suffixes.push(format!("{}{tail}", " ".repeat(padding)));
        }
    }
    suffixes
}

struct OracleFuture {
    input: String,
    prefix_roles: Vec<Role>,
    future: Future,
    prior_done: bool,
    prefix_chars: usize,
}

#[derive(Debug, Deserialize)]
struct OracleProbe {
    events: Vec<Role>,
    end: Option<Mark>,
    error: Option<Error>,
}

/// Checks resumed snapshots against Go's committed outcomes and successful event sequences.
fn check_oracle_futures(oracle: &mut Oracle, batch: &mut Vec<OracleFuture>) -> eyre::Result<u64> {
    if batch.is_empty() {
        return Ok(0);
    }
    let replies = oracle.request(&json!({
        "op": "v2_batch",
        "data": batch.iter().map(|case| encoded(case.input.as_bytes())).collect::<Vec<_>>()
    }))?;
    let replies = replies
        .as_array()
        .ok_or_eyre("oracle futures are not an array")?;
    sim_assert_eq!(have: replies.len(), want: batch.len());
    for (case, reply) in batch.iter().zip(replies) {
        let OracleProbe { events, end, error } = serde_json::from_value(
            reply
                .get("probe")
                .ok_or_eyre("oracle future has no probe")?
                .clone(),
        )?;
        let error_kind = error.as_ref().map(Error::kind);
        let end_offset = if case.prior_done {
            None
        } else {
            end.map(|mark| mark.index as i128 - case.prefix_chars as i128)
        };
        if case.future.error != error_kind
            || case.future.done != error.is_none()
            || case.future.end_offset != end_offset
        {
            eyre::bail!(
                "window continuation differs from Go: input {:?}; port {:?}; Go error {error:?}, end {end:?}",
                case.input,
                case.future
            );
        }
        // Go validates each raw window before emitting events.
        // Rust emits provisional events, so successful documents compare the complete sequence.
        if error.is_none() {
            let actual: Vec<_> = case
                .prefix_roles
                .iter()
                .chain(&case.future.roles)
                .cloned()
                .collect();
            if actual != events {
                eyre::bail!(
                    "window event roles differ from Go: input {:?}; port {actual:?}; Go {events:?}",
                    case.input
                );
            }
        }
    }
    let count = u64::try_from(batch.len())?;
    batch.clear();
    Ok(count)
}

/// Raw-window crossings, including the short/long unit-test pair, preserve Go-observed futures.
#[test]
fn p2_window_control_futures_match_go() -> eyre::Result<()> {
    let mut groups: HashMap<ReaderControl, Vec<Prefix>> = HashMap::new();
    let mut prefixes = 0;
    for padding in 505..=515 {
        // Equal-length comment payloads can merge.
        // A mapping before padding keeps the parser active.
        for lead in [
            format!("#{}\n", " ".repeat(padding - 2)),
            format!("#{}\n", "x".repeat(padding - 2)),
            formatdoc!(
                "
                a: b
                #{}
                ",
                " ".repeat(padding - 7)
            ),
            formatdoc!(
                "
                a: b
                ...
                {}",
                " ".repeat(padding - 9)
            ),
        ] {
            words(STRUCTURAL, 2, |word| {
                let prefix = prefix(format!("{lead}{}", std::str::from_utf8(word)?));
                let group = groups.entry(prefix.reader.control()).or_default();
                if !group.iter().any(|existing| existing.text == prefix.text) {
                    group.push(prefix);
                }
                prefixes += 1;
                Ok(())
            })?;
        }
    }
    // These are the exact 12-byte and 524-byte prefixes from the private control equality test.
    let short = prefix(
        indoc!(
            "
        a: x
        b: left"
        )
        .to_owned(),
    );
    let long = prefix(formatdoc!(
        "
        #{}
        a: y
        b: raft",
        " ".repeat(510)
    ));
    sim_assert_eq!(have: short.reader.control(), want: long.reader.control());
    let unit_pair = vec![short, long];
    let suffixes = suffixes(2);
    let mut oracle = Oracle::start()?;
    let mut batch = Vec::new();
    let mut comparisons = 0;
    let mut oracle_cases = 0;
    let mut crossing_groups = 0;
    for group in groups
        .values()
        .filter(|group| group.len() > 1)
        .chain(std::iter::once(&unit_pair))
    {
        comparisons += check_group(group, &suffixes)?;
        if group.iter().any(|prefix| prefix.text.len() >= 512) {
            crossing_groups += 1;
        }
        for suffix in &suffixes {
            for prefix in group {
                batch.push(OracleFuture {
                    input: format!("{}{suffix}", prefix.text),
                    prefix_roles: prefix.roles.clone(),
                    future: future(&prefix.text, &prefix.reader, suffix),
                    prior_done: prefix.reader.is_done(),
                    prefix_chars: prefix.text.chars().count(),
                });
                if batch.len() == 256 {
                    oracle_cases += check_oracle_futures(&mut oracle, &mut batch)?;
                }
            }
        }
    }
    oracle_cases += check_oracle_futures(&mut oracle, &mut batch)?;
    assert!(
        comparisons > 0 && crossing_groups > 0 && oracle_cases > 0,
        "the window law must compare crossing prefixes and Go continuations"
    );
    eprintln!(
        "P2 window prefixes={prefixes}; suffixes={}; crossing groups={crossing_groups}; pair-suffix comparisons={comparisons}; Go continuations={oracle_cases}",
        suffixes.len()
    );
    oracle.finish()
}

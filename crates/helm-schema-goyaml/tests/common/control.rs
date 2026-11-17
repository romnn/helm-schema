use std::collections::HashMap;

use color_eyre::eyre;
use helm_schema_goyaml::{ErrorKind, Event, EventKind, Reader, ReaderControl, ScalarStyle};
use test_util::prelude::sim_assert_eq;

use super::{STRUCTURAL, words};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Role {
    StreamStart,
    StreamEnd,
    DocumentStart(bool),
    DocumentEnd(bool),
    Alias,
    Scalar { style: ScalarStyle, implicit: bool },
    SequenceStart(bool),
    SequenceEnd,
    MappingStart(bool),
    MappingEnd,
}

fn role(event: &Event) -> Role {
    match &event.kind {
        EventKind::StreamStart => Role::StreamStart,
        EventKind::StreamEnd => Role::StreamEnd,
        EventKind::DocumentStart { implicit } => Role::DocumentStart(*implicit),
        EventKind::DocumentEnd { implicit } => Role::DocumentEnd(*implicit),
        EventKind::Alias(_) => Role::Alias,
        EventKind::Scalar {
            anchor: _,
            tag: _,
            value: _,
            style,
            implicit,
        } => Role::Scalar {
            style: *style,
            implicit: *implicit,
        },
        EventKind::SequenceStart {
            anchor: _,
            tag: _,
            implicit,
        } => Role::SequenceStart(*implicit),
        EventKind::SequenceEnd => Role::SequenceEnd,
        EventKind::MappingStart {
            anchor: _,
            tag: _,
            implicit,
        } => Role::MappingStart(*implicit),
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
}

fn prefix(text: String) -> eyre::Result<Prefix> {
    let mut reader = Reader::default();
    for character in text.chars() {
        reader.step(character, &mut |_| Ok(()))?;
    }
    Ok(Prefix { text, reader })
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
    let mut alphabet = STRUCTURAL.to_vec();
    alphabet.extend(*b"\ta");
    let mut suffixes = Vec::new();
    words(&alphabet, 3, |bytes| {
        suffixes.push(String::from_utf8(bytes.to_vec())?);
        Ok(())
    })?;
    let mut groups: HashMap<ReaderControl, Vec<Prefix>> = HashMap::new();
    let prefixes = words(&alphabet, 3, |bytes| {
        let prefix = prefix(String::from_utf8(bytes.to_vec())?)?;
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
            let prefix = prefix(text)?;
            let group = groups.entry(prefix.reader.control()).or_default();
            if group.len() < 2 && !group.iter().any(|existing| existing.text == prefix.text) {
                group.push(prefix);
            }
        }
    }
    let mut alphabet = STRUCTURAL.to_vec();
    alphabet.extend(*b"\ta");
    let mut suffixes = Vec::new();
    words(&alphabet, 3, |bytes| {
        suffixes.push(String::from_utf8(bytes.to_vec())?);
        Ok(())
    })?;
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

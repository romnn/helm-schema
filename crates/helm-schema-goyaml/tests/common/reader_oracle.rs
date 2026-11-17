use super::{compare, encoded};
use crate::oracle::Oracle;
use color_eyre::eyre::{self, OptionExt as _};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

fn cache_cases() -> Vec<Vec<u8>> {
    let mut cases = vec![
        "\n\u{feff}\n".as_bytes().to_vec(),
        "\n\u{feff}".as_bytes().to_vec(),
        "---\n\u{feff}\n".as_bytes().to_vec(),
        "\n\u{feff}x".as_bytes().to_vec(),
        "---\n\u{feff}x".as_bytes().to_vec(),
        "a: b\n\u{feff}c: d".as_bytes().to_vec(),
        "\n\u{feff}\nx".as_bytes().to_vec(),
    ];
    for padding in 495..=520 {
        let text = format!("#{}\n\u{feff}key: x\na: b\n", " ".repeat(padding));
        cases.push(text.as_bytes().to_vec());
        for little_endian in [false, true] {
            let mut input = if little_endian {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for unit in text.encode_utf16() {
                input.extend(if little_endian {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            cases.push(input);
        }
        let text = format!("#{}\n\u{10000}\u{a0}key: value\n", " ".repeat(padding));
        cases.push(text.into_bytes());
        for hostile in [0xff, 0x00, 0x01] {
            let mut input = b"a: b\n...\n".to_vec();
            input.resize(padding, b' ');
            input.push(hostile);
            cases.push(input);
        }
    }
    // A terminal node can finish without asking the byte reader to complete a raw prefix.
    // Shorter prefixes still need scanner lookahead, so their incomplete character wins.
    let prefixes: &[&[u8]] = &[
        b"a: b\n...\n",
        b"a: b\n...",
        b"a: b\n...\n[open",
        b"]xxxx",
        b"[}xxxx",
        b"[}",
        b"a: [}xxxx",
        b"a: *x\nb: [}",
        b"a: *x",
    ];
    let utf8_tails: &[&[u8]] = &[
        b"\xe1",
        b"\xe1\x80",
        b"\xc0",
        b"\xed\xa0",
        b"\xf5\x80\x80",
        b"\xff",
        b"\0",
        b"\xc0\x80",
        b"\xed\xa0\x80",
        b"\xf5\x80\x80\x80",
    ];
    for prefix in prefixes {
        for tail in utf8_tails {
            let mut input = prefix.to_vec();
            input.extend_from_slice(tail);
            cases.push(input);
        }
        for little_endian in [false, true] {
            let mut encoded = if little_endian {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for byte in *prefix {
                let unit = u16::from(*byte);
                encoded.extend(if little_endian {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            let high = if little_endian {
                [0x00, 0xd8]
            } else {
                [0xd8, 0x00]
            };
            for tail in [
                vec![0x61],
                high.to_vec(),
                [high.as_slice(), &[0x00]].concat(),
            ] {
                let mut input = encoded.clone();
                input.extend(tail);
                cases.push(input);
            }
        }
    }

    cases
}

#[test]
fn reader_cache_boundaries_match_go() -> eyre::Result<()> {
    let cases = cache_cases();
    let mut oracle = Oracle::start()?;
    let replies = oracle.request(&json!({"op":"v2_batch","data":cases.iter().map(|input| encoded(input)).collect::<Vec<_>>()}))?;
    let replies = replies
        .as_array()
        .ok_or_eyre("oracle batch is not an array")?;
    if cases.len() != replies.len() {
        eyre::bail!("oracle batch length mismatch");
    }
    for (input, expected) in cases.iter().zip(replies) {
        compare(input, expected, false)?;
    }
    eprintln!("P1 reader cache oracle cases={}", cases.len());
    oracle.finish()
}

/// Explicit numeric and boolean tags skip timestamp inference before reporting the resolved type.
#[test]
fn timestamp_inference_obeys_the_tag_gate() -> eyre::Result<()> {
    let mut oracle = Oracle::start()?;
    let mut count = 0;
    for tag in ["!!int", "!!float", "!!bool"] {
        for value in ["2001-1-1", "2001-1-1T1:2:3Z", "2001-1-1  1:2:3"] {
            let input = format!("{tag} {value}");
            let expected =
                oracle.request(&json!({"op":"read","data":encoded(input.as_bytes())}))?;
            let actual = helm_schema_goyaml::unmarshal(input.as_bytes())
                .err()
                .ok_or_eyre("explicit incompatible tag unexpectedly succeeded")?;
            let message = expected
                .pointer("/v2/error/message")
                .and_then(serde_json::Value::as_str)
                .ok_or_eyre("Go did not report the tag error")?;
            sim_assert_eq!(have: actual.to_string(), want: message.strip_prefix("yaml: ").unwrap_or(message));
            count += 1;
        }
    }
    eprintln!("P1 timestamp tag diagnostic comparisons={count}");
    oracle.finish()
}

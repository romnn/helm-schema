use super::*;
use color_eyre::eyre::{self, OptionExt as _};
use test_util::prelude::sim_assert_eq;

#[test]
fn composed_character_edits_retain_the_source_language() -> eyre::Result<()> {
    let transforms = [
        TextTransform::Replace {
            from: ".".to_owned(),
            to: "\"".to_owned(),
        },
        TextTransform::Upper,
        TextTransform::Lower,
    ];
    let states = transformed_input(
        LexicalState::Double,
        &ValuesPath::parse("text"),
        &transforms,
        &PredicateMemo::new(),
    );
    let invalid = states
        .get(&LexicalState::Invalid)
        .ok_or_eyre("missing invalid language")?;
    let helm_schema_core::PredicateKind::Guard(Guard::MatchesPattern { pattern, .. }) =
        invalid.kind()
    else {
        eyre::bail!("expected a direct source pattern");
    };
    let pattern = regress::Regex::new(pattern)?;
    // The replacement closes the surrounding YAML quote; following text breaks the token.
    for (text, want) in [("a.x", true), ("a..x", true), ("plain", false), ("", false)] {
        sim_assert_eq!(have: pattern.find(text).is_some(), want: want, "{text:?}");
    }
    let unsupported = transformed_input(
        LexicalState::Double,
        &ValuesPath::parse("text"),
        &[TextTransform::TrimPrefix("x".to_owned())],
        &PredicateMemo::new(),
    );
    sim_assert_eq!(have: unsupported, want: BTreeMap::new());
    Ok(())
}

#[test]
fn recursive_formatted_objects_do_not_launder_quoted_failures() -> eyre::Result<()> {
    let patterns = patterns(LexicalState::Double, Domain::Map, 3, &[""]);
    let invalid = patterns
        .get(&LexicalState::Invalid)
        .ok_or_eyre("missing object failure language")?;
    for (value, want) in [
        (serde_json::json!({"x": "a\"b"}), true),
        (serde_json::json!({"x": {"y": "a\"b"}, "neutral": 7}), true),
        (serde_json::json!({"x": ["a\"b"]}), true),
        (serde_json::json!({"x": "safe"}), false),
        (serde_json::json!({"x": {"y": 7}}), false),
    ] {
        sim_assert_eq!(have: invalid.matches(&value), want: Some(want), "{value}");
    }
    Ok(())
}

#[test]
fn stable_prefixes_survive_edits_without_inventing_an_unknown_tail() {
    use crate::value::{State, TextBuffer, Value};
    let state = State::new(Value::nil());
    let mut graph = TextBuffer::default();
    graph.bytes("%!s(");
    graph.emit(state.heap.freeze(&Value::unknown()));
    graph.bytes(")");
    let value = state
        .heap
        .freeze(&crate::value::text::capture(&graph, false));
    for (transform, expected) in [
        (TextTransform::Truncate(5), "%!s("),
        (TextTransform::Truncate(-2), ""),
        (TextTransform::TrimSuffix("-x".to_owned()), "%!s("),
        (TextTransform::TrimSuffix("s(x".to_owned()), "%!"),
        (TextTransform::TrimPrefix("%!".to_owned()), "s("),
    ] {
        let prefix = crate::value::text_actions::prefix(&value, &[transform], &mut 1024);
        sim_assert_eq!(have: prefix, want: expected);
    }
}

#[test]
fn case_preimages_do_not_substitute_full_unicode_folding_for_go_rune_mapping() {
    use crate::value::{State, TextBuffer, Value};
    // Go keeps sharp s under upper; Rust's full case mapping expands it to SS.
    // A following trimPrefix SS must not expose a fabricated directive prefix.
    sim_assert_eq!(have: mapped_character('ß', &[TextTransform::Upper]), want: None);
    let state = State::new(Value::nil());
    let mut graph = TextBuffer::default();
    graph.bytes("ß%!");
    graph.emit(state.heap.freeze(&Value::unknown()));
    let source = state
        .heap
        .freeze(&crate::value::text::capture(&graph, false));
    let prefix = crate::value::text_actions::prefix(
        &source,
        &[
            TextTransform::Upper,
            TextTransform::TrimPrefix("SS".to_owned()),
        ],
        &mut 1024,
    );
    sim_assert_eq!(have: prefix, want: "");
}

#[test]
fn replacement_growth_is_bounded_before_allocating_the_result() {
    sim_assert_eq!(have: crate::value::text::bounded_replace("aa", "a", "xxxx", 5), want: None);
    sim_assert_eq!(have: crate::value::text::bounded_replace("é", "", "-", 4), want: Some("-é-".to_owned()));
    let transform = TextTransform::Replace {
        from: "a".to_owned(),
        to: "x".repeat(4096),
    };
    sim_assert_eq!(have: mapped_text("aa", &[transform]), want: None);
}

#[test]
fn formatted_map_keys_keep_their_colon_boundary_before_members() -> eyre::Result<()> {
    let patterns = patterns(LexicalState::PlainStart, Domain::Map, 3, &[""]);
    let invalid = patterns
        .get(&LexicalState::BlockInvalid)
        .ok_or_eyre("missing plain mapping failure language")?;
    for (value, want) in [
        (serde_json::json!({"a: b": {}}), true),
        (serde_json::json!({"a": {}}), false),
        (serde_json::json!({"a": {"b: c": {}}}), true),
        (serde_json::json!({"a": "safe", "b": 7}), false),
    ] {
        sim_assert_eq!(have: invalid.matches(&value), want: Some(want), "{value}");
    }
    Ok(())
}

/// Helm reads a leading `-` before a safe character, and `=` anywhere, as plain text in block
/// and flow, but its manifest splitter cuts a line-start `---` and `- ` opens a nested sequence
/// (transfer/helm-receipts/dash-plain-start/receipts.txt).
#[test]
fn dash_led_flag_words_stay_plain_until_a_third_dash() {
    let end = |text: &str| {
        text.chars()
            .try_fold(LexicalState::PlainStart, LexicalState::step)
    };
    for (text, want) in [
        ("-m", Some(LexicalState::Plain)),
        ("--port=1", Some(LexicalState::Plain)),
        ("--metrics-bind-address=:8080", Some(LexicalState::Plain)),
        ("-_", Some(LexicalState::Plain)),
        ("x=y", Some(LexicalState::Plain)),
        ("-", Some(LexicalState::PlainDash)),
        ("- x", None),
        ("---x", None),
    ] {
        sim_assert_eq!(have: end(text), want: want, "{text:?}");
    }
}

/// Inside a flow collection the same words stay plain, but `[` and `]` end the scalar
/// (transfer/helm-receipts/dash-plain-start/receipts.txt, review r2).
#[test]
fn flow_plain_words_leave_brackets_unknown() {
    let end = |text: &str| {
        text.chars()
            .try_fold(LexicalState::FlowPlainStart, LexicalState::step)
    };
    for (text, want) in [
        ("--x=a", Some(LexicalState::FlowPlain)),
        ("-m", Some(LexicalState::FlowPlain)),
        ("--x=a[b]", None),
        ("a]", None),
        ("[]", Some(LexicalState::FlowEnd)),
    ] {
        sim_assert_eq!(have: end(text), want: want, "{text:?}");
    }
}

/// A serialized number may be negative: after `--` it can spell the `---` that starts a Helm
/// 4.2.3 manifest at column zero (receipts, review r2), so only one dash admits it as a word.
#[test]
fn serialized_numbers_cross_the_two_dash_boundary_like_formatted_ones() {
    let state = crate::value::State::new(crate::value::Value::nil());
    let value = state
        .heap
        .freeze(&crate::value::Value::input(ValuesPath::parse("n")));
    let memo = PredicateMemo::new();
    let number = Predicate::from(Guard::TypeIs {
        path: ValuesPath::parse("n"),
        schema_type: "number".to_owned(),
    });
    for (start, word, admitted) in [
        (LexicalState::PlainDash, LexicalState::Plain, true),
        (LexicalState::PlainDashes, LexicalState::Plain, false),
        (
            LexicalState::FlowPlainDashes,
            LexicalState::FlowPlain,
            false,
        ),
    ] {
        let states = serialized_input(States::from([(start, Predicate::True)]), &value, &memo);
        let word = states.get(&word).cloned().unwrap_or(Predicate::False);
        sim_assert_eq!(
            have: memo.exactly_implies(&number, &word),
            want: admitted,
            "{start:?}"
        );
    }
}

/// A string domain certifies an end only when every word reaches it, after character edits only;
/// whole-text edits are elided where they run when they keep every word, so the reader certifies
/// nothing for them.
#[test]
fn string_domains_certify_only_an_end_every_word_reaches() {
    const NAME: &str = r"[a-z0-9]([-a-z0-9]*[a-z0-9])?(\.[a-z0-9]([-a-z0-9]*[a-z0-9])?)*";
    let certified = |end| States::from([(end, Predicate::True)]);
    for (start, end) in [
        (LexicalState::PlainStart, LexicalState::Plain),
        (LexicalState::Double, LexicalState::Double),
        (LexicalState::Single, LexicalState::Single),
    ] {
        sim_assert_eq!(have: domain(start, NAME, &[]), want: certified(end), "{start:?}");
    }
    let descriptor = "^(?:float64|int64)$";
    sim_assert_eq!(
        have: domain(LexicalState::PlainStart, descriptor, &[TextTransform::Upper]),
        want: certified(LexicalState::Plain),
    );
    for (pattern, transforms) in [
        // `float64` stays plain, `[]x` is a flow sequence followed by text.
        (r"^(?:float64|\[\]x)$", Vec::new()),
        (r"[\s\S]*", Vec::new()),
        (NAME, vec![TextTransform::Truncate(63)]),
        (NAME, vec![TextTransform::TrimSuffix("-".to_owned())]),
    ] {
        sim_assert_eq!(
            have: domain(LexicalState::PlainStart, pattern, &transforms),
            want: States::new(),
            "{pattern} {transforms:?}"
        );
    }
}

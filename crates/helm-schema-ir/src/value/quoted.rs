//! A partial YAML scalar-token transducer; missing transitions mean unknown.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use helm_schema_core::{
    CollectionDomain, CollectionQuantifier, Guard, Predicate, PredicateMemo, ValuePattern,
    ValuesPath,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum LexicalState {
    PlainStart,
    Plain,
    FlowEnd,
    Double,
    Single,
    SingleEnd,
    Closed,
    Separated,
    ClosedCollection,
    CollectionInvalid,
    BlockInvalid,
    Escape,
    FoldedDouble,
    Hex1,
    Hex2,
    Hex3,
    Hex4,
    Unicode4,
    Unicode5,
    Unicode5Max,
    Unicode6,
    Unicode7,
    Unicode8,
    Surrogate3,
    Invalid,
}

impl LexicalState {
    fn step(self, character: char) -> Option<Self> {
        use LexicalState::{
            BlockInvalid, Closed, ClosedCollection, CollectionInvalid, Double, Escape, FlowEnd,
            FoldedDouble, Hex1, Hex2, Hex3, Hex4, Invalid, Plain, PlainStart, Separated, Single,
            SingleEnd, Surrogate3, Unicode4, Unicode5, Unicode5Max, Unicode6, Unicode7, Unicode8,
        };
        if self.is_absorbing() {
            return Some(self);
        }
        // Removing Helm's marker cannot change any transition in this alphabet.
        if character == '<' || !alphabet().any(|allowed| allowed == character) {
            return None;
        }
        if matches!(character, '\n' | '\u{85}' | '\u{2028}' | '\u{2029}') {
            return matches!(self, Double | Escape | FoldedDouble).then_some(FoldedDouble);
        }
        Some(match self {
            PlainStart if character == ' ' => PlainStart,
            PlainStart if character.is_ascii_alphanumeric() || character == '_' => Plain,
            Plain if character.is_ascii_alphanumeric() || " _./-".contains(character) => Plain,
            PlainStart | Plain => return None,
            Double => match character {
                '"' => Closed,
                '\\' => Escape,
                _ => Double,
            },
            FoldedDouble => match character {
                ' ' | '\t' => FoldedDouble,
                // A line-start document marker may span several frozen terms.
                '-' | '.' => return None,
                _ => return Double.step(character),
            },
            Single if character == '\'' => SingleEnd,
            Single => Single,
            SingleEnd if character == '\'' => Single,
            Closed | SingleEnd | Separated => match character {
                ' ' | '\t' => Separated,
                ',' | ':' | '[' | '{' => BlockInvalid,
                ']' | '}' => ClosedCollection,
                '#' => return None,
                _ => Invalid,
            },
            ClosedCollection | FlowEnd => match character {
                ' ' | '\t' => self,
                ']' | '}' | ',' | ':' | '#' => return None,
                _ => CollectionInvalid,
            },
            Escape => escape(character),
            Hex1 => hexadecimal(character, Double),
            Hex2 => hexadecimal(character, Hex1),
            Hex3 => hexadecimal(character, Hex2),
            Unicode4 if matches!(character, 'd' | 'D') => Surrogate3,
            Hex4 | Unicode4 => hexadecimal(character, Hex3),
            Unicode5 if character == '0' => Unicode4,
            Unicode5 => hexadecimal(character, Hex4),
            Unicode5Max if character == '0' => Hex4,
            Unicode6 if character == '0' => Unicode5,
            Unicode6 if character == '1' => Unicode5Max,
            Unicode7 if character == '0' => Unicode6,
            Unicode8 if character == '0' => Unicode7,
            Surrogate3 if matches!(character, '0'..='7') => Hex2,
            Unicode5Max | Unicode6 | Unicode7 | Unicode8 | Surrogate3 => Invalid,
            BlockInvalid | CollectionInvalid | Invalid => self,
        })
    }

    pub(super) fn is_absorbing(self) -> bool {
        matches!(
            self,
            Self::Invalid | Self::BlockInvalid | Self::CollectionInvalid
        )
    }

    pub(super) fn is_unterminated(self) -> bool {
        match self {
            Self::PlainStart
            | Self::Plain
            | Self::FlowEnd
            | Self::Closed
            | Self::SingleEnd
            | Self::Separated
            | Self::ClosedCollection
            | Self::CollectionInvalid
            | Self::BlockInvalid
            | Self::Invalid => false,
            Self::Double
            | Self::FoldedDouble
            | Self::Single
            | Self::Escape
            | Self::Hex1
            | Self::Hex2
            | Self::Hex3
            | Self::Hex4
            | Self::Unicode4
            | Self::Unicode5
            | Self::Unicode5Max
            | Self::Unicode6
            | Self::Unicode7
            | Self::Unicode8
            | Self::Surrogate3 => true,
        }
    }
}

fn escape(character: char) -> LexicalState {
    match character {
        'x' => LexicalState::Hex2,
        'u' => LexicalState::Unicode4,
        'U' => LexicalState::Unicode8,
        '0' | 'a' | 'b' | 't' | '\t' | 'n' | 'v' | 'f' | 'r' | 'e' | ' ' | '"' | '\\' | '\''
        | 'N' | '_' | 'L' | 'P' => LexicalState::Double,
        _ => LexicalState::Invalid,
    }
}

fn alphabet() -> impl Iterator<Item = char> {
    ['\t', '\n']
        .into_iter()
        .chain(' '..='~')
        .chain(['\u{85}', '\u{2028}', '\u{2029}'])
}

fn hexadecimal(character: char, next: LexicalState) -> LexicalState {
    if character.is_ascii_hexdigit() {
        next
    } else {
        LexicalState::Invalid
    }
}

// Reachability derives the finite state set from the transition function.
static STATES: std::sync::LazyLock<Vec<LexicalState>> = std::sync::LazyLock::new(|| {
    let mut states = vec![
        LexicalState::PlainStart,
        LexicalState::Double,
        LexicalState::Single,
        // Formatted empty arrays enter at their closing boundary.
        LexicalState::FlowEnd,
    ];
    let mut index = 0;
    while let Some(state) = states.get(index).copied() {
        for character in alphabet() {
            if let Some(next) = state.step(character)
                && !states.contains(&next)
            {
                states.push(next);
            }
        }
        index += 1;
    }
    states.sort();
    states
});

type Languages = BTreeMap<LexicalState, String>;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct LanguageContext {
    start: LexicalState,
    suffixes: &'static [&'static str],
}
thread_local! {
    static LANGUAGES: RefCell<BTreeMap<LanguageContext, Rc<Languages>>> = RefCell::default();
}

fn languages(start: LexicalState, suffixes: &'static [&'static str]) -> Rc<Languages> {
    LANGUAGES.with_borrow_mut(|cache| {
        Rc::clone(
            cache
                .entry(LanguageContext { start, suffixes })
                .or_insert_with(|| {
                    Rc::new(
                        STATES
                            .iter()
                            .copied()
                            .filter_map(|end| {
                                language(start, end, suffixes).map(|pattern| (end, pattern))
                            })
                            .collect(),
                    )
                }),
        )
    })
}

pub(super) fn literal(
    states: BTreeMap<LexicalState, Predicate>,
    text: &str,
    memo: &PredicateMemo,
) -> BTreeMap<LexicalState, Predicate> {
    let mut result = BTreeMap::new();
    for (state, condition) in states {
        if let Some(state) = text.chars().try_fold(state, LexicalState::step) {
            merge(&mut result, state, condition, memo);
        }
    }
    result
}

pub(super) fn input(
    states: BTreeMap<LexicalState, Predicate>,
    path: &ValuesPath,
    memo: &PredicateMemo,
) -> BTreeMap<LexicalState, Predicate> {
    let mut result = BTreeMap::new();
    for (state, condition) in states {
        if state.is_absorbing() {
            merge(&mut result, state, condition, memo);
            continue;
        }
        for (end, pattern) in languages(state, &[""]).iter() {
            let selected = Predicate::all(vec![
                condition.clone(),
                Predicate::from(Guard::MatchesPattern {
                    path: path.clone(),
                    pattern: pattern.clone(),
                    templated: false,
                }),
            ]);
            merge(&mut result, *end, memo.normalize(selected), memo);
        }
    }
    result
}

#[tracing::instrument(skip_all)]
pub(super) fn formatted_input(
    states: BTreeMap<LexicalState, Predicate>,
    path: &ValuesPath,
    memo: &PredicateMemo,
) -> BTreeMap<LexicalState, Predicate> {
    let mut result = input(states.clone(), path, memo);
    for (incoming, condition) in states
        .into_iter()
        .filter(|(state, _)| !state.is_absorbing())
    {
        let absent_or_null = Predicate::Or(vec![
            Predicate::from(Guard::Absent { path: path.clone() }),
            Predicate::from(Guard::TypeIs {
                path: path.clone(),
                schema_type: "null".to_owned(),
            }),
        ]);
        merge(
            &mut result,
            incoming,
            Predicate::all(vec![condition.clone(), absent_or_null]),
            memo,
        );
        for boolean in [false, true] {
            let guard = Predicate::all(vec![
                condition.clone(),
                Predicate::from(Guard::Eq {
                    path: path.clone(),
                    value: helm_schema_core::GuardValue::Bool(boolean),
                }),
            ]);
            for (end, selected) in literal(
                BTreeMap::from([(incoming, guard)]),
                &boolean.to_string(),
                memo,
            ) {
                merge(&mut result, end, selected, memo);
            }
        }
        if let Some(end) = number_boundary(incoming) {
            merge(
                &mut result,
                end,
                Predicate::all(vec![
                    condition.clone(),
                    Predicate::from(Guard::TypeIs {
                        path: path.clone(),
                        schema_type: "number".to_owned(),
                    }),
                ]),
                memo,
            );
        }
        map_input(&mut result, incoming, &condition, path, memo);
        for (end, pattern) in array_patterns(incoming, "", 2) {
            merge(
                &mut result,
                end,
                Predicate::all(vec![
                    condition.clone(),
                    Predicate::from(Guard::MatchesValue {
                        path: path.clone(),
                        pattern,
                    }),
                ]),
                memo,
            );
        }
    }
    result
}

// Every finite Go numeric spelling preserves these boundaries; escape tails remain unknown.
fn number_boundary(state: LexicalState) -> Option<LexicalState> {
    match state {
        LexicalState::PlainStart | LexicalState::Plain => Some(LexicalState::Plain),
        LexicalState::Double | LexicalState::FoldedDouble => Some(LexicalState::Double),
        LexicalState::Single => Some(LexicalState::Single),
        LexicalState::SingleEnd | LexicalState::Closed | LexicalState::Separated => {
            Some(LexicalState::Invalid)
        }
        LexicalState::ClosedCollection | LexicalState::FlowEnd => {
            Some(LexicalState::CollectionInvalid)
        }
        LexicalState::Invalid | LexicalState::BlockInvalid | LexicalState::CollectionInvalid => {
            Some(state)
        }
        LexicalState::Escape
        | LexicalState::Hex1
        | LexicalState::Hex2
        | LexicalState::Hex3
        | LexicalState::Hex4
        | LexicalState::Unicode4
        | LexicalState::Unicode5
        | LexicalState::Unicode5Max
        | LexicalState::Unicode6
        | LexicalState::Unicode7
        | LexicalState::Unicode8
        | LexicalState::Surrogate3 => None,
    }
}

fn map_input(
    result: &mut BTreeMap<LexicalState, Predicate>,
    incoming: LexicalState,
    condition: &Predicate,
    path: &ValuesPath,
    memo: &PredicateMemo,
) {
    let Some(state) = "map[".chars().try_fold(incoming, LexicalState::step) else {
        return;
    };
    let collection = Predicate::all(vec![
        condition.clone(),
        Predicate::from(Guard::TypeIs {
            path: path.clone(),
            schema_type: "object".to_owned(),
        }),
    ]);
    if let Some(end) = state.step(']') {
        merge(
            result,
            end,
            Predicate::all(vec![
                collection.clone(),
                Predicate::truthy_path(path.encode()).negated(),
            ]),
            memo,
        );
    }
    if state.is_absorbing() {
        merge(result, state, collection, memo);
        return;
    }
    if !matches!(state, LexicalState::Double | LexicalState::Single) {
        return;
    }
    let keys = languages(state, &[":"]);
    for singleton in [false, true] {
        let members = languages(state, if singleton { &["]"] } else { &[" ", "]"] });
        let guard = Predicate::all(vec![
            collection.clone(),
            Predicate::truthy_path(path.encode()),
            if singleton {
                Predicate::from(Guard::AtMostOneMember { path: path.clone() })
            } else {
                Predicate::True
            },
        ]);
        let member_same = members.get(&state);
        let key_same = keys.get(&state);
        // A closed alphabet preserves the state until one member irreversibly exits it.
        for end in std::iter::once(state)
            .chain(STATES.iter().copied().filter(|state| state.is_absorbing()))
        {
            let member_bad = members.get(&end);
            let key_bad = keys.get(&end);
            if member_bad.is_none() && key_bad.is_none() {
                continue;
            }
            let member_allowed = union(member_same, member_bad);
            let key_allowed = union(key_same, key_bad);
            let bad = if end == state {
                Predicate::True
            } else {
                Predicate::Or(vec![
                    quantified(
                        path,
                        CollectionDomain::ObjectValues,
                        CollectionQuantifier::Any,
                        member_bad,
                    ),
                    quantified(
                        path,
                        CollectionDomain::ObjectKeys,
                        CollectionQuantifier::Any,
                        key_bad,
                    ),
                ])
            };
            merge(
                result,
                end,
                Predicate::all(vec![
                    guard.clone(),
                    bad,
                    quantified(
                        path,
                        CollectionDomain::ObjectValues,
                        CollectionQuantifier::All,
                        member_allowed.as_ref(),
                    ),
                    quantified(
                        path,
                        CollectionDomain::ObjectKeys,
                        CollectionQuantifier::All,
                        key_allowed.as_ref(),
                    ),
                ]),
                memo,
            );
        }
    }
}

// Ordered prefixes are bounded; homogeneous string members need no length bound.
#[tracing::instrument(skip_all)]
fn array_patterns(
    start: LexicalState,
    suffix: &str,
    depth: usize,
) -> BTreeMap<LexicalState, ValuePattern> {
    let mut result = BTreeMap::<_, Vec<_>>::new();
    if start == LexicalState::PlainStart {
        if let Some(end) = suffix
            .chars()
            .try_fold(LexicalState::FlowEnd, LexicalState::step)
        {
            result.entry(end).or_default().push(ValuePattern::Array {
                prefix: Vec::new(),
                items: Some(Box::new(ValuePattern::String {
                    pattern: "^[A-Za-z0-9_./+-]*$".to_owned(),
                })),
            });
        }
        return result
            .into_iter()
            .map(|(end, arms)| (end, ValuePattern::AnyOf(arms)))
            .collect();
    }
    let Some(state) = start.step('[') else {
        return BTreeMap::new();
    };
    if state.is_absorbing() {
        return BTreeMap::from([(
            state,
            ValuePattern::Array {
                prefix: Vec::new(),
                items: Some(Box::new(ValuePattern::Any)),
            },
        )]);
    }
    let closing = format!("]{suffix}");
    if let Some(end) = closing.chars().try_fold(state, LexicalState::step) {
        result.entry(end).or_default().push(ValuePattern::Array {
            prefix: Vec::new(),
            items: None,
        });
    }
    if depth == 0 || !matches!(state, LexicalState::Double | LexicalState::Single) {
        return result
            .into_iter()
            .map(|(end, arms)| (end, ValuePattern::AnyOf(arms)))
            .collect();
    }
    let members = languages(state, &[" ", "]"]);
    let same = members.get(&state);
    for end in
        std::iter::once(state).chain(STATES.iter().copied().filter(|state| state.is_absorbing()))
    {
        let bad = members.get(&end);
        let Some(allowed) = union(same, bad) else {
            continue;
        };
        let all = ValuePattern::Array {
            prefix: Vec::new(),
            items: Some(Box::new(ValuePattern::String { pattern: allowed })),
        };
        let pattern = if end == state {
            all
        } else if let Some(bad) = bad {
            ValuePattern::AllOf(vec![
                all,
                ValuePattern::Contains(Box::new(ValuePattern::String {
                    pattern: bad.clone(),
                })),
            ])
        } else {
            continue;
        };
        result.entry(end).or_default().push(pattern);
    }
    let middle = member_patterns(state, " ", depth - 1);
    let last = member_patterns(state, &closing, depth - 1);
    let mut prefix = Vec::new();
    for _ in 0..4 {
        for (end, pattern) in &last {
            let mut members = prefix.clone();
            members.push(pattern.clone());
            result.entry(*end).or_default().push(ValuePattern::Array {
                prefix: members,
                items: None,
            });
        }
        for (end, pattern) in middle.iter().filter(|(end, _)| end.is_absorbing()) {
            let mut members = prefix.clone();
            members.extend([pattern.clone(), ValuePattern::Any]);
            result.entry(*end).or_default().push(ValuePattern::Array {
                prefix: members,
                items: Some(Box::new(ValuePattern::Any)),
            });
        }
        match middle.get(&state) {
            Some(pattern) => prefix.push(pattern.clone()),
            None => break,
        }
    }
    result
        .into_iter()
        .map(|(end, arms)| (end, ValuePattern::AnyOf(arms)))
        .collect()
}

fn member_patterns(
    start: LexicalState,
    suffix: &str,
    depth: usize,
) -> BTreeMap<LexicalState, ValuePattern> {
    let mut result = BTreeMap::<_, Vec<_>>::new();
    for (state, pattern) in languages(start, &[""]).iter() {
        if let Some(end) = suffix.chars().try_fold(*state, LexicalState::step) {
            result.entry(end).or_default().push(ValuePattern::String {
                pattern: pattern.clone(),
            });
        }
    }
    if depth > 0 {
        for (end, pattern) in array_patterns(start, suffix, depth) {
            if end.is_absorbing() {
                result.entry(end).or_default().push(pattern);
            }
        }
    }
    result
        .into_iter()
        .map(|(end, arms)| (end, ValuePattern::AnyOf(arms)))
        .collect()
}

fn quantified(
    path: &ValuesPath,
    domain: CollectionDomain,
    quantifier: CollectionQuantifier,
    pattern: Option<&String>,
) -> Predicate {
    pattern.map_or(Predicate::False, |pattern| {
        Predicate::from(Guard::CollectionPattern {
            path: path.clone(),
            domain,
            quantifier,
            pattern: pattern.clone(),
        })
    })
}

fn union(left: Option<&String>, right: Option<&String>) -> Option<String> {
    match (left, right) {
        (None, None) => None,
        (Some(text), None) | (None, Some(text)) => Some(text.clone()),
        (Some(left), Some(right)) if left == right => Some(left.clone()),
        (Some(left), Some(right)) => Some(format!("(?:{left}|{right})")),
    }
}

pub(super) fn merge(
    states: &mut BTreeMap<LexicalState, Predicate>,
    state: LexicalState,
    condition: Predicate,
    memo: &PredicateMemo,
) {
    if condition == Predicate::False {
        return;
    }
    states
        .entry(state)
        .and_modify(|old| {
            *old = memo.normalize(Predicate::Or(vec![old.clone(), condition.clone()]));
        })
        .or_insert(condition);
}

// Eliminate the finite lexical states to derive exact preimages of each boundary state.
// The two extra vertices keep start/end distinct, including an empty input.
fn language(start: LexicalState, end: LexicalState, suffixes: &[&str]) -> Option<String> {
    let mut edges = BTreeMap::<(usize, usize), String>::new();
    for (from, state) in STATES.iter().enumerate() {
        for (to, next) in STATES.iter().enumerate() {
            let characters: Vec<_> = alphabet()
                .filter(|character| state.step(*character) == Some(*next))
                .map(u32::from)
                .collect();
            if !characters.is_empty() {
                let mut class = String::from("[");
                let mut characters = characters.into_iter().peekable();
                while let Some(first) = characters.next() {
                    let mut last = first;
                    while characters.peek() == Some(&(last + 1)) {
                        last += 1;
                        characters.next();
                    }
                    class.push_str(&class_character(first)?);
                    if last > first {
                        if last > first + 1 {
                            class.push('-');
                        }
                        class.push_str(&class_character(last)?);
                    }
                }
                class.push(']');
                edges.insert((from, to), class);
            }
        }
    }
    let entry = STATES.len();
    let exit = entry + 1;
    edges.insert(
        (entry, STATES.iter().position(|state| *state == start)?),
        String::new(),
    );
    if suffixes.is_empty() {
        return None;
    }
    for (index, state) in STATES.iter().enumerate() {
        if suffixes
            .iter()
            .all(|suffix| suffix.chars().try_fold(*state, LexicalState::step) == Some(end))
        {
            edges.insert((index, exit), String::new());
        }
    }
    // Eliminate acyclic escape tails before the quoted-content loops to keep expressions small.
    for removed in (0..STATES.len()).rev() {
        let incoming: Vec<_> = edges
            .iter()
            .filter(|((from, to), _)| *to == removed && *from != removed)
            .map(|((from, _), text)| (*from, text.clone()))
            .collect();
        let outgoing: Vec<_> = edges
            .iter()
            .filter(|((from, to), _)| *from == removed && *to != removed)
            .map(|((_, to), text)| (*to, text.clone()))
            .collect();
        let repeated = edges
            .get(&(removed, removed))
            .map_or_else(String::new, |text| format!("(?:{text})*"));
        for (from, before) in &incoming {
            for (to, after) in &outgoing {
                let text = format!("{before}{repeated}{after}");
                let key = (*from, *to);
                let text = match edges.remove(&key) {
                    Some(previous) if previous != text => format!("(?:{previous}|{text})"),
                    Some(previous) => previous,
                    None => text,
                };
                if text.len() > 16 * 1024 {
                    return None;
                }
                edges.insert(key, text);
            }
        }
        edges.retain(|(from, to), _| *from != removed && *to != removed);
    }
    edges
        .remove(&(entry, exit))
        .map(|text| format!("^(?:{text})$"))
}

fn class_character(character: u32) -> Option<String> {
    Some(if character <= 0xff {
        format!("\\x{character:02x}")
    } else {
        char::from_u32(character)?.to_string()
    })
}

pub(super) fn serialized_input(
    states: BTreeMap<LexicalState, Predicate>,
    value: &super::FrozenValue,
    memo: &PredicateMemo,
) -> BTreeMap<LexicalState, Predicate> {
    let mut result = BTreeMap::new();
    let Some(path) = value.identity() else {
        return result;
    };
    for (state, condition) in states {
        // Boolean and null spellings agree across the pinned JSON/YAML serializers.
        for schema_type in ["boolean", "null", "number"] {
            let spelling = match schema_type {
                "null" => "null",
                "number" => "0",
                _ => "true",
            };
            let guard = Predicate::all(vec![
                condition.clone(),
                Predicate::from(Guard::TypeIs {
                    path: path.clone(),
                    schema_type: schema_type.to_owned(),
                }),
            ]);
            for (end, guard) in literal(BTreeMap::from([(state, guard)]), spelling, memo) {
                merge(&mut result, end, guard, memo);
            }
        }
    }
    result
}

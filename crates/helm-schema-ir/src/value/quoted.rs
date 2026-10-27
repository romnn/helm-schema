//! A partial YAML quoted-token transducer; missing transitions mean unknown.

use std::cell::RefCell;
use std::collections::BTreeMap;

use helm_schema_core::{Guard, Predicate, PredicateMemo, ValuesPath};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum LexicalState {
    Double,
    Single,
    SingleEnd,
    Closed,
    Escape,
    Hex(u8),
    Invalid,
}

impl LexicalState {
    fn step(self, character: char) -> Option<Self> {
        use LexicalState::{Closed, Double, Escape, Hex, Invalid, Single, SingleEnd};
        if self == Invalid {
            return Some(Invalid);
        }
        // Removing Helm's marker cannot change any transition in this alphabet.
        // Newlines, Unicode and structure-changing delimiters retain unknown coverage.
        if !(' '..='~').contains(&character) || character == '<' {
            return None;
        }
        Some(match self {
            Double => match character {
                '"' => Closed,
                '\\' => Escape,
                _ => Double,
            },
            Single => {
                if character == '\'' {
                    SingleEnd
                } else {
                    Single
                }
            }
            SingleEnd if character == '\'' => Single,
            Closed | SingleEnd => {
                if " ,:[]{}#".contains(character) {
                    return None;
                }
                Invalid
            }
            Escape => match character {
                'x' => Hex(2),
                'u' => Hex(4),
                'U' => Hex(8),
                '0' | 'a' | 'b' | 't' | 'n' | 'v' | 'f' | 'r' | 'e' | ' ' | '"' | '/' | '\\'
                | '\'' | 'N' | '_' | 'L' | 'P' => Double,
                _ => Invalid,
            },
            Hex(remaining) => {
                if !character.is_ascii_hexdigit() {
                    Invalid
                } else if remaining == 1 {
                    Double
                } else {
                    Hex(remaining - 1)
                }
            }
            Invalid => Invalid,
        })
    }
}

const STATES: [LexicalState; 14] = [
    LexicalState::Double,
    LexicalState::Single,
    LexicalState::SingleEnd,
    LexicalState::Closed,
    LexicalState::Escape,
    LexicalState::Hex(1),
    LexicalState::Hex(2),
    LexicalState::Hex(3),
    LexicalState::Hex(4),
    LexicalState::Hex(5),
    LexicalState::Hex(6),
    LexicalState::Hex(7),
    LexicalState::Hex(8),
    LexicalState::Invalid,
];

type Languages = BTreeMap<LexicalState, String>;
thread_local! {
    static LANGUAGES: RefCell<BTreeMap<LexicalState, Languages>> = RefCell::default();
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
        if state == LexicalState::Invalid {
            merge(&mut result, state, condition, memo);
            continue;
        }
        LANGUAGES.with_borrow_mut(|cache| {
            let languages = cache.entry(state).or_insert_with(|| {
                STATES
                    .into_iter()
                    .filter_map(|end| language(state, end).map(|pattern| (end, pattern)))
                    .collect()
            });
            for (end, pattern) in languages {
                let selected = Predicate::all(vec![
                    condition.clone(),
                    Predicate::from(Guard::NotMatchesPattern {
                        path: path.clone(),
                        pattern: "[\\r\\n\u{2028}\u{2029}]".to_owned(),
                    }),
                    Predicate::from(Guard::MatchesPattern {
                        path: path.clone(),
                        pattern: pattern.clone(),
                        templated: false,
                    }),
                ]);
                merge(&mut result, *end, memo.normalize(selected), memo);
            }
        });
    }
    result
}

fn merge(
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
fn language(start: LexicalState, end: LexicalState) -> Option<String> {
    let mut edges = BTreeMap::<(usize, usize), String>::new();
    for (from, state) in STATES.iter().enumerate() {
        for (to, next) in STATES.iter().enumerate() {
            let characters: Vec<_> = (b' '..=b'~')
                .filter(|byte| state.step(char::from(*byte)) == Some(*next))
                .collect();
            if !characters.is_empty() {
                let mut class = String::from("[");
                for byte in characters {
                    use std::fmt::Write as _;
                    write!(class, "\\x{byte:02x}").ok()?;
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
    edges.insert(
        (STATES.iter().position(|state| *state == end)?, exit),
        String::new(),
    );
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

//! A partial YAML scalar-token transducer; missing transitions mean unknown.

use std::cell::RefCell;
use std::collections::BTreeMap;

use helm_schema_core::{Guard, Predicate, PredicateMemo, ValuesPath};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum LexicalState {
    PlainStart,
    Plain,
    Double,
    Single,
    SingleEnd,
    Closed,
    Escape,
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
            Closed, Double, Escape, Hex1, Hex2, Hex3, Hex4, Invalid, Plain, PlainStart, Single,
            SingleEnd, Surrogate3, Unicode4, Unicode5, Unicode5Max, Unicode6, Unicode7, Unicode8,
        };
        if self == Invalid {
            return Some(Invalid);
        }
        // Removing Helm's marker cannot change any transition in this alphabet.
        // Newlines, Unicode and structure-changing delimiters retain unknown coverage.
        if !(' '..='~').contains(&character) || character == '<' {
            return None;
        }
        Some(match self {
            PlainStart if character.is_ascii_alphanumeric() || character == '_' => Plain,
            Plain if character.is_ascii_alphanumeric() || " _./-".contains(character) => Plain,
            PlainStart | Plain => return None,
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
                'x' => Hex2,
                'u' => Unicode4,
                'U' => Unicode8,
                '0' | 'a' | 'b' | 't' | 'n' | 'v' | 'f' | 'r' | 'e' | ' ' | '"' | '\\' | '\''
                | 'N' | '_' | 'L' | 'P' => Double,
                _ => Invalid,
            },
            Hex1 => hexadecimal(character, Double),
            Hex2 => hexadecimal(character, Hex1),
            Hex3 => hexadecimal(character, Hex2),
            Hex4 => hexadecimal(character, Hex3),
            Unicode4 => match character {
                'd' | 'D' => Surrogate3,
                _ => hexadecimal(character, Hex3),
            },
            Unicode5 => match character {
                '0' => Unicode4,
                _ => hexadecimal(character, Hex4),
            },
            Unicode5Max => {
                if character == '0' {
                    Hex4
                } else {
                    Invalid
                }
            }
            Unicode6 => match character {
                '0' => Unicode5,
                '1' => Unicode5Max,
                _ => Invalid,
            },
            Unicode7 => {
                if character == '0' {
                    Unicode6
                } else {
                    Invalid
                }
            }
            Unicode8 => {
                if character == '0' {
                    Unicode7
                } else {
                    Invalid
                }
            }
            Surrogate3 => {
                if matches!(character, '0'..='7') {
                    Hex2
                } else {
                    Invalid
                }
            }
            Invalid => Invalid,
        })
    }

    pub(super) fn is_unterminated(self) -> bool {
        match self {
            Self::PlainStart | Self::Plain | Self::Closed | Self::SingleEnd | Self::Invalid => {
                false
            }
            Self::Double
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
    ];
    let mut index = 0;
    while let Some(state) = states.get(index).copied() {
        for byte in b' '..=b'~' {
            if let Some(next) = state.step(char::from(byte))
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
                    .iter()
                    .copied()
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
fn language(start: LexicalState, end: LexicalState) -> Option<String> {
    let mut edges = BTreeMap::<(usize, usize), String>::new();
    for (from, state) in STATES.iter().enumerate() {
        for (to, next) in STATES.iter().enumerate() {
            let characters: Vec<_> = (b' '..=b'~')
                .filter(|byte| state.step(char::from(*byte)) == Some(*next))
                .collect();
            if !characters.is_empty() {
                let mut class = String::from("[");
                let mut characters = characters.into_iter().peekable();
                while let Some(first) = characters.next() {
                    use std::fmt::Write as _;
                    let mut last = first;
                    while characters.peek() == Some(&(last + 1)) {
                        last += 1;
                        characters.next();
                    }
                    write!(class, "\\x{first:02x}").ok()?;
                    if last > first {
                        if last > first + 1 {
                            class.push('-');
                        }
                        write!(class, "\\x{last:02x}").ok()?;
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

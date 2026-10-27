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
            BlockInvalid, Closed, ClosedCollection, CollectionInvalid, Double, Escape,
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
            return matches!(self, Escape | FoldedDouble).then_some(FoldedDouble);
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
            ClosedCollection => match character {
                ' ' | '\t' => ClosedCollection,
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
        if state.is_absorbing() {
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

fn class_character(character: u32) -> Option<String> {
    Some(if character <= 0xff {
        format!("\\x{character:02x}")
    } else {
        char::from_u32(character)?.to_string()
    })
}

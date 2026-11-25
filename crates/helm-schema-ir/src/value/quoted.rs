//! A partial YAML scalar-token transducer; missing transitions mean unknown.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use super::kinds::GoType;
use super::relationship::TextTransform;

use helm_schema_core::{Guard, Predicate, PredicateMemo, ValuePattern, ValuesPath};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum LexicalState {
    PlainStart,
    Plain,
    PlainColon,
    Directive,
    DirectiveInvalid,
    FlowEmptyStart,
    /// A word inside the one flow sequence a plain start opened (`[` at the start of a scalar).
    FlowSequenceWord,
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
    /// A plain scalar's leading `-`, plain only before a safe character in block and flow.
    PlainDash,
    /// Two leading dashes; a third may complete Helm's line-start document separator.
    PlainDashes,
    /// The plain states of a scalar inside a flow collection, where `[` and `]` end it.
    FlowPlainStart,
    FlowPlain,
    FlowPlainColon,
    FlowPlainDash,
    FlowPlainDashes,
    TruncatableStart,
    TruncatableWord,
}

impl LexicalState {
    pub(super) fn step(self, character: char) -> Option<Self> {
        if self.is_absorbing() {
            return Some(self);
        }
        if matches!(self, Self::TruncatableStart | Self::TruncatableWord) {
            // Every nonempty byte prefix must leave the same plain-word state.
            let plain = if self == Self::TruncatableStart {
                Self::PlainStart
            } else {
                Self::Plain
            };
            return (character.is_ascii()
                && (self != Self::TruncatableStart || character.is_ascii_alphanumeric())
                && plain.step(character) == Some(Self::Plain))
            .then_some(Self::TruncatableWord);
        }
        if let Some(block) = self.flow_plain_as_block() {
            let next = block.step(character)?;
            return match next.flow_plain() {
                // A flow indicator would end the scalar and change the enclosing collection.
                Some(_) if matches!(character, '[' | ']') => None,
                Some(flow) => Some(flow),
                None => Some(next),
            };
        }
        self.step_block(character)
    }

    fn step_block(self, character: char) -> Option<Self> {
        use LexicalState::{
            BlockInvalid, Closed, ClosedCollection, CollectionInvalid, Directive, DirectiveInvalid,
            Double, Escape, FlowEmptyStart, FlowEnd, FlowPlain, FlowPlainColon, FlowPlainDash,
            FlowPlainDashes, FlowPlainStart, FlowSequenceWord, FoldedDouble, Hex1, Hex2, Hex3,
            Hex4, Invalid, Plain, PlainColon, PlainDash, PlainDashes, PlainStart, Separated,
            Single, SingleEnd, Surrogate3, Unicode4, Unicode5, Unicode5Max, Unicode6, Unicode7,
            Unicode8,
        };
        // Removing Helm's marker cannot change any transition in this alphabet.
        if character == '<' || !alphabet().any(|allowed| allowed == character) {
            return None;
        }
        if matches!(character, '\n' | '\u{85}' | '\u{2028}' | '\u{2029}') {
            return matches!(self, Double | Escape | FoldedDouble).then_some(FoldedDouble);
        }
        Some(match self {
            PlainStart if character == ' ' => PlainStart,
            PlainStart if character == '[' => FlowEmptyStart,
            FlowEmptyStart | FlowSequenceWord => return self.flow_sequence_step(character),
            PlainStart if character == '%' => Directive,
            Directive if character == '!' => DirectiveInvalid,
            PlainStart if character == '-' => PlainDash,
            PlainDash if character == '-' => PlainDashes,
            PlainStart | PlainDash | PlainDashes
                if character.is_ascii_alphanumeric() || "_./".contains(character) =>
            {
                Plain
            }
            Plain | PlainColon if character == ':' => PlainColon,
            PlainColon if matches!(character, ' ' | '\t') => BlockInvalid,
            // `=` never indicates inside a plain scalar, so `--flag=value` stays one word.
            Plain | PlainColon
                if character.is_ascii_alphanumeric() || " _./-[]=".contains(character) =>
            {
                Plain
            }
            // Flow plain states never reach here: they delegate to their block twins above.
            Directive
            | PlainStart
            | PlainDash
            | PlainDashes
            | Plain
            | PlainColon
            | FlowPlainStart
            | FlowPlain
            | FlowPlainColon
            | FlowPlainDash
            | FlowPlainDashes
            | Self::TruncatableStart
            | Self::TruncatableWord => {
                return None;
            }
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
            BlockInvalid | CollectionInvalid | DirectiveInvalid | Invalid => self,
        })
    }

    /// A flow sequence opened at a plain start: entries are plain words separated by spaces until
    /// its closing bracket; a collection opener or closer right after a word is not a separator,
    /// so the decoder fails there.
    fn flow_sequence_step(self, character: char) -> Option<Self> {
        let word = |extra: &str| character.is_ascii_alphanumeric() || extra.contains(character);
        Some(match self {
            Self::FlowEmptyStart if character == ' ' => Self::FlowEmptyStart,
            Self::FlowEmptyStart | Self::FlowSequenceWord if character == ']' => Self::FlowEnd,
            Self::FlowEmptyStart if word("_./+") => Self::FlowSequenceWord,
            Self::FlowSequenceWord if word("_./+-") => Self::FlowSequenceWord,
            Self::FlowSequenceWord if character == ' ' => Self::FlowEmptyStart,
            Self::FlowSequenceWord if "[{}".contains(character) => Self::Invalid,
            _ => return None,
        })
    }

    /// The block twin of a flow plain state.
    fn flow_plain_as_block(self) -> Option<Self> {
        match self {
            Self::FlowPlainStart => Some(Self::PlainStart),
            Self::FlowPlain => Some(Self::Plain),
            Self::FlowPlainColon => Some(Self::PlainColon),
            Self::FlowPlainDash => Some(Self::PlainDash),
            Self::FlowPlainDashes => Some(Self::PlainDashes),
            _ => None,
        }
    }

    /// The flow twin of a block plain state.
    pub(super) fn flow_plain(self) -> Option<Self> {
        match self {
            Self::PlainStart => Some(Self::FlowPlainStart),
            Self::Plain => Some(Self::FlowPlain),
            Self::PlainColon => Some(Self::FlowPlainColon),
            Self::PlainDash => Some(Self::FlowPlainDash),
            Self::PlainDashes => Some(Self::FlowPlainDashes),
            _ => None,
        }
    }

    pub(super) fn is_absorbing(self) -> bool {
        matches!(
            self,
            Self::Invalid | Self::BlockInvalid | Self::CollectionInvalid | Self::DirectiveInvalid
        )
    }

    /// State certified for a prefix-preserving edit of a truncatable word.
    pub(super) fn edited_word_end(self, transform: &TextTransform) -> Option<Self> {
        let preserves_word = match transform {
            TextTransform::Truncate(width) => *width > 0,
            TextTransform::TrimPrefix(affix) | TextTransform::TrimSuffix(affix) => {
                // This affix cannot erase the entire word.
                affix
                    .chars()
                    .next()
                    .is_none_or(|first| Self::TruncatableStart.step(first).is_none())
            }
            TextTransform::Upper
            | TextTransform::Lower
            | TextTransform::Replace { from: _, to: _ }
            | TextTransform::Cut(_)
            | TextTransform::RegexReplace {
                pattern: _,
                replacement: _,
            } => false,
        };
        if !preserves_word {
            return None;
        }
        match self {
            Self::PlainStart | Self::Plain => Some(Self::Plain),
            Self::TruncatableStart | Self::TruncatableWord => Some(Self::TruncatableWord),
            _ => None,
        }
    }

    pub(super) fn is_unterminated(self) -> bool {
        match self {
            Self::PlainStart
            | Self::PlainDash
            | Self::PlainDashes
            | Self::PlainColon
            | Self::FlowPlainStart
            | Self::FlowPlainDash
            | Self::FlowPlainDashes
            | Self::FlowPlainColon
            | Self::FlowPlain
            | Self::Directive
            | Self::DirectiveInvalid
            | Self::Plain
            | Self::FlowEnd
            | Self::Closed
            | Self::SingleEnd
            | Self::Separated
            | Self::ClosedCollection
            | Self::CollectionInvalid
            | Self::BlockInvalid
            | Self::Invalid
            | Self::TruncatableStart
            | Self::TruncatableWord => false,
            Self::Double
            | Self::FlowEmptyStart
            | Self::FlowSequenceWord
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
        LexicalState::FlowPlainStart,
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

fn lexical_states(start: LexicalState) -> &'static [LexicalState] {
    match start {
        LexicalState::TruncatableStart | LexicalState::TruncatableWord => &[
            LexicalState::TruncatableStart,
            LexicalState::TruncatableWord,
        ],
        _ => &STATES,
    }
}

type Languages = BTreeMap<LexicalState, String>;
pub(super) type States = BTreeMap<LexicalState, Predicate>;
type Cache<K, V> = RefCell<BTreeMap<K, Rc<V>>>;

// Cached transfers are pure grammar queries; conditions and heap facts compose at the caller.
fn cached<K: Ord, V>(cache: &Cache<K, V>, key: K, build: impl FnOnce() -> V) -> Rc<V> {
    if let Some(value) = cache.borrow().get(&key).cloned() {
        return value;
    }
    let value = Rc::new(build());
    let mut entries = cache.borrow_mut();
    if entries.len() >= 128 {
        entries.clear();
    }
    entries.insert(key, Rc::clone(&value));
    value
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LanguageContext {
    start: LexicalState,
    suffixes: Vec<String>,
    transforms: Vec<TextTransform>,
}
thread_local! {
    static LANGUAGES: Cache<LanguageContext, Languages> = RefCell::default();
}

fn languages(start: LexicalState, suffixes: &[&str]) -> Rc<Languages> {
    mapped_languages(start, suffixes, &[])
}

fn mapped_languages(
    start: LexicalState,
    suffixes: &[&str],
    transforms: &[TextTransform],
) -> Rc<Languages> {
    LANGUAGES.with(|cache| {
        cached(
            cache,
            LanguageContext {
                start,
                suffixes: suffixes.iter().map(|text| (*text).to_owned()).collect(),
                transforms: transforms.to_vec(),
            },
            || {
                lexical_states(start)
                    .iter()
                    .copied()
                    .filter_map(|end| {
                        preimage_language(start, end, suffixes, transforms)
                            .map(|pattern| (end, pattern))
                    })
                    .collect()
            },
        )
    })
}

pub(super) fn compose(
    states: States,
    mut transfer: impl FnMut(LexicalState) -> States,
    memo: &PredicateMemo,
) -> States {
    let mut result = States::new();
    for (state, condition) in states {
        for (end, guard) in transfer(state) {
            merge(
                &mut result,
                end,
                memo.normalize(Predicate::all(vec![condition.clone(), guard])),
                memo,
            );
        }
    }
    result
}

pub(super) fn literal(states: States, text: &str, memo: &PredicateMemo) -> States {
    let mut result = BTreeMap::new();
    for (state, condition) in states {
        if let Some(state) = text.chars().try_fold(state, LexicalState::step) {
            merge(&mut result, state, condition, memo);
        }
    }
    result
}

pub(super) fn input(states: States, path: &ValuesPath, memo: &PredicateMemo) -> States {
    compose(
        states,
        |state| transformed_input(state, path, &[], memo),
        memo,
    )
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FormatContext {
    state: LexicalState,
    kind: GoType,
    path: ValuesPath,
}

thread_local! {
    static FORMATTED: Cache<FormatContext, States> = RefCell::default();
}

#[tracing::instrument(skip_all)]
pub(super) fn formatted_input(
    states: States,
    path: &ValuesPath,
    kind: GoType,
    memo: &PredicateMemo,
) -> States {
    compose(
        states,
        |state| {
            if state.is_absorbing() {
                return States::from([(state, Predicate::True)]);
            }
            let context = FormatContext {
                state,
                kind,
                path: path.clone(),
            };
            FORMATTED
                .with(|cache| cached(cache, context, || build_formatted(path, state, kind, memo)))
                .as_ref()
                .clone()
        },
        memo,
    )
}

#[tracing::instrument(skip_all)]
fn build_formatted(
    path: &ValuesPath,
    state: LexicalState,
    kind: GoType,
    memo: &PredicateMemo,
) -> States {
    let mut result = input(BTreeMap::from([(state, Predicate::True)]), path, memo);
    merge(
        &mut result,
        state,
        Predicate::Or(vec![
            Predicate::from(Guard::Absent { path: path.clone() }),
            Predicate::from(Guard::TypeIs {
                path: path.clone(),
                schema_type: "null".to_owned(),
            }),
        ]),
        memo,
    );
    for boolean in [false, true] {
        let guard = Predicate::from(Guard::Eq {
            path: path.clone(),
            value: helm_schema_core::GuardValue::Bool(boolean),
        });
        for (end, guard) in literal(BTreeMap::from([(state, guard)]), &boolean.to_string(), memo) {
            merge(&mut result, end, guard, memo);
        }
    }
    if let Some(end) = scalar_boundary(state) {
        merge(
            &mut result,
            end,
            Predicate::from(Guard::TypeIs {
                path: path.clone(),
                schema_type: "number".to_owned(),
            }),
            memo,
        );
    }
    for (domain, depth) in [
        (Domain::Map, 3),
        (Domain::SingletonMap, 0),
        (Domain::OrderedArray, 1),
    ] {
        let needed = match kind {
            GoType::Unknown => true,
            GoType::Map => domain != Domain::OrderedArray,
            GoType::Slice => domain == Domain::OrderedArray,
            GoType::Nil
            | GoType::Bool
            | GoType::String
            | GoType::Int
            | GoType::Int64
            | GoType::Uint64
            | GoType::Float64 => false,
        };
        if !needed {
            continue;
        }
        for (end, pattern) in patterns(state, domain, depth, &[""]).iter() {
            let mut guards = vec![Predicate::from(Guard::MatchesValue {
                path: path.clone(),
                pattern: pattern.clone(),
            })];
            if domain == Domain::SingletonMap {
                guards.push(Predicate::from(Guard::AtMostOneMember {
                    path: path.clone(),
                }));
            }
            merge(&mut result, *end, Predicate::all(guards), memo);
        }
    }
    result
}

// Scalar words preserve these boundaries; escape tails remain unknown.
fn scalar_boundary(state: LexicalState) -> Option<LexicalState> {
    match state {
        LexicalState::PlainStart
        | LexicalState::PlainDash
        | LexicalState::Plain
        | LexicalState::PlainColon => Some(LexicalState::Plain),
        LexicalState::FlowPlainStart
        | LexicalState::FlowPlainDash
        | LexicalState::FlowPlain
        | LexicalState::FlowPlainColon => Some(LexicalState::FlowPlain),
        // Numbers print as one word, which a flow entry continues or starts (`-1` included).
        LexicalState::FlowEmptyStart | LexicalState::FlowSequenceWord => {
            Some(LexicalState::FlowSequenceWord)
        }
        LexicalState::Double | LexicalState::FoldedDouble => Some(LexicalState::Double),
        LexicalState::Single => Some(LexicalState::Single),
        LexicalState::SingleEnd | LexicalState::Closed | LexicalState::Separated => {
            Some(LexicalState::Invalid)
        }
        LexicalState::ClosedCollection | LexicalState::FlowEnd => {
            Some(LexicalState::CollectionInvalid)
        }
        LexicalState::Invalid
        | LexicalState::BlockInvalid
        | LexicalState::CollectionInvalid
        | LexicalState::DirectiveInvalid => Some(state),
        // A negative number after two dashes may spell Helm's document separator.
        LexicalState::PlainDashes
        | LexicalState::FlowPlainDashes
        | LexicalState::Escape
        | LexicalState::Directive
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
        | LexicalState::Surrogate3
        | LexicalState::TruncatableStart
        | LexicalState::TruncatableWord => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Domain {
    Member,
    OrderedMember,
    Array,
    OrderedArray,
    Map,
    SingletonMap,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CollectionContext {
    state: LexicalState,
    domain: Domain,
    depth: usize,
    suffixes: Vec<String>,
}

type Patterns = BTreeMap<LexicalState, ValuePattern>;
thread_local! {
    static COLLECTIONS: Cache<CollectionContext, Patterns> = RefCell::default();
}

fn patterns(state: LexicalState, domain: Domain, depth: usize, suffixes: &[&str]) -> Rc<Patterns> {
    let context = CollectionContext {
        state,
        domain,
        depth,
        suffixes: suffixes.iter().map(|text| (*text).to_owned()).collect(),
    };
    COLLECTIONS.with(|cache| {
        cached(cache, context, || match domain {
            Domain::Member | Domain::OrderedMember => members(state, domain, depth, suffixes),
            Domain::Array | Domain::OrderedArray | Domain::Map | Domain::SingletonMap => {
                collection(state, domain, depth, suffixes)
            }
        })
    })
}

fn after(state: LexicalState, suffixes: &[&str]) -> Option<LexicalState> {
    let end = suffixes
        .first()?
        .chars()
        .try_fold(state, LexicalState::step)?;
    suffixes
        .iter()
        .all(|suffix| suffix.chars().try_fold(state, LexicalState::step) == Some(end))
        .then_some(end)
}

fn members(state: LexicalState, domain: Domain, depth: usize, suffixes: &[&str]) -> Patterns {
    let mut result: BTreeMap<_, Vec<_>> = languages(state, suffixes)
        .iter()
        .map(|(end, pattern)| {
            (
                *end,
                vec![ValuePattern::String {
                    pattern: pattern.clone(),
                }],
            )
        })
        .collect();
    if domain == Domain::Member
        && let Some(end) = scalar_boundary(state).and_then(|state| after(state, suffixes))
    {
        result.entry(end).or_default().push(ValuePattern::Scalar);
    }
    if depth > 0 {
        let nested = if domain == Domain::OrderedMember {
            &[Domain::OrderedArray, Domain::Map][..]
        } else {
            &[Domain::Array, Domain::Map][..]
        };
        for domain in nested {
            for (end, pattern) in patterns(state, *domain, depth - 1, suffixes).iter() {
                result.entry(*end).or_default().push(pattern.clone());
            }
        }
    }
    result
        .into_iter()
        .map(|(end, arms)| (end, ValuePattern::AnyOf(arms)))
        .collect()
}

#[tracing::instrument(skip_all)]
fn collection(start: LexicalState, domain: Domain, depth: usize, suffixes: &[&str]) -> Patterns {
    let mapping = match domain {
        Domain::Map | Domain::SingletonMap => true,
        Domain::Array | Domain::OrderedArray => false,
        Domain::Member | Domain::OrderedMember => return Patterns::new(),
    };
    let mut result = Patterns::new();
    if !mapping
        && start == LexicalState::PlainStart
        && let Some(end) = after(LexicalState::FlowEnd, suffixes)
    {
        // A printed array opens a flow sequence whose space-separated words read as entries:
        // words of this alphabet close it only at the final bracket, whatever their number.
        result.insert(
            end,
            ValuePattern::Array {
                prefix: Vec::new(),
                items: Some(Box::new(ValuePattern::String {
                    pattern: "^[A-Za-z0-9_./+-]*$".to_owned(),
                })),
            },
        );
    }
    let Some(state) = (if mapping { "map[" } else { "[" })
        .chars()
        .try_fold(start, LexicalState::step)
    else {
        return Patterns::new();
    };
    let shape = |member| {
        if mapping {
            object_pattern(ValuePattern::Any, member)
        } else {
            ValuePattern::Array {
                prefix: Vec::new(),
                items: Some(Box::new(member)),
            }
        }
    };
    if state.is_absorbing() {
        return BTreeMap::from([(state, shape(ValuePattern::Any))]);
    }
    let closing: Vec<_> = suffixes.iter().map(|suffix| format!("]{suffix}")).collect();
    let last: Vec<_> = closing.iter().map(String::as_str).collect();
    if !mapping && let Some(end) = after(state, &last) {
        result.entry(end).or_insert(ValuePattern::Array {
            prefix: Vec::new(),
            items: None,
        });
    }
    if !matches!(
        state,
        LexicalState::Single
            | LexicalState::Double
            | LexicalState::Plain
            | LexicalState::FlowEmptyStart
    ) {
        return result;
    }
    let Some(member_start) = (if mapping {
        state.step(':')
    } else {
        Some(state)
    }) else {
        return result;
    };
    // Singleton maps have no inter-member separator; other lengths need both boundaries.
    let mut boundaries = last.clone();
    if domain != Domain::SingletonMap {
        boundaries.push(" ");
    }
    let member_domain = if domain == Domain::OrderedArray {
        Domain::OrderedMember
    } else {
        Domain::Member
    };
    let member_depth = if domain == Domain::OrderedArray {
        0
    } else {
        depth
    };
    let members = patterns(member_start, member_domain, member_depth, &boundaries);
    let keys = mapping.then(|| languages(state, &[":"]));
    result.extend(collection_patterns(state, &members, keys.as_deref()));
    if domain == Domain::OrderedArray {
        return ordered_array(state, depth, &last, result);
    }
    result
}

// Ordered prefixes retain delimiter completion even without a homogeneous closure.
fn ordered_array(state: LexicalState, depth: usize, last: &[&str], result: Patterns) -> Patterns {
    let middle = patterns(state, Domain::OrderedMember, depth, &[" "]);
    let last = patterns(state, Domain::OrderedMember, depth, last);
    let mut prefix = Vec::new();
    let mut arms: BTreeMap<_, Vec<_>> = result
        .into_iter()
        .map(|(end, pattern)| (end, vec![pattern]))
        .collect();
    for _ in 0..4 {
        for (end, pattern) in last.iter() {
            let mut members = prefix.clone();
            members.push(pattern.clone());
            arms.entry(*end).or_default().push(ValuePattern::Array {
                prefix: members,
                items: None,
            });
        }
        for (end, pattern) in middle.iter().filter(|(end, _)| end.is_absorbing()) {
            let mut members = prefix.clone();
            members.extend([pattern.clone(), ValuePattern::Any]);
            arms.entry(*end).or_default().push(ValuePattern::Array {
                prefix: members,
                items: Some(Box::new(ValuePattern::Any)),
            });
        }
        match middle.get(&state) {
            Some(pattern) => prefix.push(pattern.clone()),
            None => break,
        }
    }
    arms.into_iter()
        .map(|(end, patterns)| (end, ValuePattern::AnyOf(patterns)))
        .collect()
}

pub(super) fn merge(
    states: &mut States,
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
fn preimage_language(
    start: LexicalState,
    end: LexicalState,
    suffixes: &[&str],
    transforms: &[TextTransform],
) -> Option<String> {
    let states = lexical_states(start);
    let mut edges = BTreeMap::<(usize, usize), String>::new();
    for (from, state) in states.iter().enumerate() {
        let mut targets = BTreeMap::<LexicalState, Vec<char>>::new();
        for character in alphabet() {
            let next = if transforms.is_empty() {
                state.step(character)
            } else {
                mapped_character(character, transforms)
                    .and_then(|text| text.chars().try_fold(*state, LexicalState::step))
            };
            if let Some(next) = next {
                targets.entry(next).or_default().push(character);
            }
        }
        for (next, characters) in targets {
            let to = states.binary_search(&next).ok()?;
            edges.insert(
                (from, to),
                helm_schema_core::regex_character_class(characters)?,
            );
        }
    }
    let entry = states.len();
    let exit = entry + 1;
    edges.insert(
        (entry, states.iter().position(|state| *state == start)?),
        String::new(),
    );
    if suffixes.is_empty() {
        return None;
    }
    for (index, state) in states.iter().enumerate() {
        if suffixes
            .iter()
            .all(|suffix| suffix.chars().try_fold(*state, LexicalState::step) == Some(end))
        {
            edges.insert((index, exit), String::new());
        }
    }
    helm_schema_core::RegularLanguage {
        transitions: edges
            .into_iter()
            .map(|((from, to), pattern)| helm_schema_core::RegularTransition { from, to, pattern })
            .collect(),
        start: entry,
        end: exit,
        states: states.len(),
    }
    .eliminate()
    .map(|text| format!("^(?:{text})$"))
}

pub(super) fn serialized_input(
    states: States,
    value: &super::FrozenValue,
    memo: &PredicateMemo,
) -> States {
    let mut result = BTreeMap::new();
    let Some(path) = value.identity() else {
        return result;
    };
    let typed = |condition: &Predicate, schema_type: &str| {
        Predicate::all(vec![
            condition.clone(),
            Predicate::from(Guard::TypeIs {
                path: path.clone(),
                schema_type: schema_type.to_owned(),
            }),
        ])
    };
    for (state, condition) in states {
        // Boolean and null spellings agree across the pinned JSON/YAML serializers.
        for (schema_type, spelling) in [("boolean", "true"), ("null", "null")] {
            let guard = typed(&condition, schema_type);
            for (end, guard) in literal(BTreeMap::from([(state, guard)]), spelling, memo) {
                merge(&mut result, end, guard, memo);
            }
        }
        // A serialized number may be negative, so it crosses the same word boundary as a
        // directly formatted one.
        if let Some(end) = scalar_boundary(state) {
            merge(&mut result, end, typed(&condition, "number"), memo);
        }
    }
    result
}

// These edits are character homomorphisms on the admitted alphabet. Unsupported edits
// withdraw the preimage; they never reuse the untransformed input language.
pub(super) fn mapped_character(character: char, transforms: &[TextTransform]) -> Option<String> {
    let mut text = character.to_string();
    for transform in transforms {
        if !transform.maps_characters() {
            return None;
        }
        text = transform.apply(&text, 4096)?;
    }
    Some(text)
}

/// The end state of a whole string drawn from a full-match language, printed after character
/// edits. It is certified only when every word reaches the same end; any other edit certifies
/// nothing (a whole-text edit that keeps every word was already elided where it ran).
pub(super) fn domain(start: LexicalState, pattern: &str, transforms: &[TextTransform]) -> States {
    if start.is_absorbing() {
        return States::from([(start, Predicate::True)]);
    }
    if !transforms.iter().all(TextTransform::maps_characters) {
        return States::new();
    }
    let ends = helm_schema_core::pattern_end_states(
        pattern,
        start,
        |state, character| {
            mapped_character(character, transforms)?
                .chars()
                .try_fold(*state, LexicalState::step)
        },
        4096,
    );
    let Some(ends) = ends else {
        return States::new();
    };
    match Vec::from_iter(ends).as_slice() {
        [Some(end)] => States::from([(*end, Predicate::True)]),
        _ => States::new(),
    }
}

pub(super) fn transformed_input(
    start: LexicalState,
    path: &ValuesPath,
    transforms: &[TextTransform],
    memo: &PredicateMemo,
) -> States {
    if start.is_absorbing() {
        return States::from([(start, Predicate::True)]);
    }
    if mapped_character('a', transforms).is_none() {
        return BTreeMap::new();
    }
    let patterns = mapped_languages(start, &[""], transforms);
    let mut result = BTreeMap::new();
    for (end, pattern) in patterns.iter() {
        merge(
            &mut result,
            *end,
            Predicate::from(Guard::MatchesPattern {
                path: path.clone(),
                pattern: pattern.clone(),
                templated: false,
            }),
            memo,
        );
    }
    result
}

fn collection_patterns(
    state: LexicalState,
    members: &BTreeMap<LexicalState, ValuePattern>,
    keys: Option<&Languages>,
) -> BTreeMap<LexicalState, ValuePattern> {
    let Some(good) = members.get(&state) else {
        return BTreeMap::new();
    };
    let key_boundary = state.step(':');
    let key_good = keys
        .and_then(|keys| key_boundary.and_then(|boundary| keys.get(&boundary)))
        .map(|pattern| ValuePattern::String {
            pattern: pattern.clone(),
        });
    let all = |values, key| {
        if keys.is_some() {
            object_pattern(key, values)
        } else {
            ValuePattern::Array {
                prefix: Vec::new(),
                items: Some(Box::new(values)),
            }
        }
    };
    let good_collection = all(
        good.clone(),
        key_good
            .clone()
            .unwrap_or_else(|| ValuePattern::AnyOf(Vec::new())),
    );
    let mut result = BTreeMap::from([(state, good_collection.clone())]);
    for end in STATES.iter().copied().filter(|end| end.is_absorbing()) {
        let bad = members
            .get(&end)
            .cloned()
            .unwrap_or(ValuePattern::AnyOf(Vec::new()));
        let allowed = ValuePattern::AnyOf(vec![good.clone(), bad]);
        let allowed_key = match keys {
            Some(keys) => ValuePattern::AnyOf(
                [
                    key_boundary.and_then(|boundary| keys.get(&boundary)),
                    keys.get(&end),
                ]
                .into_iter()
                .flatten()
                .map(|pattern| ValuePattern::String {
                    pattern: pattern.clone(),
                })
                .collect(),
            ),
            None => ValuePattern::Any,
        };
        result.insert(
            end,
            ValuePattern::AllOf(vec![
                all(allowed, allowed_key),
                ValuePattern::Not(Box::new(good_collection.clone())),
            ]),
        );
    }
    result
}

fn object_pattern(keys: ValuePattern, values: ValuePattern) -> ValuePattern {
    ValuePattern::Object {
        keys: Box::new(keys),
        values: Box::new(values),
    }
}

pub(super) fn mapped_text(text: &str, transforms: &[TextTransform]) -> Option<String> {
    let mut output = String::new();
    for character in text.chars() {
        let mapped = mapped_character(character, transforms)?;
        if output.len().saturating_add(mapped.len()) > 4096 {
            return None;
        }
        output.push_str(&mapped);
    }
    Some(output)
}

#[cfg(test)]
#[path = "../tests/runtime_quoted_preimages.rs"]
mod tests;

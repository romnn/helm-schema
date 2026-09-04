use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A structural path beneath Helm's `.Values` root.
///
/// Segments are the semantic representation. Encoding is explicit because
/// dots and backslashes in literal keys must not become selector boundaries.
#[derive(Clone, Debug, Default, Eq)]
pub struct ValuesPath {
    segments: Vec<Segment>,
    encoded: Box<str>,
}

/// One structural component of a [`ValuesPath`].
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Segment {
    /// A chart-authored mapping key, including a literal `*` key.
    Literal(String),
    /// Every member reached by a Helm `range`.
    EachMember,
}

impl PartialOrd for Segment {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Segment {
    fn cmp(&self, other: &Self) -> Ordering {
        EncodedSegmentBytes::new(self).cmp(EncodedSegmentBytes::new(other))
    }
}

impl Segment {
    /// Returns the chart-authored key, or `None` for a member wildcard.
    #[must_use]
    pub fn literal(&self) -> Option<&str> {
        match self {
            Self::Literal(value) => Some(value),
            Self::EachMember => None,
        }
    }

    /// Reports whether this segment selects every ranged member.
    #[must_use]
    pub const fn is_each_member(&self) -> bool {
        matches!(self, Self::EachMember)
    }

    /// Encodes this segment for legacy string-based phase boundaries.
    #[must_use]
    pub fn encode_component(&self) -> String {
        match self {
            Self::Literal(value) if value == "*" => r"\*".to_string(),
            Self::Literal(value) => value.replace('\\', r"\\"),
            Self::EachMember => "*".to_string(),
        }
    }

    /// Decodes one component emitted by [`Self::encode_component`].
    #[must_use]
    pub fn from_encoded_component(encoded: &str) -> Self {
        if encoded == "*" {
            return Self::EachMember;
        }
        let mut literal = String::new();
        let mut characters = encoded.chars().peekable();
        while let Some(character) = characters.next() {
            if character == '\\'
                && characters
                    .peek()
                    .is_some_and(|next| matches!(next, '\\' | '*'))
            {
                if let Some(escaped) = characters.next() {
                    literal.push(escaped);
                }
            } else {
                literal.push(character);
            }
        }
        Self::Literal(literal)
    }
}

impl From<String> for Segment {
    fn from(value: String) -> Self {
        Self::Literal(value)
    }
}

impl From<&str> for Segment {
    fn from(value: &str) -> Self {
        Self::Literal(value.to_string())
    }
}

impl From<&Segment> for Segment {
    fn from(value: &Segment) -> Self {
        value.clone()
    }
}

impl ValuesPath {
    /// Parses the legacy escaped-dot path spelling into structural segments.
    #[must_use]
    pub fn parse(path: &str) -> Self {
        let mut segments = Vec::new();
        let mut segment = String::new();
        let mut escaped_star = false;
        let mut characters = path.chars().peekable();
        while let Some(character) = characters.next() {
            match character {
                '.' => {
                    push_parsed_segment(&mut segments, &mut segment, &mut escaped_star);
                }
                '\\' if characters
                    .peek()
                    .is_some_and(|next| matches!(next, '.' | '\\' | '*')) =>
                {
                    if let Some(escaped) = characters.next() {
                        escaped_star |= escaped == '*';
                        segment.push(escaped);
                    }
                }
                _ => segment.push(character),
            }
        }
        push_parsed_segment(&mut segments, &mut segment, &mut escaped_star);
        let encoded = encode_segments(&segments).into_boxed_str();
        Self { segments, encoded }
    }

    /// Constructs a path from literal structural segments.
    #[must_use]
    pub fn from_segments<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<Segment>,
    {
        let segments = segments
            .into_iter()
            .map(Into::into)
            .filter(|segment| !matches!(segment, Segment::Literal(value) if value.is_empty()))
            .collect::<Vec<_>>();
        let encoded = encode_segments(&segments).into_boxed_str();
        Self { segments, encoded }
    }

    /// Iterates structural segments from root to leaf.
    #[must_use]
    pub fn segments(&self) -> impl DoubleEndedIterator<Item = &Segment> + ExactSizeIterator {
        self.segments.iter()
    }

    /// Encodes the path in the stable escaped-dot wire spelling.
    #[must_use]
    pub fn encode(&self) -> String {
        self.encoded.to_string()
    }

    /// Returns the strict structural parent, if this path is non-root.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        let (_, parent) = self.segments.split_last()?;
        let encoded = encode_segments(parent).into_boxed_str();
        Some(Self {
            segments: parent.to_vec(),
            encoded,
        })
    }

    /// Appends one literal segment. Empty segments preserve legacy no-op behavior.
    pub fn push(&mut self, segment: impl Into<String>) {
        let segment = segment.into();
        if !segment.is_empty() {
            self.segments.push(Segment::Literal(segment));
            self.encoded = encode_segments(&self.segments).into_boxed_str();
        }
    }

    /// Appends the structural marker for every ranged member.
    pub fn push_each_member(&mut self) {
        self.segments.push(Segment::EachMember);
        self.encoded = encode_segments(&self.segments).into_boxed_str();
    }

    /// Reports whether this path is a strict descendant of `ancestor`.
    #[must_use]
    pub fn is_descendant_of(&self, ancestor: &Self) -> bool {
        self.segments.len() > ancestor.segments.len()
            && self.segments.starts_with(&ancestor.segments)
    }

    /// Returns the collection path for a trailing member segment.
    #[must_use]
    pub fn item_parent(&self) -> Option<Self> {
        (self.segments.last() == Some(&Segment::EachMember))
            .then(|| self.parent())
            .flatten()
    }
}

fn push_parsed_segment(segments: &mut Vec<Segment>, segment: &mut String, escaped_star: &mut bool) {
    if segment.is_empty() {
        *escaped_star = false;
        return;
    }
    let segment = std::mem::take(segment);
    if segment == "*" && !*escaped_star {
        segments.push(Segment::EachMember);
    } else {
        segments.push(Segment::Literal(segment));
    }
    *escaped_star = false;
}

fn encode_segments(segments: &[Segment]) -> String {
    let mut encoded = String::new();
    for (index, segment) in segments.iter().enumerate() {
        if index != 0 {
            encoded.push('.');
        }
        match segment {
            Segment::EachMember => encoded.push('*'),
            Segment::Literal(value) if value == "*" => encoded.push_str(r"\*"),
            Segment::Literal(value) => {
                for character in value.chars() {
                    if matches!(character, '.' | '\\') {
                        encoded.push('\\');
                    }
                    encoded.push(character);
                }
            }
        }
    }
    encoded
}

impl PartialEq for ValuesPath {
    fn eq(&self, other: &Self) -> bool {
        self.segments == other.segments
    }
}

impl PartialOrd for ValuesPath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ValuesPath {
    fn cmp(&self, other: &Self) -> Ordering {
        self.encoded.cmp(&other.encoded)
    }
}

enum EncodedSegmentBytes<'a> {
    EachMember(bool),
    LiteralStar(u8),
    Literal {
        bytes: &'a [u8],
        index: usize,
        escape_pending: bool,
    },
}

impl<'a> EncodedSegmentBytes<'a> {
    fn new(segment: &'a Segment) -> Self {
        match segment {
            Segment::EachMember => Self::EachMember(false),
            Segment::Literal(value) if value == "*" => Self::LiteralStar(0),
            Segment::Literal(value) => Self::Literal {
                bytes: value.as_bytes(),
                index: 0,
                escape_pending: false,
            },
        }
    }
}

impl Iterator for EncodedSegmentBytes<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::EachMember(emitted) => {
                if *emitted {
                    None
                } else {
                    *emitted = true;
                    Some(b'*')
                }
            }
            Self::LiteralStar(index) => {
                let byte = b"\\*".get(usize::from(*index)).copied();
                *index += u8::from(byte.is_some());
                byte
            }
            Self::Literal {
                bytes,
                index,
                escape_pending,
            } => {
                let byte = *bytes.get(*index)?;
                if *escape_pending {
                    *escape_pending = false;
                    *index += 1;
                    return Some(byte);
                }
                if byte == b'\\' {
                    *escape_pending = true;
                    Some(b'\\')
                } else {
                    *index += 1;
                    Some(byte)
                }
            }
        }
    }
}

impl Hash for ValuesPath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.segments.hash(state);
    }
}

impl Serialize for ValuesPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.encode())
    }
}

impl<'de> Deserialize<'de> for ValuesPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|path| Self::parse(&path))
    }
}

/// Joins structural `.Values` path segments into the contract path currency.
///
/// Dots and backslashes inside a segment are escaped so literal YAML keys
/// remain distinct from selector boundaries.
#[must_use]
pub fn join_value_path<I, S>(segments: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    ValuesPath::from_segments(
        segments
            .into_iter()
            .map(|segment| segment.as_ref().to_string()),
    )
    .encode()
}

/// Joins components previously emitted by [`Segment::encode_component`].
#[must_use]
pub fn join_encoded_value_path<I, S>(segments: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    ValuesPath::from_segments(
        segments
            .into_iter()
            .map(|segment| Segment::from_encoded_component(segment.as_ref())),
    )
    .encode()
}

/// Splits the contract `.Values` path currency into structural segments.
#[must_use]
pub fn split_value_path(path: &str) -> Vec<String> {
    ValuesPath::parse(path)
        .segments
        .into_iter()
        .map(|segment| match segment {
            Segment::Literal(value) => value,
            Segment::EachMember => "*".to_string(),
        })
        .collect()
}

/// Appends one structural segment to an encoded `.Values` path.
#[must_use]
pub fn append_value_path(path: &str, segment: &str) -> String {
    let mut path = ValuesPath::parse(path);
    path.push(segment);
    path.encode()
}

/// Appends one ranged-member segment to an encoded `.Values` path.
#[must_use]
pub fn append_each_member_value_path(path: &str) -> String {
    let mut path = ValuesPath::parse(path);
    path.push_each_member();
    path.encode()
}

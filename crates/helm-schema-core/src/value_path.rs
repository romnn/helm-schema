use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A structural path beneath Helm's `.Values` root.
///
/// Segments are the semantic representation. Encoding is explicit because
/// dots and backslashes in literal keys must not become selector boundaries.
#[derive(Clone, Debug, Default, Eq)]
pub struct ValuesPath {
    segments: Vec<String>,
}

impl ValuesPath {
    /// Parses the legacy escaped-dot path spelling into structural segments.
    #[must_use]
    pub fn parse(path: &str) -> Self {
        let mut segments = Vec::new();
        let mut segment = String::new();
        let mut characters = path.chars().peekable();
        while let Some(character) = characters.next() {
            match character {
                '.' => {
                    if !segment.is_empty() {
                        segments.push(std::mem::take(&mut segment));
                    }
                }
                '\\' if characters
                    .peek()
                    .is_some_and(|next| matches!(next, '.' | '\\')) =>
                {
                    if let Some(escaped) = characters.next() {
                        segment.push(escaped);
                    }
                }
                _ => segment.push(character),
            }
        }
        if !segment.is_empty() {
            segments.push(segment);
        }
        Self { segments }
    }

    /// Constructs a path from literal structural segments.
    #[must_use]
    pub fn from_segments<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            segments: segments
                .into_iter()
                .map(Into::into)
                .filter(|segment: &String| !segment.is_empty())
                .collect(),
        }
    }

    /// Iterates literal structural segments from root to leaf.
    pub fn segments(&self) -> impl DoubleEndedIterator<Item = &str> + ExactSizeIterator {
        self.segments.iter().map(String::as_str)
    }

    /// Encodes the path in the stable escaped-dot wire spelling.
    #[must_use]
    pub fn encode(&self) -> String {
        self.encoded_chars().collect()
    }

    /// Returns the strict structural parent, if this path is non-root.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        let (_, parent) = self.segments.split_last()?;
        Some(Self {
            segments: parent.to_vec(),
        })
    }

    /// Appends one literal segment. Empty segments preserve legacy no-op behavior.
    pub fn push(&mut self, segment: impl Into<String>) {
        let segment = segment.into();
        if !segment.is_empty() {
            self.segments.push(segment);
        }
    }

    /// Reports whether this path is a strict descendant of `ancestor`.
    #[must_use]
    pub fn is_descendant_of(&self, ancestor: &Self) -> bool {
        self.segments.len() > ancestor.segments.len()
            && self.segments.starts_with(&ancestor.segments)
    }

    /// Returns the collection path for a trailing legacy `*` member segment.
    #[must_use]
    pub fn item_parent(&self) -> Option<Self> {
        (self.segments.last().is_some_and(|segment| segment == "*"))
            .then(|| self.parent())
            .flatten()
    }

    fn encoded_chars(&self) -> EncodedPathChars<'_> {
        EncodedPathChars::new(&self.segments)
    }
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
        self.encoded_chars().cmp(other.encoded_chars())
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

struct EncodedPathChars<'a> {
    segments: std::slice::Iter<'a, String>,
    characters: Option<std::str::Chars<'a>>,
    emitted_segment: bool,
    escaped: Option<char>,
}

impl<'a> EncodedPathChars<'a> {
    fn new(segments: &'a [String]) -> Self {
        Self {
            segments: segments.iter(),
            characters: None,
            emitted_segment: false,
            escaped: None,
        }
    }
}

impl Iterator for EncodedPathChars<'_> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(escaped) = self.escaped.take() {
                return Some(escaped);
            }
            if let Some(character) = self.characters.as_mut().and_then(Iterator::next) {
                if matches!(character, '.' | '\\') {
                    self.escaped = Some(character);
                    return Some('\\');
                }
                return Some(character);
            }
            let segment = self.segments.next()?;
            self.characters = Some(segment.chars());
            if self.emitted_segment {
                return Some('.');
            }
            self.emitted_segment = true;
        }
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

/// Splits the contract `.Values` path currency into structural segments.
#[must_use]
pub fn split_value_path(path: &str) -> Vec<String> {
    ValuesPath::parse(path).segments
}

/// Appends one structural segment to an encoded `.Values` path.
#[must_use]
pub fn append_value_path(path: &str, segment: &str) -> String {
    let mut path = ValuesPath::parse(path);
    path.push(segment);
    path.encode()
}

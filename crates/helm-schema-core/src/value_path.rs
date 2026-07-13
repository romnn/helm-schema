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
    let mut path = String::new();
    for segment in segments {
        let segment = segment.as_ref();
        if segment.is_empty() {
            continue;
        }
        if !path.is_empty() {
            path.push('.');
        }
        for character in segment.chars() {
            if matches!(character, '.' | '\\') {
                path.push('\\');
            }
            path.push(character);
        }
    }
    path
}

/// Splits the contract `.Values` path currency into structural segments.
#[must_use]
pub fn split_value_path(path: &str) -> Vec<String> {
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
    segments
}

/// Appends one structural segment to an encoded `.Values` path.
#[must_use]
pub fn append_value_path(path: &str, segment: &str) -> String {
    let mut segments = split_value_path(path);
    segments.push(segment.to_string());
    join_value_path(segments)
}

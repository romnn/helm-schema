//! Known prefixes preserved by bounded byte edits.

use super::relationship::TextTransform;

#[derive(Clone)]
pub(super) struct Prefix {
    pub(super) text: String,
    pub(super) complete: bool,
}

impl Prefix {
    pub(super) fn literal(text: &str) -> Option<Self> {
        (text.len() <= 4096).then(|| Self {
            text: text.to_owned(),
            complete: true,
        })
    }

    pub(super) fn concatenate(mut self, next: Option<Self>) -> Self {
        if self.complete {
            if let Some(next) = next
                && self.text.len().saturating_add(next.text.len()) <= 4096
            {
                self.text.push_str(&next.text);
                self.complete = next.complete;
            } else {
                self.complete = false;
            }
        }
        self
    }

    pub(super) fn strip_marker(mut self) -> Self {
        self.text = self.text.replace("<no value>", "");
        if !self.complete {
            self.text
                .truncate(self.text.len() - pending_suffix(&self.text, "<no value>"));
        }
        self
    }

    pub(super) fn trim(mut self) -> Self {
        self.text = self.text.trim().to_owned();
        self
    }

    fn unknown() -> Self {
        Self {
            text: String::new(),
            complete: false,
        }
    }

    pub(super) fn edited(mut self, transform: &TextTransform) -> Self {
        if !self.complete {
            match transform {
                TextTransform::Truncate(length) if *length < 0 => return Self::unknown(),
                TextTransform::Truncate(length) => {
                    let Ok(width) = usize::try_from(length.unsigned_abs()) else {
                        return Self::unknown();
                    };
                    self.complete = self.text.len() >= width;
                }
                TextTransform::TrimPrefix(affix)
                    if affix.starts_with(&self.text) && !self.text.starts_with(affix) =>
                {
                    return Self::unknown();
                }
                TextTransform::TrimSuffix(affix) => {
                    let pending = pending_suffix(&self.text, affix);
                    self.text.truncate(self.text.len() - pending);
                    return self;
                }
                TextTransform::Replace { from, to: _ } if !from.is_empty() => {
                    let pending = pending_suffix(&self.text, from);
                    self.text.truncate(self.text.len() - pending);
                }
                TextTransform::Cut(separator) => {
                    if let Some(end) = self.text.find(separator.as_str()) {
                        self.text.truncate(end);
                        self.complete = true;
                    } else {
                        let pending = pending_suffix(&self.text, separator);
                        self.text.truncate(self.text.len() - pending);
                    }
                    return self;
                }
                TextTransform::RegexReplace {
                    pattern: _,
                    replacement: _,
                } => return Self::unknown(),
                TextTransform::Upper
                | TextTransform::Lower
                | TextTransform::TrimPrefix(_)
                | TextTransform::Replace { from: _, to: _ } => {}
            }
        }
        let Some(text) = transform.apply(&self.text, 4096) else {
            return Self::unknown();
        };
        self.text = text;
        self
    }
}

fn pending_suffix(text: &str, pattern: &str) -> usize {
    (0..=pattern.len().min(text.len()))
        .rev()
        .find(|length| {
            pattern
                .get(..*length)
                .is_some_and(|prefix| text.ends_with(prefix))
        })
        .unwrap_or(0)
}

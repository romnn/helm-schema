//! Concrete text stages used by Helm and Sprig before YAML decoding.

mod quote_print;

/// Recognizes the Unicode whitespace predicate used by Go's `strings.TrimSpace`.
///
/// Ports Go `unicode/graphic.go:128` (`IsSpace`).
#[must_use]
pub fn is_space(ch: char) -> bool {
    matches!(ch, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{0085}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

fn separator_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{000c}')
}

/// Adds the initial pad and another pad after each LF, preserving bare CRs.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Indent {
    pad: String,
    nindent: bool,
    started: bool,
}

impl Indent {
    /// Creates Sprig's `indent` or `nindent` stage (`strings.go:109-116`).
    #[must_use]
    pub fn new(width: usize, nindent: bool) -> Self {
        Self {
            pad: " ".repeat(width),
            nindent,
            started: false,
        }
    }

    /// Emits one character and the padding required by `strings.go:109-116`.
    #[must_use]
    pub fn push(&mut self, ch: char) -> String {
        let mut output = self.start();
        output.push(ch);
        if ch == '\n' {
            output.push_str(&self.pad);
        }
        output
    }

    /// Emits the pad even when the input is empty (`strings.go:111`).
    #[must_use]
    pub fn finish(&mut self) -> String {
        self.start()
    }

    fn start(&mut self) -> String {
        if self.started {
            return String::new();
        }
        self.started = true;
        let mut output = String::new();
        if self.nindent {
            output.push('\n');
        }
        output.push_str(&self.pad);
        output
    }
}

/// Holds trailing Unicode whitespace until a subsequent non-space arrives.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Trim {
    started: bool,
    pending: String,
}

impl Trim {
    /// Applies `strings.TrimSpace` incrementally (Sprig `functions.go:122`).
    #[must_use]
    pub fn push(&mut self, ch: char) -> String {
        if is_space(ch) {
            if self.started {
                self.pending.push(ch);
            }
            return String::new();
        }
        self.started = true;
        let mut output = std::mem::take(&mut self.pending);
        output.push(ch);
        output
    }

    /// Drops the final whitespace run, as Go `strings/strings.go:1087` does.
    #[must_use]
    pub fn finish(&mut self) -> String {
        self.pending.clear();
        String::new()
    }
}

/// Escapes one UTF-8 character at a time using Go's `%q` string formatting.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Quote {
    started: bool,
    finished: bool,
}

impl Quote {
    /// Ports Go `strconv/quote.go:31-51,64-123`, used by Sprig `strings.go:83-91`.
    #[must_use]
    pub fn push(&mut self, ch: char) -> String {
        let mut output = self.start();
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{0007}' => output.push_str("\\a"),
            '\u{0008}' => output.push_str("\\b"),
            '\u{000c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{000b}' => output.push_str("\\v"),
            ch if quote_print::is_print(ch) => output.push(ch),
            ch => append_escape(&mut output, ch),
        }
        output
    }

    /// Closes the quoted string, including an empty string (`strconv/quote.go:50`).
    #[must_use]
    pub fn finish(&mut self) -> String {
        if self.finished {
            return String::new();
        }
        self.finished = true;
        let mut output = self.start();
        output.push('"');
        output
    }

    fn start(&mut self) -> String {
        if self.started {
            return String::new();
        }
        self.started = true;
        String::from("\"")
    }
}

/// Ports Go `strconv/quote.go:100-122` without a fallible formatting sink.
fn append_escape(output: &mut String, ch: char) {
    let value = u32::from(ch);
    let digits = if value < 0x20 || value == 0x7f {
        output.push_str("\\x");
        2
    } else if value < 0x10000 {
        output.push_str("\\u");
        4
    } else {
        output.push_str("\\U");
        8
    };
    for shift in (0..digits).rev() {
        let digit = (value >> (shift * 4)) & 15;
        if let Some(hex) = char::from_digit(digit, 16) {
            output.push(hex);
        }
    }
}

/// Removes the literal `<no value>` sequence, preserving partial matches.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct StripNoValue {
    pending: String,
}

impl StripNoValue {
    /// Ports Helm `pkg/engine/engine.go:193,312` (`strings.ReplaceAll`).
    #[must_use]
    pub fn push(&mut self, ch: char) -> String {
        const NEEDLE: &str = "<no value>";
        self.pending.push(ch);
        if self.pending == NEEDLE {
            self.pending.clear();
            return String::new();
        }
        let mut output = String::new();
        while !NEEDLE.starts_with(&self.pending) {
            output.push(self.pending.remove(0));
        }
        output
    }

    /// Emits an unmatched suffix (`strings/strings.go:1131`).
    #[must_use]
    pub fn finish(&mut self) -> String {
        std::mem::take(&mut self.pending)
    }
}

/// Output from Helm's splitter, with boundaries only for nonblank chunks.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SplitEvent {
    /// Bytes belonging to the current chunk after its leading Unicode trim.
    Text(String),
    /// Completes one nonblank chunk.
    End,
}

/// Applies Helm's separator regex while retaining only its undecidable suffix.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Splitter {
    file_started: bool,
    chunk_started: bool,
    after_separator: bool,
    whitespace: String,
    dashes: u8,
}

impl Splitter {
    /// Ports Helm `pkg/release/v1/util/manifest.go:37,57-66`.
    #[must_use]
    pub fn push(&mut self, ch: char) -> Vec<SplitEvent> {
        let mut events = Vec::new();
        if !self.file_started {
            if is_space(ch) {
                return events;
            }
            self.file_started = true;
            if ch == '-' {
                self.dashes = 1;
                return events;
            }
        }
        if self.after_separator {
            if separator_space(ch) {
                return events;
            }
            self.after_separator = false;
        }
        if self.dashes > 0 {
            if ch == '-' {
                self.dashes += 1;
                if self.dashes == 3 {
                    self.dashes = 0;
                    self.whitespace.clear();
                    self.after_separator = true;
                    if self.chunk_started {
                        events.push(SplitEvent::End);
                    }
                    self.chunk_started = false;
                }
                return events;
            }
            let mut held = std::mem::take(&mut self.whitespace);
            held.push_str(&"-".repeat(usize::from(self.dashes)));
            self.dashes = 0;
            self.emit(&held, &mut events);
        }
        if ch == '-' && self.whitespace.ends_with('\n') {
            self.dashes = 1;
        } else if separator_space(ch) {
            self.whitespace.push(ch);
        } else {
            let mut held = std::mem::take(&mut self.whitespace);
            held.push(ch);
            self.emit(&held, &mut events);
        }
        events
    }

    /// Completes Helm's final chunk (`pkg/release/v1/util/manifest.go:60-66`).
    #[must_use]
    pub fn finish(&mut self) -> Vec<SplitEvent> {
        let mut events = Vec::new();
        let mut held = std::mem::take(&mut self.whitespace);
        held.push_str(&"-".repeat(usize::from(self.dashes)));
        self.dashes = 0;
        self.emit(&held, &mut events);
        if self.chunk_started {
            events.push(SplitEvent::End);
            self.chunk_started = false;
        }
        events
    }

    fn emit(&mut self, text: &str, events: &mut Vec<SplitEvent>) {
        let text = if self.chunk_started {
            text
        } else {
            text.trim_start_matches(is_space)
        };
        if !text.is_empty() {
            self.chunk_started = true;
            events.push(SplitEvent::Text(text.to_owned()));
        }
    }
}

/// Splits and trims manifests as Helm `pkg/release/v1/util/manifest.go:37-70` does.
#[must_use]
pub fn split_manifests(text: &str) -> Vec<String> {
    let mut splitter = Splitter::default();
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        for event in splitter.push(ch) {
            match event {
                SplitEvent::Text(text) => current.push_str(&text),
                SplitEvent::End => chunks.push(std::mem::take(&mut current)),
            }
        }
    }
    for event in splitter.finish() {
        match event {
            SplitEvent::Text(text) => current.push_str(&text),
            SplitEvent::End => chunks.push(std::mem::take(&mut current)),
        }
    }
    chunks
}

/// Applies Sprig `strings.go:109-112` to a complete string.
#[must_use]
pub fn indent(width: usize, text: &str) -> String {
    let mut stage = Indent::new(width, false);
    let mut output = text.chars().map(|ch| stage.push(ch)).collect::<String>();
    output.push_str(&stage.finish());
    output
}

/// Applies Sprig `strings.go:114-116` to a complete string.
#[must_use]
pub fn nindent(width: usize, text: &str) -> String {
    let mut stage = Indent::new(width, true);
    let mut output = text.chars().map(|ch| stage.push(ch)).collect::<String>();
    output.push_str(&stage.finish());
    output
}

/// Applies Go `strings.TrimSpace`, registered by Sprig `functions.go:122`.
#[must_use]
pub fn trim(text: &str) -> String {
    let mut stage = Trim::default();
    let mut output = text.chars().map(|ch| stage.push(ch)).collect::<String>();
    output.push_str(&stage.finish());
    output
}

/// Applies Sprig `strings.go:83-91` to one string argument.
#[must_use]
pub fn quote(text: &str) -> String {
    let mut stage = Quote::default();
    let mut output = text.chars().map(|ch| stage.push(ch)).collect::<String>();
    output.push_str(&stage.finish());
    output
}

/// Applies Helm's `<no value>` strip (`pkg/engine/engine.go:193,312`).
#[must_use]
pub fn strip_no_value(text: &str) -> String {
    let mut stage = StripNoValue::default();
    let mut output = text.chars().map(|ch| stage.push(ch)).collect::<String>();
    output.push_str(&stage.finish());
    output
}

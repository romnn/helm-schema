//! The pinned reader, scanner and parser composed in Go's pull order.

use crate::parser::{Parser, ParserControl};
use crate::reader::ByteReader;
use crate::scanner::{Scanner, ScannerControl};
use crate::types::{Error, ErrorKind, Event, EventKind, Mark, Token};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Outcome {
    Done,
    Failed(Error),
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
enum OutcomeControl {
    #[default]
    Active,
    PendingDone,
    PendingFailure(ErrorKind),
    Done,
    Failed(ErrorKind),
}

/// Go's raw-cache validation stage, without a text buffer (`readerc.go:56-88`).
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
struct Validator {
    phase: usize,
    pending: Option<Outcome>,
    invalid: Option<Error>,
    committed: Option<Outcome>,
}

impl Validator {
    fn result(&self) -> Result<(), Error> {
        if let Some(Outcome::Failed(err)) = &self.committed {
            Err(err.clone())
        } else {
            Ok(())
        }
    }

    fn commit(&mut self) -> Result<(), Error> {
        if let Some(err) = self.invalid.take() {
            self.committed = Some(Outcome::Failed(err));
        } else if let Some(outcome) = self.pending.take() {
            self.committed = Some(outcome);
        }
        self.result()
    }

    fn active(&self) -> bool {
        self.pending.is_none() && self.invalid.is_none() && self.committed.is_none()
    }
}

/// One unit of syntax a [`Reader`] delivers, in the order the parser consumes it.
///
/// Every token reaches the observer before the events its consumption produces.
#[derive(Clone, Copy, Debug)]
pub enum Syntax<'a> {
    /// A scanner token (`yamlh.go:183-229`).
    Token(&'a Token),
    /// A parser event (`yamlh.go:283-386`).
    Event(&'a Event),
}

/// A cloneable suspended reader of the first YAML document.
///
/// Characters reach the scanner immediately.
/// Its events are provisional until their raw window has been validated: a forbidden
/// character later in the same 512-byte window overrides a pending parser outcome.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Reader {
    bytes: ByteReader,
    validator: Validator,
    scanner: Scanner,
    parser: Parser,
    document_end: Option<Mark>,
    decoded_seen: usize,
    input_seen: bool,
}

/// A content-free merge key retaining all registers that affect future transitions.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ReaderControl {
    bytes: Option<ByteReader>,
    phase: usize,
    outcome: OutcomeControl,
    pending_end_offset: Option<isize>,
    scanner: Option<ScannerControl>,
    parser: Option<ParserControl>,
    input_seen: bool,
}

impl Reader {
    /// Steps the scanner immediately with one Unicode symbol.
    ///
    /// Ports `readerc.go:91` and `parserc.go:62`, with the raw validator kept separately.
    /// The observer receives provisional tokens and events; an error rejects the whole document.
    ///
    /// # Errors
    ///
    /// Returns the pinned diagnostic once its raw window or EOF commits the outcome.
    pub fn step_syntax(
        &mut self,
        character: char,
        observer: &mut impl FnMut(Syntax<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if self.validator.committed.is_some() {
            return self.validator.result();
        }
        if !self.input_seen {
            self.bytes.assume_utf8();
            if character == '\u{feff}' {
                self.input_seen = true;
                self.bytes.skip_utf8_bom();
                self.validator.phase = 3;
                return Ok(());
            }
        }
        // Inside one raw window a symbol's bytes decode to the symbol itself, so it skips the
        // byte decoder; a symbol crossing the window boundary takes the byte path.
        let width = character.len_utf8();
        if self.validator.active() && self.validator.phase + width < 512 && self.bytes.utf8_ready()
        {
            self.input_seen = true;
            self.validator.phase += width;
            match self.bytes.accept_utf8(character) {
                Ok(()) => {
                    self.decoded_seen += 1;
                    self.scan_character(character, observer);
                }
                Err(err) => self.validator.invalid = Some(err),
            }
            return Ok(());
        }
        let mut encoded = [0; 4];
        self.push_bytes_syntax(character.encode_utf8(&mut encoded).as_bytes(), observer)
    }

    /// Supplies bytes in any partition, retaining only an incomplete UTF prefix.
    ///
    /// Port of `readerc.go:56` (`yaml_parser_update_raw_buffer`).
    /// The validator stores a byte count and pending outcome, never source text.
    ///
    /// # Errors
    ///
    /// Returns the pinned diagnostic when a complete raw window commits it.
    pub fn push_bytes_syntax(
        &mut self,
        input: &[u8],
        observer: &mut impl FnMut(Syntax<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.validator.result()?;
        for &byte in input {
            if self.validator.committed.is_some() {
                break;
            }
            self.input_seen = true;
            self.validator.phase += 1;
            if self.validator.invalid.is_none() {
                match self.bytes.push(byte) {
                    Ok(characters) => {
                        for character in characters {
                            self.decoded_seen += 1;
                            self.scan_character(character, observer);
                        }
                    }
                    Err(err) => self.validator.invalid = Some(err),
                }
            }
            if self.validator.phase == 512 {
                self.scanner.close_buffer(self.decoded_seen);
                self.drain(observer);
                self.validator.commit()?;
                if self.validator.committed.is_none() {
                    self.validator.phase = self.bytes.pending_len();
                    self.scanner.open_buffer();
                }
            }
        }
        Ok(())
    }

    /// Supplies EOF and commits the first document's provisional outcome.
    ///
    /// Port of `readerc.go:91` and `scannerc.go:1020`.
    ///
    /// # Errors
    ///
    /// Returns incomplete UTF, scanner, parser or event-consumer failures in Go's order.
    pub fn finish_syntax(
        &mut self,
        observer: &mut impl FnMut(Syntax<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if self.validator.committed.is_some() {
            return self.validator.result();
        }
        if !self.input_seen {
            self.step_syntax('\n', observer)?;
        }
        if self.validator.pending.is_some() || self.validator.invalid.is_some() {
            return self.validator.commit();
        }
        match self.bytes.finish() {
            Ok(characters) => {
                for character in characters {
                    self.decoded_seen += 1;
                    self.scan_character(character, observer);
                }
            }
            Err(err) => self.validator.invalid = Some(err),
        }
        self.scanner.close_buffer(self.decoded_seen);
        self.scanner.finish_input();
        self.drain(observer);
        if self.validator.active() {
            match self.parser.finish() {
                Ok(events) => self.deliver(&events, observer),
                Err(err) => self.validator.pending = Some(Outcome::Failed(err)),
            }
        }
        self.validator.commit()
    }

    /// Steps the scanner immediately with one Unicode symbol.
    ///
    /// The sink receives provisional events; an error rejects the whole document.
    ///
    /// # Errors
    ///
    /// Returns the pinned diagnostic once its raw window or EOF commits the outcome.
    pub fn step(
        &mut self,
        character: char,
        sink: &mut impl FnMut(&Event) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.step_syntax(character, &mut events_only(sink))
    }

    /// Supplies bytes in any partition, retaining only an incomplete UTF prefix.
    ///
    /// # Errors
    ///
    /// Returns the pinned diagnostic when a complete raw window commits it.
    pub fn push_bytes(
        &mut self,
        input: &[u8],
        sink: &mut impl FnMut(&Event) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.push_bytes_syntax(input, &mut events_only(sink))
    }

    /// Supplies EOF and commits the first document's provisional outcome.
    ///
    /// # Errors
    ///
    /// Returns incomplete UTF, scanner, parser or event-consumer failures in Go's order.
    pub fn finish(
        &mut self,
        sink: &mut impl FnMut(&Event) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.finish_syntax(&mut events_only(sink))
    }

    /// Reports a first-document completion whose raw window has been validated.
    #[must_use]
    pub fn is_done(&self) -> bool {
        matches!(self.validator.committed, Some(Outcome::Done))
    }

    /// Returns the validated document-end mark, or stream end for an empty input.
    #[must_use]
    pub fn document_end(&self) -> Option<Mark> {
        if self.is_done() {
            self.document_end
        } else {
            None
        }
    }

    /// Projects control, omitting consumed text and absolute presentation positions.
    ///
    /// Only an incomplete encoding prefix remains in the byte decoder.
    /// Terminal marks are data; a provisional end retains its offset until validation commits.
    #[must_use]
    pub fn control(&self) -> ReaderControl {
        let outcome = match &self.validator.committed {
            Some(Outcome::Done) => OutcomeControl::Done,
            Some(Outcome::Failed(err)) => OutcomeControl::Failed(err.kind),
            None if self.validator.invalid.is_some() => {
                OutcomeControl::PendingFailure(ErrorKind::Reader)
            }
            None => match &self.validator.pending {
                Some(Outcome::Done) => OutcomeControl::PendingDone,
                Some(Outcome::Failed(err)) => OutcomeControl::PendingFailure(err.kind),
                None => OutcomeControl::Active,
            },
        };
        let terminal = self.validator.committed.is_some();
        let active = self.validator.active();
        let pending_end_offset = if matches!(outcome, OutcomeControl::PendingDone) {
            self.document_end.and_then(|mark| {
                let end = isize::try_from(mark.index).ok()?;
                let input = isize::try_from(self.decoded_seen).ok()?;
                Some(end - input)
            })
        } else {
            None
        };
        ReaderControl {
            bytes: if terminal || self.validator.invalid.is_some() {
                None
            } else {
                Some(self.bytes.control())
            },
            phase: if terminal { 0 } else { self.validator.phase },
            outcome,
            pending_end_offset,
            scanner: active.then(|| self.scanner.control()),
            parser: active.then(|| self.parser.control()),
            input_seen: !terminal && self.input_seen,
        }
    }

    /// Delivers a decoded symbol without waiting for the rest of its raw window.
    fn scan_character(
        &mut self,
        character: char,
        observer: &mut impl FnMut(Syntax<'_>) -> Result<(), Error>,
    ) {
        if self.validator.active() {
            self.scanner.push_input(character);
            self.drain(observer);
        }
    }

    /// Requests tokens in Go's pull order (`parserc.go:46-60`).
    fn drain(&mut self, observer: &mut impl FnMut(Syntax<'_>) -> Result<(), Error>) {
        while self.validator.active() {
            match self.scanner.next_token() {
                Ok(Some(token)) => {
                    if let Err(err) = observer(Syntax::Token(&token)) {
                        self.validator.pending = Some(Outcome::Failed(err));
                        break;
                    }
                    match self.parser.push(token) {
                        Ok(events) => self.deliver(&events, observer),
                        Err(err) => self.validator.pending = Some(Outcome::Failed(err)),
                    }
                }
                Ok(None) => break,
                Err(err) => self.validator.pending = Some(Outcome::Failed(err)),
            }
        }
    }

    fn deliver(
        &mut self,
        events: &[Event],
        observer: &mut impl FnMut(Syntax<'_>) -> Result<(), Error>,
    ) {
        for event in events {
            if !self.validator.active() {
                break;
            }
            if let Err(err) = observer(Syntax::Event(event)) {
                self.validator.pending = Some(Outcome::Failed(err));
                break;
            }
            if matches!(
                event.kind,
                EventKind::DocumentEnd { .. } | EventKind::StreamEnd
            ) {
                self.document_end = Some(event.end);
                self.validator.pending = Some(Outcome::Done);
            }
        }
    }
}

/// Adapts an event sink to a syntax observer that ignores tokens.
fn events_only(
    sink: &mut impl FnMut(&Event) -> Result<(), Error>,
) -> impl FnMut(Syntax<'_>) -> Result<(), Error> + '_ {
    move |syntax| match syntax {
        Syntax::Token(_) => Ok(()),
        Syntax::Event(event) => sink(event),
    }
}

//! The pinned reader, scanner and parser composed in Go's pull order.

use crate::parser::{Parser, ParserControl};
use crate::reader::ByteReader;
use crate::scanner::{Scanner, ScannerControl};
use crate::types::{Error, ErrorKind, Event, EventKind, Mark};

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

/// A clonable suspended reader of the first YAML document.
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
        let mut encoded = [0; 4];
        self.push_bytes(character.encode_utf8(&mut encoded).as_bytes(), sink)
    }

    /// Supplies bytes in any partition, retaining only an incomplete UTF prefix.
    ///
    /// Port of `readerc.go:56` (`yaml_parser_update_raw_buffer`).
    /// The validator stores a byte count and pending outcome, never source text.
    ///
    /// # Errors
    ///
    /// Returns the pinned diagnostic when a complete raw window commits it.
    pub fn push_bytes(
        &mut self,
        input: &[u8],
        sink: &mut impl FnMut(&Event) -> Result<(), Error>,
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
                            self.scan_character(character, sink);
                        }
                    }
                    Err(err) => self.validator.invalid = Some(err),
                }
            }
            if self.validator.phase == 512 {
                self.scanner.close_buffer(self.decoded_seen);
                self.drain(sink);
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
    pub fn finish(
        &mut self,
        sink: &mut impl FnMut(&Event) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if self.validator.committed.is_some() {
            return self.validator.result();
        }
        if !self.input_seen {
            self.step('\n', sink)?;
        }
        if self.validator.pending.is_some() || self.validator.invalid.is_some() {
            return self.validator.commit();
        }
        match self.bytes.finish() {
            Ok(characters) => {
                for character in characters {
                    self.decoded_seen += 1;
                    self.scan_character(character, sink);
                }
            }
            Err(err) => self.validator.invalid = Some(err),
        }
        self.scanner.close_buffer(self.decoded_seen);
        self.scanner.finish_input();
        self.drain(sink);
        if self.validator.active() {
            match self.parser.finish() {
                Ok(events) => self.deliver(&events, sink),
                Err(err) => self.validator.pending = Some(Outcome::Failed(err)),
            }
        }
        self.validator.commit()
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
        sink: &mut impl FnMut(&Event) -> Result<(), Error>,
    ) {
        if self.validator.active() {
            self.scanner.push_input(character);
            self.drain(sink);
        }
    }

    /// Requests tokens in Go's pull order (`parserc.go:46-60`).
    fn drain(&mut self, sink: &mut impl FnMut(&Event) -> Result<(), Error>) {
        while self.validator.active() {
            match self.scanner.next_token() {
                Ok(Some(token)) => match self.parser.push(token) {
                    Ok(events) => self.deliver(&events, sink),
                    Err(err) => self.validator.pending = Some(Outcome::Failed(err)),
                },
                Ok(None) => break,
                Err(err) => self.validator.pending = Some(Outcome::Failed(err)),
            }
        }
    }

    fn deliver(&mut self, events: &[Event], sink: &mut impl FnMut(&Event) -> Result<(), Error>) {
        for event in events {
            if !self.validator.active() {
                break;
            }
            if let Err(err) = sink(event) {
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

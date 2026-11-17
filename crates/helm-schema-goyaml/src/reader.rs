//! Incremental byte decoding from go-yaml v2.4.3's `readerc.go`.
//!
//! The original reader is Copyright 2006 Kirill Simonov and Copyright 2011-2016
//! Canonical Ltd, distributed under the MIT and Apache 2.0 licenses.

use crate::types::{Error, ErrorKind};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

/// The byte-order and partial-character state of go-yaml's reader.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ByteReader {
    encoding: Option<Encoding>,
    pending: Vec<u8>,
    offset: usize,
    finished: bool,
    initial_bom: bool,
}

impl ByteReader {
    /// Selects UTF-8 when the caller already supplies decoded Unicode symbols.
    pub(crate) fn assume_utf8(&mut self) {
        if self.encoding.is_none() && self.pending.is_empty() {
            self.encoding = Some(Encoding::Utf8);
        }
    }

    pub(crate) fn skip_utf8_bom(&mut self) {
        self.assume_utf8();
        self.offset += 3;
        self.initial_bom = true;
    }

    /// Whether the next decoded symbol can bypass byte decoding: UTF-8 is selected and no
    /// partial character or BOM is pending.
    pub(crate) fn utf8_ready(&self) -> bool {
        self.encoding == Some(Encoding::Utf8) && self.pending.is_empty() && !self.finished
    }

    /// Accepts one already-decoded UTF-8 symbol, with the printable check of the byte path.
    ///
    /// Port of `readerc.go:91` (`yaml_parser_update_buffer`, UTF-8 branch and range check).
    pub(crate) fn accept_utf8(&mut self, character: char) -> Result<(), Error> {
        if !matches!(u32::from(character), 0x09 | 0x0a | 0x0d | 0x20..=0x7e | 0x85 | 0xa0..=0xd7ff | 0xe000..=0xfffd | 0x0001_0000..=0x0010_ffff)
        {
            return Err(reader_error("control characters are not allowed"));
        }
        self.offset += character.len_utf8();
        Ok(())
    }

    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub(crate) fn control(&self) -> Self {
        let mut reader = self.clone();
        reader.offset = 0;
        reader
    }

    /// Decodes the next byte, retaining only an incomplete BOM or character.
    ///
    /// Port of `readerc.go:91` (`yaml_parser_update_buffer`).
    ///
    /// # Errors
    ///
    /// Returns the pinned reader's UTF and printable-character failures.
    pub fn push(&mut self, byte: u8) -> Result<Vec<char>, Error> {
        if self.finished {
            return Ok(Vec::new());
        }
        self.pending.push(byte);
        self.decode(false)
    }

    /// Finishes an input stream without emitting go-yaml's internal NUL sentinel.
    ///
    /// Port of `readerc.go:91` (`yaml_parser_update_buffer`, EOF branch).
    ///
    /// # Errors
    ///
    /// Returns an incomplete-character error if the stream ends within a character.
    pub fn finish(&mut self) -> Result<Vec<char>, Error> {
        if self.finished {
            return Ok(Vec::new());
        }
        let output = self.decode(true)?;
        self.finished = true;
        Ok(output)
    }

    /// Port of `readerc.go:25` (`yaml_parser_determine_encoding`).
    fn determine_encoding(&mut self, eof: bool) {
        if self.encoding.is_some() || (self.pending.len() < 3 && !eof) {
            return;
        }
        let (encoding, width) = if self.pending.starts_with(&[0xff, 0xfe]) {
            (Encoding::Utf16Le, 2)
        } else if self.pending.starts_with(&[0xfe, 0xff]) {
            (Encoding::Utf16Be, 2)
        } else if self.pending.starts_with(&[0xef, 0xbb, 0xbf]) {
            (Encoding::Utf8, 3)
        } else {
            (Encoding::Utf8, 0)
        };
        self.encoding = Some(encoding);
        self.initial_bom = width != 0;
        self.pending.drain(..width);
        self.offset += width;
    }

    /// Port of `readerc.go:91` (`yaml_parser_update_buffer`, decode loop).
    fn decode(&mut self, eof: bool) -> Result<Vec<char>, Error> {
        self.determine_encoding(eof);
        let Some(encoding) = self.encoding else {
            return Ok(Vec::new());
        };
        let mut output = Vec::new();
        while !self.pending.is_empty() {
            let decoded = match encoding {
                Encoding::Utf8 => self.decode_utf8(eof)?,
                Encoding::Utf16Le => self.decode_utf16(eof, true)?,
                Encoding::Utf16Be => self.decode_utf16(eof, false)?,
            };
            let Some(Decoded { value, width }) = decoded else {
                break;
            };
            if !matches!(value, 0x09 | 0x0a | 0x0d | 0x20..=0x7e | 0x85 | 0xa0..=0xd7ff | 0xe000..=0xfffd | 0x0001_0000..=0x0010_ffff)
            {
                return Err(reader_error("control characters are not allowed"));
            }
            let character =
                char::from_u32(value).ok_or_else(|| reader_error("invalid Unicode character"))?;
            self.pending.drain(..width);
            self.offset += width;
            output.push(character);
        }
        Ok(output)
    }

    /// Port of `readerc.go:91` (`yaml_parser_update_buffer`, UTF-8 branch).
    fn decode_utf8(&self, eof: bool) -> Result<Option<Decoded>, Error> {
        let Some(&octet) = self.pending.first() else {
            return Ok(None);
        };
        let (width, mut value) = if octet & 0x80 == 0 {
            (1, u32::from(octet & 0x7f))
        } else if octet & 0xe0 == 0xc0 {
            (2, u32::from(octet & 0x1f))
        } else if octet & 0xf0 == 0xe0 {
            (3, u32::from(octet & 0x0f))
        } else if octet & 0xf8 == 0xf0 {
            (4, u32::from(octet & 0x07))
        } else {
            return Err(reader_error("invalid leading UTF-8 octet"));
        };
        if self.pending.len() < width {
            return if eof {
                Err(reader_error("incomplete UTF-8 octet sequence"))
            } else {
                Ok(None)
            };
        }
        for &trailing in self.pending.iter().take(width).skip(1) {
            if trailing & 0xc0 != 0x80 {
                return Err(reader_error("invalid trailing UTF-8 octet"));
            }
            value = (value << 6) + u32::from(trailing & 0x3f);
        }
        if !matches!(
            (width, value),
            (1, _) | (2, 0x80..) | (3, 0x800..) | (4, 0x0001_0000..)
        ) {
            return Err(reader_error("invalid length of a UTF-8 sequence"));
        }
        if (0xd800..=0xdfff).contains(&value) || value > 0x0010_ffff {
            return Err(reader_error("invalid Unicode character"));
        }
        Ok(Some(Decoded { value, width }))
    }

    /// Port of `readerc.go:91` (`yaml_parser_update_buffer`, UTF-16 branch).
    fn decode_utf16(&self, eof: bool, little_endian: bool) -> Result<Option<Decoded>, Error> {
        let Some(first) = self.utf16_unit(0, little_endian) else {
            return if eof {
                Err(reader_error("incomplete UTF-16 character"))
            } else {
                Ok(None)
            };
        };
        let first = u32::from(first);
        if first & 0xfc00 == 0xdc00 {
            return Err(reader_error("unexpected low surrogate area"));
        }
        if first & 0xfc00 != 0xd800 {
            return Ok(Some(Decoded {
                value: first,
                width: 2,
            }));
        }
        let Some(second) = self.utf16_unit(2, little_endian) else {
            return if eof {
                Err(reader_error("incomplete UTF-16 surrogate pair"))
            } else {
                Ok(None)
            };
        };
        let second = u32::from(second);
        if second & 0xfc00 != 0xdc00 {
            return Err(reader_error("expected low surrogate area"));
        }
        Ok(Some(Decoded {
            value: 0x0001_0000 + ((first & 0x3ff) << 10) + (second & 0x3ff),
            width: 4,
        }))
    }

    fn utf16_unit(&self, offset: usize, little_endian: bool) -> Option<u16> {
        let first = *self.pending.get(offset)?;
        let second = *self.pending.get(offset + 1)?;
        Some(if little_endian {
            u16::from_le_bytes([first, second])
        } else {
            u16::from_be_bytes([first, second])
        })
    }
}

struct Decoded {
    value: u32,
    width: usize,
}

/// Port of `readerc.go:8` (`yaml_parser_set_reader_error`).
fn reader_error(message: &str) -> Error {
    Error {
        kind: ErrorKind::Reader,
        line: 0,
        message: message.to_owned(),
    }
}

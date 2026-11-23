use std::io::{Error, ErrorKind};

use bstr::ByteSlice;

pub(super) struct Rules(Vec<Rule>);

struct Rule {
    pattern: Vec<u8>,
    basename: bool,
    directory: bool,
    negate: bool,
}

impl Rules {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let rules = bytes.split(|&byte| byte == b'\n').enumerate();
        let mut parsed = Vec::new();
        for (index, line) in rules.chain([(1, b"templates/.?*".as_slice())]) {
            if line.len() >= 64 * 1024 {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    ".helmignore line exceeds Helm's scanner limit",
                ));
            }
            let raw = if index == 0 {
                line.strip_prefix(b"\xef\xbb\xbf").unwrap_or(line)
            } else {
                line
            }
            .trim();
            if raw.is_empty() || raw.starts_with(b"#") {
                continue;
            }
            if raw.find(b"**").is_some() || glob(raw, b"abc").is_err() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("invalid .helmignore pattern {:?}", raw.as_bstr()),
                ));
            }
            let negate = raw.starts_with(b"!");
            let pattern = raw.strip_prefix(b"!").unwrap_or(raw);
            let directory = pattern.ends_with(b"/");
            let pattern = pattern.strip_suffix(b"/").unwrap_or(pattern);
            parsed.push(Rule {
                basename: !pattern.contains(&b'/'),
                pattern: pattern.strip_prefix(b"/").unwrap_or(pattern).to_vec(),
                directory,
                negate,
            });
        }
        Ok(Self(parsed))
    }

    pub(super) fn ignores(&self, name: &[u8], directory: bool) -> bool {
        let basename = name.rsplit(|byte| *byte == b'/').next().unwrap_or(name);
        self.0.iter().any(|rule| {
            let path = if rule.basename { basename } else { name };
            ((!rule.directory || directory) && glob(&rule.pattern, path) == Ok(true)) != rule.negate
        })
    }
}

const SEPARATOR: u8 = if cfg!(windows) { b'\\' } else { b'/' };

// Go's filepath.Match advances a star by bytes and commits to the first matching chunk.
fn glob(mut pattern: &[u8], mut name: &[u8]) -> Result<bool, ()> {
    while !pattern.is_empty() {
        let star = pattern.starts_with(b"*");
        pattern = pattern.trim_start_with(|byte| byte == '*');
        let (chunk, tail) = chunk(pattern);
        pattern = tail;
        if star && chunk.is_empty() {
            return Ok(!name.contains(&SEPARATOR));
        }
        loop {
            if let Some(rest) = match_chunk(chunk, name)?
                && (!pattern.is_empty() || rest.is_empty())
            {
                name = rest;
                break;
            }
            match name.split_first() {
                Some((&byte, rest)) if star && byte != SEPARATOR => name = rest,
                _ => return Ok(false),
            }
        }
    }
    Ok(name.is_empty())
}

fn chunk(pattern: &[u8]) -> (&[u8], &[u8]) {
    let mut escaped = false;
    let mut class = false;
    for (index, byte) in pattern.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' if !cfg!(windows) => escaped = true,
            b'[' => class = true,
            b']' => class = false,
            b'*' if !class => return pattern.split_at(index),
            _ => {}
        }
    }
    (pattern, b"")
}

fn match_chunk<'a>(mut pattern: &[u8], name: &'a [u8]) -> Result<Option<&'a [u8]>, ()> {
    let mut name = Some(name);
    while let Some((&byte, rest)) = pattern.split_first() {
        pattern = rest;
        match byte {
            b'[' => (pattern, name) = class(rest, name)?,
            b'?' => {
                name = name
                    .filter(|name| !name.starts_with(&[SEPARATOR]))
                    .and_then(|name| rune(name).map(|(_, rest)| rest));
            }
            b'\\' if !cfg!(windows) => {
                let (&byte, rest) = rest.split_first().ok_or(())?;
                pattern = rest;
                name = name.and_then(|name| name.strip_prefix(&[byte]));
            }
            byte => name = name.and_then(|name| name.strip_prefix(&[byte])),
        }
    }
    Ok(name)
}

fn class<'p, 'n>(
    mut pattern: &'p [u8],
    name: Option<&'n [u8]>,
) -> Result<(&'p [u8], Option<&'n [u8]>), ()> {
    let negated = pattern.starts_with(b"^");
    pattern = pattern.strip_prefix(b"^").unwrap_or(pattern);
    let target = name.and_then(rune);
    let mut matched = false;
    loop {
        let (low, tail) = class_char(pattern)?;
        let (high, tail) = if let Some(tail) = tail.strip_prefix(b"-") {
            class_char(tail)?
        } else {
            (low, tail)
        };
        matched |= target.is_some_and(|(ch, _)| (low..=high).contains(&ch));
        if let Some(tail) = tail.strip_prefix(b"]") {
            return Ok((
                tail,
                target.filter(|_| matched != negated).map(|(_, rest)| rest),
            ));
        }
        pattern = tail;
    }
}

fn class_char(pattern: &[u8]) -> Result<(char, &[u8]), ()> {
    let input = match pattern.first() {
        Some(b'-' | b']') | None => return Err(()),
        Some(b'\\') if !cfg!(windows) => pattern.get(1..).ok_or(())?,
        Some(_) => pattern,
    };
    let (ch, width) = bstr::decode_utf8(input);
    Ok((ch.ok_or(())?, input.get(width..).ok_or(())?))
}

fn rune(name: &[u8]) -> Option<(char, &[u8])> {
    let (ch, width) = bstr::decode_utf8(name);
    Some((
        ch.unwrap_or(char::REPLACEMENT_CHARACTER),
        name.get(ch.map_or(1, |_| width)..)?,
    ))
}

//! Scalar resolution from go-yaml v2.4.3 `resolve.go`.

use base64::Engine as _;

use crate::{Error, ErrorKind, Value};

const PREFIX: &str = "tag:yaml.org,2002:";

/// Resolves a scalar into the generic Go target (`resolve.go:86-190`, `decode.go:405-481`).
pub(crate) fn resolve(tag: &str, input: &str, implicit: bool) -> Result<Value, Error> {
    let short = if tag.is_empty() {
        ""
    } else {
        let Some(short) = tag.strip_prefix(PREFIX).filter(|short| {
            matches!(
                *short,
                "str" | "bool" | "int" | "float" | "null" | "timestamp" | "binary"
            )
        }) else {
            return Ok(Value::String(input.to_owned()));
        };
        short
    };
    if tag.is_empty() && !implicit {
        return Ok(Value::String(input.to_owned()));
    }
    if short == "binary" {
        let raw = input.replace(['\r', '\n'], "");
        let engine = base64::engine::GeneralPurpose::new(
            &base64::alphabet::STANDARD,
            base64::engine::GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
        );
        let bytes = engine
            .decode(raw)
            .map_err(|_| decode_error("!!binary value contains invalid base64 data"))?;
        return Ok(match String::from_utf8(bytes) {
            Ok(text) => Value::String(text),
            Err(err) => Value::RawString(err.into_bytes()),
        });
    }
    if !matches!(
        short,
        "" | "str" | "bool" | "int" | "float" | "null" | "timestamp"
    ) {
        return Ok(Value::String(input.to_owned()));
    }
    if short == "str" {
        return Ok(Value::String(input.to_owned()));
    }
    let (resolved_tag, mut value) = infer(short, input);
    if short == "float" {
        if let Value::Int(integer) = value {
            value = Value::float(crate::decode::go_signed_float(integer));
        }
        if matches!(value, Value::Float(_)) {
            return Ok(value);
        }
    } else if short.is_empty() || short == resolved_tag {
        return Ok(value);
    }
    Err(decode_error(format!(
        "cannot decode !!{resolved_tag} `{input}` as a !!{short}"
    )))
}

/// Resolves YAML 1.1 literals and numeric prefixes (`resolve.go:32-52,113-189`).
fn infer(tag: &str, input: &str) -> (&'static str, Value) {
    let mapped = match input {
        "y" | "Y" | "yes" | "Yes" | "YES" | "true" | "True" | "TRUE" | "on" | "On" | "ON" => {
            Some(("bool", Value::Bool(true)))
        }
        "n" | "N" | "no" | "No" | "NO" | "false" | "False" | "FALSE" | "off" | "Off" | "OFF" => {
            Some(("bool", Value::Bool(false)))
        }
        "" | "~" | "null" | "Null" | "NULL" => Some(("null", Value::Null)),
        ".nan" | ".NaN" | ".NAN" => {
            Some(("float", Value::float(f64::from_bits(0x7ff8_0000_0000_0001))))
        }
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => {
            Some(("float", Value::float(f64::INFINITY)))
        }
        "-.inf" | "-.Inf" | "-.INF" => Some(("float", Value::float(f64::NEG_INFINITY))),
        "<<" => Some(("merge", Value::String(input.to_owned()))),
        _ => None,
    };
    if let Some(value) = mapped {
        return value;
    }
    match input.as_bytes().first() {
        Some(b'0'..=b'9' | b'+' | b'-') => {
            if (tag.is_empty() || tag == "timestamp") && timestamp(input) {
                return ("timestamp", Value::String(input.to_owned()));
            }
            let plain = input.replace('_', "");
            if let Some(value) = integer(&plain) {
                return ("int", value);
            }
            if decimal_float(&plain)
                && let Ok(value) = plain.parse::<f64>()
                && value.is_finite()
            {
                return ("float", Value::float(value));
            }
            if let Some(value) = binary_fallback(&plain) {
                return ("int", value);
            }
        }
        Some(b'.') => {
            if let Some(value) = dot_float(input) {
                return ("float", Value::float(value));
            }
        }
        _ => {}
    }
    ("str", Value::String(input.to_owned()))
}

/// Applies Go's decimal underscore rules before `ParseFloat` (`strconv/atof.go:620-639`).
fn dot_float(input: &str) -> Option<f64> {
    let bytes = input.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'_'
            && !(index
                .checked_sub(1)
                .and_then(|index| bytes.get(index))
                .is_some_and(u8::is_ascii_digit)
                && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
        {
            return None;
        }
    }
    let value = input.replace('_', "").parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

/// Implements `strconv.ParseInt`/`ParseUint` base zero (`resolve.go:149-185`).
fn integer(input: &str) -> Option<Value> {
    let (negative, plain) = if let Some(rest) = input.strip_prefix('-') {
        (true, rest)
    } else {
        (false, input.strip_prefix('+').unwrap_or(input))
    };
    let (base, digits) = if let Some(rest) = plain
        .strip_prefix("0x")
        .or_else(|| plain.strip_prefix("0X"))
    {
        (16, rest)
    } else if let Some(rest) = plain
        .strip_prefix("0b")
        .or_else(|| plain.strip_prefix("0B"))
    {
        (2, rest)
    } else if let Some(rest) = plain
        .strip_prefix("0o")
        .or_else(|| plain.strip_prefix("0O"))
    {
        (8, rest)
    } else if plain.len() > 1 && plain.starts_with('0') {
        (8, plain)
    } else {
        (10, plain)
    };
    if digits.is_empty() || digits.starts_with(['+', '-']) {
        return None;
    }
    let magnitude = u64::from_str_radix(digits, base).ok()?;
    if negative {
        if magnitude == 1_u64 << 63 {
            Some(Value::Int(i64::MIN))
        } else {
            i64::try_from(magnitude)
                .ok()
                .map(|value| Value::Int(-value))
        }
    } else if let Ok(value) = i64::try_from(magnitude) {
        Some(Value::Int(value))
    } else if !input.starts_with('+') {
        Some(Value::Uint(magnitude))
    } else {
        None
    }
}

/// Implements the historical signed binary fallback (`resolve.go:165-190`).
fn binary_fallback(input: &str) -> Option<Value> {
    let owned;
    let digits = if let Some(rest) = input.strip_prefix("0b") {
        rest
    } else {
        let rest = input.strip_prefix("-0b")?;
        owned = format!("-{rest}");
        &owned
    };
    let (negative, digits) = if let Some(rest) = digits.strip_prefix('-') {
        (true, rest)
    } else {
        (false, digits.strip_prefix('+').unwrap_or(digits))
    };
    if digits.is_empty() || digits.starts_with(['+', '-']) {
        return None;
    }
    let magnitude = u64::from_str_radix(digits, 2).ok()?;
    if negative {
        if magnitude == 1_u64 << 63 {
            Some(Value::Int(i64::MIN))
        } else {
            i64::try_from(magnitude)
                .ok()
                .map(|value| Value::Int(-value))
        }
    } else if let Ok(value) = i64::try_from(magnitude) {
        Some(Value::Int(value))
    } else if !input.strip_prefix("0b")?.starts_with('+') {
        Some(Value::Uint(magnitude))
    } else {
        None
    }
}

/// Recognizes `yamlStyleFloat` without compiling a second parser (`resolve.go:84`).
fn decimal_float(input: &str) -> bool {
    let input = input.strip_prefix(['+', '-']).unwrap_or(input);
    let mut parts = input.split(['e', 'E']);
    let mantissa = parts.next().unwrap_or_default();
    if let Some(exponent) = parts.next() {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent.is_empty() || !exponent.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
    }
    if parts.next().is_some() {
        return false;
    }
    let mut digits = 0;
    let mut dots = 0;
    for byte in mantissa.bytes() {
        if byte.is_ascii_digit() {
            digits += 1;
        } else if byte == b'.' {
            dots += 1;
        } else {
            return false;
        }
    }
    digits > 0 && dots <= 1
}

/// Checks Go's four accepted timestamp layouts (`resolve.go:222-258`).
fn timestamp(input: &str) -> bool {
    let Some((date, rest)) = input.split_once(['T', 't', ' ']) else {
        return valid_date(input);
    };
    if !valid_date(date) {
        return false;
    }
    let spaced = input.as_bytes().get(date.len()) == Some(&b' ');
    let rest = if spaced {
        rest.trim_start_matches(' ')
    } else {
        rest
    };
    let (time, zone) = if let Some(time) = rest.strip_suffix('Z') {
        (time, Some("Z"))
    } else if let Some(index) = rest.find(['+', '-']) {
        let Some(time) = rest.get(..index) else {
            return false;
        };
        (time, rest.get(index..))
    } else {
        (rest, None)
    };
    if spaced != zone.is_none() {
        return false;
    }
    if let Some(zone) = zone.filter(|zone| *zone != "Z") {
        let Some(zone) = zone.get(1..) else {
            return false;
        };
        let Some((hour, minute)) = zone.split_once(':') else {
            return false;
        };
        if hour.len() != 2 || minute.len() != 2 || !bounded(hour, 24) || !bounded(minute, 60) {
            return false;
        }
    }
    let mut fields = time.split(':');
    let hour = fields.next().unwrap_or_default();
    let minute = fields.next().unwrap_or_default();
    let second = fields.next().unwrap_or_default();
    let second = if let Some((second, fraction)) = second.split_once(['.', ',']) {
        if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        second
    } else {
        second
    };
    fields.next().is_none() && bounded(hour, 23) && bounded(minute, 59) && bounded(second, 59)
}

fn bounded(input: &str, max: u32) -> bool {
    !input.is_empty()
        && input.len() <= 2
        && input.bytes().all(|byte| byte.is_ascii_digit())
        && input.parse::<u32>().is_ok_and(|value| value <= max)
}

fn valid_date(input: &str) -> bool {
    let mut fields = input.split('-');
    let year = fields.next().unwrap_or_default();
    let month = fields.next().unwrap_or_default();
    let day = fields.next().unwrap_or_default();
    if fields.next().is_some()
        || year.len() != 4
        || !year.bytes().all(|byte| byte.is_ascii_digit())
        || !bounded(month, 12)
        || !bounded(day, 31)
    {
        return false;
    }
    let year = year.parse::<u32>().unwrap_or_default();
    let month = month.parse::<u32>().unwrap_or_default();
    let day = day.parse::<u32>().unwrap_or_default();
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    day > 0 && day <= days
}

pub(crate) fn decode_error(message: impl Into<String>) -> Error {
    Error {
        kind: ErrorKind::Decode,
        line: 0,
        message: message.into(),
    }
}

//! Differential verification of the int-cast string-preimage languages
//! against a reference implementation of Sprig's `int`/`int64` coercion
//! (`strconv.ParseInt` base 0 over `trimZeroDecimal`, parse failures and
//! overflow coercing to 0). Every claim direction is checked over an
//! exhaustive short-spelling corpus plus targeted boundary spellings:
//! a Within match must coerce into the region, and an Excluding
//! non-match must coerce into the region too (the escape language may
//! overapproximate, never the claim).

use crate::condition_encoding::{
    IntStringPreimage, decimal_strings_above, decimal_strings_below, int_region_string_preimage,
};

/// Go's `underscoreOK`: underscores separate digits, or the base prefix
/// from the first digit; anything else fails the parse.
fn underscore_ok(text: &str) -> bool {
    let mut saw = '^';
    let rest = text.strip_prefix(['+', '-']).unwrap_or(text);
    let bytes = rest.as_bytes();
    let mut index = 0;
    let mut hex = false;
    if bytes.len() >= 2
        && bytes[0] == b'0'
        && matches!(bytes[1].to_ascii_lowercase(), b'b' | b'o' | b'x')
    {
        index = 2;
        saw = '0';
        hex = bytes[1].eq_ignore_ascii_case(&b'x');
    }
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_digit() || (hex && byte.to_ascii_lowercase().is_ascii_hexdigit()) {
            saw = '0';
        } else if byte == b'_' {
            if saw != '0' {
                return false;
            }
            saw = '_';
        } else {
            if saw == '_' {
                return false;
            }
            saw = '!';
        }
        index += 1;
    }
    saw != '_'
}

/// Reference `strconv.ParseInt(text, 0, 64)`.
fn go_parse_int_base0(text: &str) -> Option<i64> {
    if text.is_empty() || !underscore_ok(text) {
        return None;
    }
    let (negative, unsigned) = match text.as_bytes()[0] {
        b'+' => (false, &text[1..]),
        b'-' => (true, &text[1..]),
        _ => (false, text),
    };
    if unsigned.is_empty() {
        return None;
    }
    let cleaned: String = unsigned.chars().filter(|ch| *ch != '_').collect();
    let bytes = cleaned.as_bytes();
    let (base, digits): (u32, &str) = if bytes.len() >= 2 && bytes[0] == b'0' {
        match bytes[1].to_ascii_lowercase() {
            b'x' => (16, &cleaned[2..]),
            b'b' => (2, &cleaned[2..]),
            b'o' => (8, &cleaned[2..]),
            _ => (8, &cleaned[1..]),
        }
    } else {
        (10, cleaned.as_str())
    };
    if digits.is_empty() {
        return None;
    }
    let mut value: i128 = 0;
    for ch in digits.chars() {
        let digit = i128::from(ch.to_digit(base)?);
        value = value * i128::from(base) + digit;
        if value > i128::from(i64::MAX) + 1 {
            return None;
        }
    }
    let signed = if negative { -value } else { value };
    i64::try_from(signed).ok()
}

/// spf13/cast `trimZeroDecimal`: strip a `.0…` tail with at least one
/// zero and nothing but zeros after the dot.
fn trim_zero_decimal(text: &str) -> &str {
    match text.rsplit_once('.') {
        Some((head, tail)) if !tail.is_empty() && tail.bytes().all(|byte| byte == b'0') => head,
        _ => text,
    }
}

/// Sprig's `int64` coercion for strings: parse failures coerce to 0.
fn sprig_int64(text: &str) -> i64 {
    go_parse_int_base0(trim_zero_decimal(text)).unwrap_or(0)
}

fn spelling_corpus() -> Vec<String> {
    let alphabet = [
        "0", "1", "5", "8", "9", "a", "f", "x", "o", "b", "_", "+", "-", ".",
    ];
    let mut corpus: Vec<String> = vec![String::new()];
    let mut layer: Vec<String> = vec![String::new()];
    for _ in 0..4 {
        layer = layer
            .iter()
            .flat_map(|prefix| alphabet.iter().map(move |token| format!("{prefix}{token}")))
            .collect();
        corpus.extend(layer.iter().cloned());
    }
    // Boundary spellings: values around each tested bound in every radix,
    // padded, signed, underscored, zero-decimal-tailed, and the parse
    // length cliffs where overflow starts coercing to 0.
    for value in [0i64, 1, 2, 254, 255, 256, 510, 511, 512, 4_294_967_295] {
        for spelling in [
            format!("{value}"),
            format!("+{value}"),
            format!("-{value}"),
            format!("{value}.0"),
            format!("{value}."),
            format!("0{value:o}"),
            format!("0o{value:o}"),
            format!("00{value:o}"),
            format!("0x{value:x}"),
            format!("0x{value:X}"),
            format!("0x0{value:x}"),
            format!("0b{value:b}"),
            "2_5_5".to_string(),
            format!("_{value}"),
            format!("{value}_"),
        ] {
            corpus.push(spelling);
        }
    }
    corpus.extend(
        [
            "9223372036854775807",
            "9223372036854775808",
            "9999999999999999999",
            "10000000000000000000",
            "99999999999999999999",
            "0x7fffffffffffffff",
            "0x8000000000000000",
            "0xffffffffffffffff",
            "0x10000000000000000",
            "0777777777777777777777",
            "01777777777777777777777",
            "017777777777777777777777",
        ]
        .map(str::to_string),
    );
    corpus
}

#[test]
fn int_cast_preimages_agree_with_the_reference_coercion() {
    let corpus = spelling_corpus();
    let bounds = [0i64, 1, 2, 255, 511, 4_294_967_295, -1, -2, -255];
    for bound in bounds {
        let above = decimal_strings_above(bound)
            .map(|pattern| regex::Regex::new(&pattern).expect("above pattern compiles"));
        let below = decimal_strings_below(bound)
            .map(|pattern| regex::Regex::new(&pattern).expect("below pattern compiles"));
        let gt_region = compile_preimage(int_region_string_preimage(true, bound));
        let lt_region = compile_preimage(int_region_string_preimage(false, bound));
        for spelling in &corpus {
            let coerced = sprig_int64(spelling);
            if let Some(above) = &above
                && above.is_match(spelling)
            {
                assert!(
                    coerced > bound,
                    "above({bound}) claims {spelling:?} but it coerces to {coerced}"
                );
            }
            if let Some(below) = &below
                && below.is_match(spelling)
            {
                assert!(
                    coerced < bound,
                    "below({bound}) claims {spelling:?} but it coerces to {coerced}"
                );
            }
            // The region preimages CLAIM a string when the Within pattern
            // matches, or when the Excluding escape does NOT match.
            if preimage_claims(&gt_region, spelling) {
                assert!(
                    coerced > bound,
                    "region(>{bound}) claims {spelling:?} but it coerces to {coerced}"
                );
            }
            if preimage_claims(&lt_region, spelling) {
                assert!(
                    coerced < bound,
                    "region(<{bound}) claims {spelling:?} but it coerces to {coerced}"
                );
            }
        }
    }
}

enum CompiledPreimage {
    Within(regex::Regex),
    Excluding(regex::Regex),
}

fn compile_preimage(preimage: IntStringPreimage) -> CompiledPreimage {
    match preimage {
        IntStringPreimage::Within(pattern) => {
            CompiledPreimage::Within(regex::Regex::new(&pattern).expect("within pattern compiles"))
        }
        IntStringPreimage::Excluding(pattern) => CompiledPreimage::Excluding(
            regex::Regex::new(&pattern).expect("excluding pattern compiles"),
        ),
    }
}

fn preimage_claims(preimage: &CompiledPreimage, spelling: &str) -> bool {
    match preimage {
        CompiledPreimage::Within(pattern) => pattern.is_match(spelling),
        CompiledPreimage::Excluding(pattern) => !pattern.is_match(spelling),
    }
}

/// The exact new claims the nineteenth-round refinement adds: certainly
/// -zero and certainly-small spellings land inside upper-bounded regions,
/// zero-padded radix forms land inside lower-bounded ones.
#[test]
fn refined_windows_claim_the_base0_families_exactly() {
    let above_one = decimal_strings_above(1)
        .map(|pattern| regex::Regex::new(&pattern).expect("pattern compiles"))
        .expect("bound 1 has a single-sign region");
    for spelling in ["05", "0x5", "0o5", "0b10", "0x05", "005", "2", "10"] {
        assert!(
            above_one.is_match(spelling),
            "{spelling:?} certainly parses above 1"
        );
    }
    for spelling in ["08", "0", "1", "01", "0x1", "", "abc", "_5", "5_"] {
        assert!(
            !above_one.is_match(spelling),
            "{spelling:?} does not certainly parse above 1"
        );
    }
    let IntStringPreimage::Excluding(escape) = int_region_string_preimage(false, 1) else {
        panic!("bound 1 upper region rides the excluding lane");
    };
    let escape = regex::Regex::new(&escape).expect("escape compiles");
    // Claimed (outside the escape): spellings certainly coercing to 0.
    for spelling in ["0", "00", "0x0", "0o0", "0b0", "abc", "", "08"] {
        assert!(
            !escape.is_match(spelling),
            "{spelling:?} certainly coerces below 1 and must be claimed"
        );
    }
    // Unclaimed escapes: anything that may reach 1.
    for spelling in ["1", "01", "0x1", "10094", "2_5_5", "0x_1"] {
        assert!(
            escape.is_match(spelling),
            "{spelling:?} may reach 1 and must stay unclaimed"
        );
    }
}

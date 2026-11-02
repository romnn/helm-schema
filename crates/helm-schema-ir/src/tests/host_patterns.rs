//! Reference host-parser languages used only by the oracle witness tests.

use super::{CollectionItemSemantics, FunctionSemantics, ParserSemantics};

impl FunctionSemantics {
    pub(crate) const fn is_known(self) -> bool {
        self.known
    }
}

/// Returns the lexical language required by a strict string parser operand.
///
/// The pattern is a conservative superset of every string accepted by the
/// runtime parser, so lowering it may miss some invalid inputs but never
/// rejects an input solely because the parser accepts a wider spelling.
#[must_use]
pub(crate) fn strict_parser_operand_pattern(
    semantics: FunctionSemantics,
    argument_count: usize,
) -> Option<(usize, &'static str)> {
    match semantics.parser {
        ParserSemantics::Semver if argument_count == 2 => {
            // Masterminds semver's coercing parser keeps the CORE segments
            // loose (leading zeros parse through `ParseUint`), but its
            // prerelease validation rejects a NUMERIC identifier with a
            // leading zero (`3.1.0-01` aborts while `3.1.0-rc.1` renders),
            // so the prerelease alternatives spell that rule out. Build
            // metadata stays unvalidated. Core components parse through
            // `ParseUint(…, 10, 64)`, which overflow-checks the VALUE, not
            // the spelling: leading zeros never overflow, so the bound
            // applies to the significant digits only — up to 20 may fit
            // uint64, while 21+ certainly overflow and abort (still a
            // superset of the accepted language).
            Some((
                argument_count - 1,
                r"^v?(0*[0-9]{1,20})(\.0*[0-9]{1,20})?(\.0*[0-9]{1,20})?(-(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)(\.(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*)?(\+([0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*))?$",
            ))
        }
        // `time.ParseDuration` overflow-checks each term twice: the raw
        // digit scan caps int64 (~19 significant digits) and the unit
        // scaling caps int64 NANOSECONDS, so a term whose significant
        // integer digits exceed the unit's may-fit count certainly aborts
        // (2562047h fits, 8-digit hour terms cannot). Leading zeros carry
        // no value and stay unbounded, as do fractional digits (the
        // fraction scan drops precision instead of overflowing). Multi-term
        // sums may still overflow inside the bounds; the pattern stays a
        // superset of the accepted language.
        ParserSemantics::Duration if argument_count == 2 => Some((
            0,
            r"^[+-]?(0|((0*[0-9]{1,19}(\.[0-9]*)?|\.[0-9]+)ns|(0*[0-9]{1,16}(\.[0-9]*)?|\.[0-9]+)(us|µs|μs)|(0*[0-9]{1,13}(\.[0-9]*)?|\.[0-9]+)ms|(0*[0-9]{1,10}(\.[0-9]*)?|\.[0-9]+)s|(0*[0-9]{1,9}(\.[0-9]*)?|\.[0-9]+)m|(0*[0-9]{1,7}(\.[0-9]*)?|\.[0-9]+)h)+)$",
        )),
        // Go `url.Parse`'s accepted language, differential-verified against
        // ~900k fuzz candidates plus a fixed battery (zero mismatches and
        // zero widenings against the lenient oracle; the battery is pinned
        // by `url_parse_pattern_matches_the_go_verdicts`). Structure: an
        // authority form (`scheme://…` or `//…`) parses userinfo (ASCII
        // charset, valid escapes), then either a bracketed host — a valid
        // IPv6 literal (netip's language, the F87 enumeration) with an
        // optional nonempty `%25` zone — or a plain host whose raw bytes
        // are Go's host-legal set (`[` forbidden, `]` legal), whose
        // escapes decode only to `%25` or non-ASCII bytes, and whose text
        // after the LAST colon must be digits (the pre-Go-1.26 port rule
        // for every scheme; Go 1.26 hardened http/https to a single
        // colon, so a multi-colon http host is a deliberate cross-version
        // widening). Paths validate escapes, queries stay raw, and
        // fragments validate escapes but accept control characters — Go
        // splits the fragment off before its control-byte check.
        // Non-authority forms: `scheme:` raw opaque text, rooted
        // single-slash paths, and relative references whose first segment
        // is colon-free.
        ParserSemantics::Url if argument_count == 1 => Some((0, URL_PARSE_PATTERN)),
        _ => None,
    }
}

macro_rules! ipv4_pattern {
    () => {
        concat!(
            r"((25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])\.){3}",
            "(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])"
        )
    };
}

/// The IPv6 textual language of `net.ParseIP`/`netip.ParseAddr` (no
/// zone), shared by the certificate ip-list items and `urlParse` bracket
/// hosts: 1-4 hex digits per group, at most one `::` expanding at least
/// one zero group, an embedded dotted quad only as the final 4 bytes. The
/// v4-embedded arms enumerate the group splits because a regex cannot
/// count the 8-group budget globally.
macro_rules! ipv6_pattern {
    () => {
        concat!(
            "([0-9A-Fa-f]{1,4}:){7}[0-9A-Fa-f]{1,4}",
            "|([0-9A-Fa-f]{1,4}:){1,7}:",
            "|([0-9A-Fa-f]{1,4}:){1,6}:[0-9A-Fa-f]{1,4}",
            "|([0-9A-Fa-f]{1,4}:){1,5}(:[0-9A-Fa-f]{1,4}){1,2}",
            "|([0-9A-Fa-f]{1,4}:){1,4}(:[0-9A-Fa-f]{1,4}){1,3}",
            "|([0-9A-Fa-f]{1,4}:){1,3}(:[0-9A-Fa-f]{1,4}){1,4}",
            "|([0-9A-Fa-f]{1,4}:){1,2}(:[0-9A-Fa-f]{1,4}){1,5}",
            "|[0-9A-Fa-f]{1,4}:(:[0-9A-Fa-f]{1,4}){1,6}",
            "|:(:[0-9A-Fa-f]{1,4}){1,7}",
            "|::",
            "|([0-9A-Fa-f]{1,4}:){6}",
            ipv4_pattern!(),
            "|::([0-9A-Fa-f]{1,4}:){0,5}",
            ipv4_pattern!(),
            "|([0-9A-Fa-f]{1,4}:){1}:([0-9A-Fa-f]{1,4}:){0,4}",
            ipv4_pattern!(),
            "|([0-9A-Fa-f]{1,4}:){2}:([0-9A-Fa-f]{1,4}:){0,3}",
            ipv4_pattern!(),
            "|([0-9A-Fa-f]{1,4}:){3}:([0-9A-Fa-f]{1,4}:){0,2}",
            ipv4_pattern!(),
            "|([0-9A-Fa-f]{1,4}:){4}:([0-9A-Fa-f]{1,4}:){0,1}",
            ipv4_pattern!(),
            "|([0-9A-Fa-f]{1,4}:){5}:",
            ipv4_pattern!()
        )
    };
}

/// Raw host-legal bytes (Go's unescaped host set: `[` forbidden, `]`
/// legal, non-ASCII free) beside the host escapes `%25` and non-ASCII
/// byte encodings.
macro_rules! url_host_char {
    () => {
        concat!(
            "(?:[A-Za-z0-9._~!$&'()*+,;=\\]<>\"\\-]",
            r"|[^\u0000-\u007F]",
            "|%25|%[89A-Fa-f][0-9A-Fa-f])"
        )
    };
}
/// Query (raw text) and fragment (escapes validated, control bytes legal
/// — Go splits the fragment off before its control-character check).
macro_rules! url_query_fragment {
    () => {
        r"(?:\?[^\u0000-\u001F\u007F#]*)?(?:#(?:[^%]|%[0-9A-Fa-f]{2})*)?"
    };
}
/// A rooted single-slash path: the body must not begin with a second `/`
/// (that spelling is the authority form). Escapes validate.
macro_rules! url_rooted {
    () => {
        r"/(?:(?:[^\u0000-\u001F\u007F%/?#]|%[0-9A-Fa-f]{2})(?:[^\u0000-\u001F\u007F%?#]|%[0-9A-Fa-f]{2})*)?"
    };
}
/// Userinfo, then a bracketed IPv6 (optional `%25` zone) or a plain host
/// under the last-colon port rule: with any colon present the text after
/// the last one must be digits; colon-free hosts are free.
macro_rules! url_authority {
    () => {
        concat!(
            r"(?:(?:[A-Za-z0-9._~!$&'()*+,;=:@\-]|%[0-9A-Fa-f]{2})*@)?",
            r"(?:\[(?:",
            ipv6_pattern!(),
            r")(?:%25(?:[^\u0000-\u001F\u007F%/?#\]]|%[0-9A-Fa-f]{2})+)?\](?::[0-9]*)?",
            "|(?:(?:",
            url_host_char!(),
            "|:)*:[0-9]*|",
            url_host_char!(),
            "*))"
        )
    };
}
/// The optional path/query/fragment after an authority.
macro_rules! url_authority_tail {
    () => {
        concat!(
            r"(?:/(?:[^\u0000-\u001F\u007F%?#]|%[0-9A-Fa-f]{2})*)?",
            url_query_fragment!()
        )
    };
}

const URL_PARSE_PATTERN: &str = concat!(
    "^(?:",
    r"[A-Za-z][A-Za-z0-9+.\-]*://",
    url_authority!(),
    url_authority_tail!(),
    "|//",
    url_authority!(),
    url_authority_tail!(),
    r"|[A-Za-z][A-Za-z0-9+.\-]*:(?:[^\u0000-\u001F\u007F/?#][^\u0000-\u001F\u007F?#]*)?",
    url_query_fragment!(),
    r"|[A-Za-z][A-Za-z0-9+.\-]*:",
    url_rooted!(),
    url_query_fragment!(),
    r"|(?:[^\u0000-\u001F\u007F%:/?#]|%[0-9A-Fa-f]{2})+(?:/(?:[^\u0000-\u001F\u007F%?#]|%[0-9A-Fa-f]{2})*)?",
    url_query_fragment!(),
    "|",
    url_rooted!(),
    url_query_fragment!(),
    "|",
    url_query_fragment!(),
    ")$",
);

/// Returns the lexical language required of every ITEM of a strict
/// collection operand, keyed by the zero-based operand index.
///
/// Like [`strict_parser_operand_pattern`], the pattern is a conservative
/// superset of every string the runtime parser accepts, so lowering it may
/// miss some invalid inputs but never rejects one the parser accepts.
#[must_use]
pub(crate) fn strict_collection_item_pattern(
    semantics: FunctionSemantics,
    index: usize,
) -> Option<&'static str> {
    match (semantics.collection_items, index) {
        // genSignedCert/genSelfSignedCert pass every ip-list entry through
        // net.ParseIP and abort rendering on nil. The pattern is the
        // parser's EXACT accepted language (fuzz-differentialed against
        // `net.ParseIP`): dotted-quad IPv4 without leading zeros, plus the
        // shared IPv6 enumeration (no zone suffix — ParseIP rejects them).
        (CollectionItemSemantics::CertificateIpList, 1) => {
            Some(concat!("^(", ipv4_pattern!(), "|", ipv6_pattern!(), ")$",))
        }
        _ => None,
    }
}

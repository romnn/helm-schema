//! The `urlParse` operand pattern must be Go `url.Parse`'s accepted
//! language. Every verdict below was produced by the Go parser itself
//! (`GODEBUG=urlstrictcolons=0`, the lenient pre-1.26 port rule the
//! pattern deliberately targets across helm builds); the full pattern was
//! additionally fuzz-differentialed against ~900k candidates with zero
//! mismatches in either direction. An accepted URL rejected here is a
//! false rejection; a rejected URL accepted here would let a render abort
//! through the schema.

use helm_schema_ast::strict_parser_operand_pattern;

#[test]
fn url_parse_pattern_matches_the_go_verdicts() {
    let (index, pattern) =
        strict_parser_operand_pattern("urlParse", 1).expect("urlParse pattern catalogued");
    assert!(index == 0, "urlParse subject is the first operand");
    let regex = regex::Regex::new(pattern).expect("valid regex");

    for (candidate, accepted) in [
        ("", true),
        ("http://example.com", true),
        ("http://example.com:8080/path?q=1#f", true),
        ("http://host:99x/", false),
        ("http://ho st/", false),
        ("http://h/p a t h", true),
        ("http://user name@h/", false),
        ("http://u@h@x/", true),
        ("http://[::1]:80/", true),
        ("http://[::1", false),
        ("http://%zz/", false),
        ("http://h/?a=%zz", true),
        ("http://h/#%zz", false),
        ("foo.com:8080/path", true),
        ("://x", false),
        ("//host:junk/path", false),
        ("//ho st", false),
        ("///path", true),
        ("mailto:user@example", true),
        ("mailto:%zz", true),
        ("a:b:c", true),
        ("/a:b", true),
        (":8080/x", false),
        ("h ttp://x y", false),
        ("http://", true),
        ("http://u@", true),
        ("http://:pass@h/", true),
        ("http://host:/", true),
        ("1:x", false),
        ("http:", true),
        ("http:?q", true),
        ("http:/", true),
        ("http://h/%zz", false),
        ("a//b", true),
        ("http://hést/", true),
        ("//u@h:80/p?q#f", true),
        ("//%zz", false),
        ("http://a:b:1/", true),
        ("http://a:b/", false),
        ("http://[a]b]", false),
        ("relative/path", true),
        ("?query#frag", true),
        ("#frag", true),
        ("scheme:opaque?query#frag", true),
        ("scheme:opaque%zz", true),
        ("http://<h>/", true),
        ("path%zz", false),
        ("path%41", true),
        ("+a:x", false),
        ("ht~tp://h/", false),
        ("http://a[b]c/", false),
        ("http://a]b/", true),
        ("http://h%25x/", true),
        ("http://h%41x/", false),
        ("http://h%80x/", true),
        ("http://[fe80::1%25eth0]/", true),
        ("http://[fe80::1%25]/", false),
        ("http://[abc]/", false),
        ("http://[]/", false),
        ("http://[::ffff:1.2.3.4]/", true),
        ("a/b//c:d", true),
        ("a:b/c", true),
        ("http://u%2F@h/", true),
        ("http://h/p%2Fq", true),
        ("scheme:op#f%zz", false),
        ("http://u@[::1]/", true),
        ("http://[::1]:/", true),
        ("http://fe80::1/", true),
        // Control bytes: rejected before the fragment, legal inside it.
        ("http://h/#f\u{1}", true),
        ("x#\u{1}", true),
        ("\u{1}#x", false),
        ("http://h/?q\u{1}", false),
    ] {
        let matched = regex.is_match(candidate);
        assert!(
            matched == accepted,
            "url.Parse {} {candidate:?} but the pattern {}",
            if accepted { "accepts" } else { "rejects" },
            if matched { "matches" } else { "does not match" },
        );
    }
}

//! The `genSignedCert`/`genSelfSignedCert` ip-list item pattern must be
//! `net.ParseIP`'s exact accepted language: every probe below was verified
//! against the Go parser (and the boundary cases against `helm template`),
//! so an accepted spelling rejected here is a false rejection and an
//! accepted invalid spelling would let a render abort through the schema.

use helm_schema_ast::strict_collection_item_pattern;

#[test]
fn ip_item_pattern_is_the_parse_ip_language() {
    let pattern =
        strict_collection_item_pattern("genSignedCert", 1).expect("ip item pattern catalogued");
    let regex = regex::Regex::new(pattern).expect("valid regex");

    let accepted = [
        // IPv4 without leading zeros.
        "1.2.3.4",
        "0.0.0.0",
        "255.255.255.255",
        // IPv6 full, compressed, and mixed-case forms.
        "::",
        "::1",
        "1::",
        "fe80::1",
        "FE80::A",
        "1:2:3:4:5:6:7:8",
        "1:2:3:4:5:6:7::",
        "::1:2:3:4:5:6:7",
        "1::2:3:4:5:6:7",
        "0001::1",
        // Embedded dotted quads as the final four bytes.
        "1:2:3:4:5:6:1.2.3.4",
        "::1.2.3.4",
        "::ffff:1.2.3.4",
        "::ffff:0:1.2.3.4",
        "::5:6:1.2.3.4",
        "1::1.2.3.4",
        "1:2:3:4:5::1.2.3.4",
    ];
    for candidate in accepted {
        assert!(
            regex.is_match(candidate),
            "ParseIP accepts {candidate}; rejecting it is a false rejection"
        );
    }

    let rejected = [
        // The `::` must expand at least one zero group.
        "1:2:3:4:5:6:7:8::",
        "::1:2:3:4:5:6:7:8",
        // Group and quad lexical limits.
        "00001::1",
        "12345::1",
        "g::1",
        "::1.2.3.04",
        "::1.2.3.256",
        "1:2:3:4:5:6:7:1.2.3.4",
        "1:2:3:4:5:6::1.2.3.4",
        "1:2:3:4:5:6:1.2.3.4.5",
        // Structure and zone rejections.
        ":",
        ":::",
        "1::2::3",
        "1:2:3:4:5:6:7",
        "fe80::1%eth0",
        "::%",
    ];
    for candidate in rejected {
        assert!(
            !regex.is_match(candidate),
            "ParseIP rejects {candidate}; accepting it would let a render abort"
        );
    }
}

use std::path::Path;

use test_util::prelude::sim_assert_eq;

use crate::cache::cache_entry::{CacheEntry, NonPortableCacheName};

fn reason(source_id: &str, dir: &str, file: &str) -> Option<NonPortableCacheName> {
    CacheEntry::new(Path::new("/cache"), source_id, dir, file)
        .writable()
        .err()
        .cloned()
}

/// The two records the corpus intake once committed to the provider bundle,
/// which no Windows checkout could create.
#[test]
fn refuses_the_historical_quote_records() {
    for file in ["certificate-\".json", "issuer-\".json"] {
        sim_assert_eq!(
            have: reason("default", "v1.29.0-standalone-strict", file),
            want: Some(NonPortableCacheName::Character {
                component: file.to_string(),
                character: '"',
            })
        );
    }
}

#[test]
fn refuses_every_non_portable_component() {
    let long = "a".repeat(201);
    for (source_id, dir, file, want) in [
        (
            "",
            "v1.29.0",
            "service-v1.json",
            NonPortableCacheName::Empty,
        ),
        (
            "default",
            "V1.29.0",
            "service-v1.json",
            NonPortableCacheName::Character {
                component: "V1.29.0".to_string(),
                character: 'V',
            },
        ),
        (
            "default",
            "example.com",
            "foo:bar_v1.json",
            NonPortableCacheName::Character {
                component: "foo:bar_v1.json".to_string(),
                character: ':',
            },
        ),
        (
            "default",
            "example.com",
            "foo/bar_v1.json",
            NonPortableCacheName::Character {
                component: "foo/bar_v1.json".to_string(),
                character: '/',
            },
        ),
        (
            "default",
            "example.com",
            "tab\t_v1.json",
            NonPortableCacheName::Character {
                component: "tab\t_v1.json".to_string(),
                character: '\t',
            },
        ),
        (
            "default",
            "..",
            "service-v1.json",
            NonPortableCacheName::DotEdge {
                component: "..".to_string(),
            },
        ),
        (
            "default",
            "example.com.",
            "service-v1.json",
            NonPortableCacheName::DotEdge {
                component: "example.com.".to_string(),
            },
        ),
        (
            "default",
            "nul",
            "service-v1.json",
            NonPortableCacheName::ReservedDevice {
                component: "nul".to_string(),
            },
        ),
        (
            "default",
            "example.com",
            "com1.json",
            NonPortableCacheName::ReservedDevice {
                component: "com1.json".to_string(),
            },
        ),
        (
            "default",
            "example.com",
            long.as_str(),
            NonPortableCacheName::TooLong {
                component: long.clone(),
            },
        ),
    ] {
        sim_assert_eq!(have: reason(source_id, dir, file), want: Some(want));
    }
}

#[test]
fn accepts_names_the_providers_write() {
    for (source_id, dir, file) in [
        (
            "default",
            "v1.29.0-standalone-strict",
            "certificate-cert-manager-io-v1.json",
        ),
        ("default", "v1.35.0", "_definitions.json"),
        ("0123456789ab", "cert-manager.io", "certificate_v1.json"),
        (
            "default",
            "k8s.cni.cncf.io",
            "network-attachment-definition_v1.json",
        ),
        ("default", "example.com", "console_v1.json"),
    ] {
        sim_assert_eq!(have: reason(source_id, dir, file), want: None);
    }
}

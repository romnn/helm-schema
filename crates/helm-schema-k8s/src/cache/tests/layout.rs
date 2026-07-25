use std::collections::BTreeMap;
use std::path::PathBuf;

use test_util::prelude::sim_assert_eq;

use super::{cache_home_for, default_cache_dir};

/// A scripted environment for [`cache_home_for`], so both platform branches
/// run under every host OS in CI. Values use host-absolute paths — the
/// function checks absoluteness with the host's rules, and the logic under
/// test is variable precedence, not path syntax.
fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<PathBuf> {
    let map: BTreeMap<String, String> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    move |var: &str| map.get(var).map(PathBuf::from)
}

/// The resolved root must never be working-directory-relative, whatever the
/// environment looks like. Windows sets neither `XDG_CACHE_HOME` nor `HOME`,
/// and an earlier fallback returned a bare `.cache` there — so the cache
/// landed wherever the process happened to be launched from, and two runs
/// over the same chart from different directories consulted different caches.
#[test]
fn default_cache_dir_is_always_absolute() {
    let root = default_cache_dir(
        "HELM_SCHEMA_TEST_CACHE_ROOT_UNSET",
        "kubernetes-json-schema",
    );
    assert!(
        root.is_absolute(),
        "cache root must be absolute, got {}",
        root.display()
    );
}

/// The leaf keeps the two managed roots apart under one cache home.
#[test]
fn default_cache_dir_separates_managed_roots_by_leaf() {
    let k8s = default_cache_dir(
        "HELM_SCHEMA_TEST_CACHE_ROOT_UNSET",
        "kubernetes-json-schema",
    );
    let crd = default_cache_dir("HELM_SCHEMA_TEST_CACHE_ROOT_UNSET", "crds-catalog");

    assert_ne!(k8s, crd, "managed roots must not share a directory");
    sim_assert_eq!(have: k8s.parent(), want: crd.parent());
    sim_assert_eq!(
        have: k8s.file_name().and_then(std::ffi::OsStr::to_str),
        want: Some("kubernetes-json-schema")
    );
}

#[test]
fn unix_prefers_xdg_cache_home_over_home() {
    sim_assert_eq!(
        have: cache_home_for(false, env(&[("XDG_CACHE_HOME", "/xdg"), ("HOME", "/home/u")])),
        want: Some(PathBuf::from("/xdg"))
    );
}

#[test]
fn unix_home_fallback_appends_dot_cache() {
    sim_assert_eq!(
        have: cache_home_for(false, env(&[("HOME", "/home/u")])),
        want: Some(PathBuf::from("/home/u").join(".cache"))
    );
}

/// The XDG basedir rule: a relative path in these variables is invalid and
/// ignored. Honoring it would anchor the "per-user" cache to the working
/// directory.
#[test]
fn unix_relative_xdg_cache_home_is_ignored() {
    sim_assert_eq!(
        have: cache_home_for(false, env(&[("XDG_CACHE_HOME", "rel/cache"), ("HOME", "/home/u")])),
        want: Some(PathBuf::from("/home/u").join(".cache"))
    );
}

#[test]
fn unix_without_profile_yields_none() {
    sim_assert_eq!(have: cache_home_for(false, env(&[])), want: None);
}

#[test]
fn windows_prefers_localappdata() {
    sim_assert_eq!(
        have: cache_home_for(true, env(&[("LOCALAPPDATA", "/win/local"), ("USERPROFILE", "/win/profile")])),
        want: Some(PathBuf::from("/win/local"))
    );
}

#[test]
fn windows_userprofile_fallback_appends_appdata_local() {
    sim_assert_eq!(
        have: cache_home_for(true, env(&[("USERPROFILE", "/win/profile")])),
        want: Some(PathBuf::from("/win/profile").join("AppData").join("Local"))
    );
}

/// cmd and PowerShell leave HOME unset while MSYS shells set it elsewhere;
/// consulting it would move the cache depending on the launching shell. The
/// Windows branch must resolve from Windows variables only.
#[test]
fn windows_ignores_home() {
    sim_assert_eq!(
        have: cache_home_for(true, env(&[("HOME", "/home/u")])),
        want: None
    );
}

#[test]
fn windows_relative_localappdata_is_ignored() {
    sim_assert_eq!(
        have: cache_home_for(true, env(&[("LOCALAPPDATA", "rel/local"), ("USERPROFILE", "/win/profile")])),
        want: Some(PathBuf::from("/win/profile").join("AppData").join("Local"))
    );
}

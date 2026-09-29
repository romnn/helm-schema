//! Decides whether the renders of a prepared chart may be replayed.
//!
//! A render may be replayed only when identical inputs must produce
//! identical output. Helm executes every file under a chart's `templates`,
//! packaged dependencies included. A chart is never replayed when any such
//! file calls a clock, randomness, key or certificate generation, salted
//! hashing, a function of unordered map iteration, or `tpl`, whose subject
//! can assemble any of these at render time; nor when a template holds an
//! action the parser cannot model. A chart whose only such call is `lookup`
//! is replayed only as a client-only template, where `lookup` answers empty.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read as _;
use std::path::Path;

use color_eyre::eyre;
use flate2::read::GzDecoder;
use helm_schema_ast::{TemplateExpr, contains_template_action, parse_action_expressions};

use crate::helm::invocation::{Cacheability, Replay};

/// Template functions whose result can differ between identical invocations.
const NONDETERMINISTIC_FUNCTIONS: &[&str] = &[
    // Clocks.
    "ago",
    "date",
    "dateInZone",
    "dateModify",
    "date_in_zone",
    "date_modify",
    "htmlDate",
    "htmlDateInZone",
    "mustDateModify",
    "must_date_modify",
    "now",
    "unixEpoch",
    // Randomness and salted or keyed generation.
    "bcrypt",
    "derivePassword",
    "encryptAES",
    "genCA",
    "genCAWithKey",
    "genPrivateKey",
    "genSelfSignedCert",
    "genSelfSignedCertWithKey",
    "genSignedCert",
    "genSignedCertWithKey",
    "htpasswd",
    "randAlpha",
    "randAlphaNum",
    "randAscii",
    "randBytes",
    "randInt",
    "randNumeric",
    "shuffle",
    "uuidv4",
    // Unordered map iteration.
    "keys",
    "values",
    // Text executed at render time.
    "tpl",
    // Answers from outside the chart.
    "getHostByName",
];

/// Answers empty unless Helm talks to a cluster.
const CLUSTER_FUNCTION: &str = "lookup";

/// Whether renders of the prepared chart at `chart` may be replayed, with
/// the first reasons they may not.
///
/// # Errors
///
/// Returns an error when the chart or one of its archives cannot be read.
pub fn render_cacheability(chart: &Path) -> eyre::Result<Cacheability> {
    let mut reasons = BTreeSet::new();
    scan_directory(chart, "", &mut reasons)?;
    let lookups = reasons
        .iter()
        .filter(|reason| reason.ends_with(&format!(": calls {CLUSTER_FUNCTION}")))
        .count();
    let mut reasons = reasons.into_iter();
    Ok(Cacheability::trusted(match reasons.next() {
        None => Replay::Cacheable,
        Some(_) if lookups == reasons.len() + 1 => Replay::ClientOnly,
        Some(first) => {
            let more = reasons.count();
            Replay::Bypass(if more == 0 {
                first
            } else {
                format!("{first} (and {more} more)")
            })
        }
    }))
}

fn scan_directory(
    directory: &Path,
    location: &str,
    reasons: &mut BTreeSet<String>,
) -> eyre::Result<()> {
    let mut entries = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{location}{name}");
        if entry.file_type()?.is_dir() {
            scan_directory(&entry.path(), &format!("{path}/"), reasons)?;
        } else {
            scan_file(&path, &fs::read(entry.path())?, reasons)?;
        }
    }
    Ok(())
}

fn scan_file(path: &str, bytes: &[u8], reasons: &mut BTreeSet<String>) -> eyre::Result<()> {
    let archive = Path::new(path)
        .extension()
        .is_some_and(|extension| extension == "tgz")
        || path.ends_with(".tar.gz");
    if archive {
        let mut archive = tar::Archive::new(GzDecoder::new(bytes));
        for entry in archive.entries()? {
            let mut entry = entry?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let member = format!("{path}:{}", entry.path()?.to_string_lossy());
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents)?;
            scan_file(&member, &contents, reasons)?;
        }
    } else if path.split(['/', ':']).any(|part| part == "templates") {
        match std::str::from_utf8(bytes) {
            Ok(source) => scan_template(path, source, reasons),
            Err(_) => {
                reasons.insert(format!("{path}: template is not UTF-8"));
            }
        }
    }
    Ok(())
}

/// Records the nondeterministic calls of the template `source`.
fn scan_template(path: &str, source: &str, reasons: &mut BTreeSet<String>) {
    match contains_template_action(source) {
        Ok(false) => return,
        Ok(true) => {}
        Err(_) => {
            reasons.insert(format!("{path}: template does not parse"));
            return;
        }
    }
    let expressions = parse_action_expressions(source);
    if expressions.is_empty() {
        reasons.insert(format!("{path}: template actions do not parse"));
    }
    for expression in &expressions {
        expression.walk(|node| match node {
            TemplateExpr::Call { function, .. }
                if function == CLUSTER_FUNCTION
                    || NONDETERMINISTIC_FUNCTIONS.contains(&function.as_str()) =>
            {
                reasons.insert(format!("{path}: calls {function}"));
            }
            TemplateExpr::Unknown(text) => {
                reasons.insert(format!("{path}: unmodeled expression {text:?}"));
            }
            _ => {}
        });
    }
}

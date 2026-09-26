//! Helm's `.helmignore` rules (`pkg/ignore/rules.go`).
//!
//! The directory loader applies the root chart's rules to every path of the
//! tree, subcharts included, so they decide which values files and
//! dependencies Helm loads at all.

use super::ValuesError;

pub(super) struct Rules {
    patterns: Vec<Pattern>,
}

struct Pattern {
    glob: Vec<char>,
    negate: bool,
    must_dir: bool,
    scope: Scope,
}

enum Scope {
    /// A pattern with a leading `/` or an inner `/` matches the whole path.
    Path,
    /// A pattern without a `/` matches the file name only.
    FileName,
}

impl Rules {
    /// The rules of a `.helmignore` file plus Helm's default rule.
    pub(super) fn parse(source: &str) -> Result<Self, ValuesError> {
        let mut rules = Self {
            patterns: Vec::new(),
        };
        let source = source.strip_prefix('\u{feff}').unwrap_or(source);
        for line in source.lines() {
            rules.add(line)?;
        }
        rules.add("templates/.?*")?;
        Ok(rules)
    }

    fn add(&mut self, rule: &str) -> Result<(), ValuesError> {
        let rule = rule.trim();
        if rule.is_empty() || rule.starts_with('#') {
            return Ok(());
        }
        if rule.contains("**") {
            return Err(ValuesError::NotValidated(format!(
                "Helm refuses the .helmignore rule {rule:?}: double-star (**) syntax is not supported"
            )));
        }
        // `filepath.Match` character classes and escapes are not ported.
        if rule.contains(['[', '\\']) {
            return Err(ValuesError::Unmodelled(format!(
                ".helmignore rule {rule:?}"
            )));
        }
        let (negate, rule) = match rule.strip_prefix('!') {
            Some(rule) => (true, rule),
            None => (false, rule),
        };
        let (must_dir, rule) = match rule.strip_suffix('/') {
            Some(rule) => (true, rule),
            None => (false, rule),
        };
        let (scope, rule) = match rule.strip_prefix('/') {
            Some(rule) => (Scope::Path, rule),
            None if rule.contains('/') => (Scope::Path, rule),
            None => (Scope::FileName, rule),
        };
        self.patterns.push(Pattern {
            glob: rule.chars().collect(),
            negate,
            must_dir,
            scope,
        });
        Ok(())
    }

    /// `Rules.Ignore` for a `/`-separated path relative to the chart root.
    ///
    /// A negated rule ignores every path it does NOT match, exactly as Helm
    /// v4.2.3 evaluates it.
    pub(super) fn ignores(&self, path: &str, is_dir: bool) -> bool {
        if path.is_empty() || path == "." || path == "./" {
            return false;
        }
        let file_name: Vec<char> = path.rsplit('/').next().unwrap_or(path).chars().collect();
        let path: Vec<char> = path.chars().collect();
        for pattern in &self.patterns {
            let subject = match pattern.scope {
                Scope::Path => &path,
                Scope::FileName => &file_name,
            };
            if pattern.negate {
                if pattern.must_dir && !is_dir {
                    return true;
                }
                if !glob_matches(&pattern.glob, subject) {
                    return true;
                }
                continue;
            }
            if pattern.must_dir && !is_dir {
                continue;
            }
            if glob_matches(&pattern.glob, subject) {
                return true;
            }
        }
        false
    }
}

/// `filepath.Match` for `*`, `?` and literal characters; neither wildcard
/// matches the `/` separator.
fn glob_matches(pattern: &[char], name: &[char]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some(('*', rest)) => {
            let mut skipped = 0;
            loop {
                if glob_matches(rest, name.get(skipped..).unwrap_or_default()) {
                    return true;
                }
                match name.get(skipped) {
                    Some('/') | None => return false,
                    Some(_) => skipped += 1,
                }
            }
        }
        Some(('?', rest)) => match name.split_first() {
            Some((character, tail)) if *character != '/' => glob_matches(rest, tail),
            _ => false,
        },
        Some((literal, rest)) => match name.split_first() {
            Some((character, tail)) if character == literal => glob_matches(rest, tail),
            _ => false,
        },
    }
}

use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub(crate) struct GetBinding {
    pub(crate) base: String,
    pub(crate) key_var: String,
}

pub(crate) fn parse_literal_list_range(header: &str) -> Option<(String, Vec<String>)> {
    if !header.contains("list") {
        return None;
    }

    let toks: Vec<&str> = header.split_whitespace().collect();
    let list_pos = toks.iter().position(|t| *t == "list")?;

    // `range $k := list ...` or `$k := list ...` or `list ...` (in some
    // tree-sitter nodes). We only care about the bound variable name and the
    // literal domain.
    let var = toks
        .iter()
        .take(list_pos)
        .find_map(|t| t.strip_prefix('$'))
        .filter(|value| {
            !value.is_empty()
                && !value.contains('.')
                && !value.contains('/')
                && !value.contains('(')
        })
        .map(std::string::ToString::to_string)?;

    let mut out = Vec::new();
    for t in toks.iter().skip(list_pos + 1) {
        if let Some(s) = t.strip_prefix('"').and_then(|x| x.strip_suffix('"'))
            && !s.is_empty()
        {
            out.push(s.to_string());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some((var, out))
    }
}

pub(crate) fn parse_get_binding(text: &str) -> Option<(String, GetBinding)> {
    // Patterns like:
    //   $x := get $.Values.foo.bar $k
    //   $x = get $.Values.foo $k
    let toks: Vec<&str> = text.split_whitespace().collect();
    let get_pos = toks.iter().position(|t| *t == "get")?;
    if get_pos < 2 {
        return None;
    }
    if get_pos + 2 >= toks.len() {
        return None;
    }

    let op = toks.get(get_pos.checked_sub(1)?)?;
    if *op != ":=" && *op != "=" {
        return None;
    }

    let var_tok = toks.get(get_pos.checked_sub(2)?)?;
    let var = var_tok.strip_prefix('$')?.to_string();

    let base_tok = toks.get(get_pos + 1)?;
    let base = base_tok
        .strip_prefix("$.Values.")
        .or_else(|| base_tok.strip_prefix(".Values."))?
        .to_string();

    let key_tok = toks.get(get_pos + 2)?;
    let key_var = key_tok.strip_prefix('$')?.to_string();
    Some((var, GetBinding { base, key_var }))
}

fn eq_literals_for_var(text: &str, var: &str) -> Vec<String> {
    let needle = format!("eq ${var} \"");
    let mut literals = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(&needle) {
        let after = &rest[(i + needle.len())..];
        if let Some(end) = after.find('"') {
            let lit = &after[..end];
            if !lit.is_empty() {
                literals.push(lit.to_string());
            }
            rest = &after[end..];
        } else {
            break;
        }
    }
    literals
}

pub(crate) fn extract_bound_values(
    text: &str,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // Track `$var.field[.subfield]...` reads for templates that bind
    // `$var := get $.Values.someMap $key` inside a known key-domain range.
    for tok in text.split_whitespace() {
        let Some(tok) = tok.strip_prefix('$') else {
            continue;
        };
        let Some((var, rest)) = tok.split_once('.') else {
            continue;
        };

        let rest = rest
            .trim_end_matches(',')
            .trim_end_matches(')')
            .trim_end_matches('}')
            .trim_end_matches('|');

        let Some(binding) = get_bindings.get(var) else {
            continue;
        };
        let Some(domain) = range_domains.get(&binding.key_var) else {
            continue;
        };

        let mut skip_literals: HashSet<String> = HashSet::new();
        if rest == "enabled" && binding.base == "config" {
            for lit in eq_literals_for_var(text, &binding.key_var) {
                skip_literals.insert(lit);
            }
        }
        for value in domain {
            if skip_literals.contains(value) {
                continue;
            }
            out.push(format!("{}.{}.{}", binding.base, value, rest));
        }
    }

    out.sort();
    out.dedup();
    out
}

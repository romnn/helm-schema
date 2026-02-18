/// Simple, robust scanner for Go-template blocks in Helm files.
/// Recognizes `{{ ... }}`, `{{- ... -}}` and keeps byte ranges.
/// Inside a block it respects `"..."` and `` `...` `` strings so `}}` in
/// literals don't terminate the block.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GtmplBlock {
    pub start: usize,       // byte offset of '{{' (or '{{-')
    pub end: usize,         // byte offset AFTER '}}' (or '-}}')
    pub inner_start: usize, // first byte of the inner expression
    pub inner_end: usize,   // first byte AFTER the inner expr
}

pub fn scan_gotmpl_blocks(src: &str) -> Vec<GtmplBlock> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i + 1 < b.len() {
        if b[i] == b'{' && b[i + 1] == b'{' {
            let start = i;
            i += 2;
            if i < b.len() && b[i] == b'-' {
                i += 1;
            }
            while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
                i += 1;
            }
            let inner_start = i;

            // scan to matching '}}' respecting strings
            let mut dq = false; // "
            let mut bt = false; // `
            let mut esc = false;
            while i + 1 < b.len() {
                let c = b[i];
                if bt {
                    if c == b'`' {
                        bt = false;
                    }
                    i += 1;
                    continue;
                }
                if dq {
                    if esc {
                        esc = false;
                        i += 1;
                        continue;
                    }
                    if c == b'\\' {
                        esc = true;
                        i += 1;
                        continue;
                    }
                    if c == b'"' {
                        dq = false;
                        i += 1;
                        continue;
                    }
                    i += 1;
                    continue;
                }
                // outside strings
                if c == b'`' {
                    bt = true;
                    i += 1;
                    continue;
                }
                if c == b'"' {
                    dq = true;
                    i += 1;
                    continue;
                }

                if c == b'}' && b[i + 1] == b'}' {
                    let mut end = i + 2;
                    // allow optional leading spaces before closing and optional '-}}'
                    // (we already are at '}}', but support '-}}' forms scanned as ... '-' '}}')
                    // handle trim marker BEFORE the braces
                    // e.g. '{{ ... -}}'
                    // we won't backtrack; enough to cover ' -}}' by ignoring inner_end whitespace
                    let inner_end = i;
                    out.push(GtmplBlock {
                        start,
                        end,
                        inner_start,
                        inner_end,
                    });
                    i = end;
                    break;
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out
}

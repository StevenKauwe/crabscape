//! Normalise a Rust source file before it is fingerprinted, so that only real
//! code changes alter the settled seed's fingerprint: comments (line, block,
//! doc), whitespace runs and `#[cfg(test)]` items do not count. Shared by
//! `build.rs` (via `#[path]`) and `tests/seed_fingerprint.rs`. A pure
//! function of its input.

/// The end (exclusive) of a string, raw string or char literal starting at `i`,
/// or `None` if `b[i]` does not start one (a lifetime is not a literal).
fn literal_end(b: &[u8], i: usize) -> Option<usize> {
    let n = b.len();
    let quoted = |mut j: usize| -> usize {
        // b[j] == '"'
        j += 1;
        while j < n {
            match b[j] {
                b'\\' => j += 2,
                b'"' => return j + 1,
                _ => j += 1,
            }
        }
        n
    };
    match b[i] {
        b'"' => Some(quoted(i)),
        b'b' if b.get(i + 1) == Some(&b'"') => Some(quoted(i + 1)),
        b'r' | b'b' => {
            let mut j = i + usize::from(b[i] == b'b');
            if b.get(j) != Some(&b'r') {
                return None;
            }
            j += 1;
            let hashes = b[j..].iter().take_while(|&&c| c == b'#').count();
            j += hashes;
            if b.get(j) != Some(&b'"') {
                return None;
            }
            j += 1;
            while j < n {
                if b[j] == b'"' && b[j + 1..].iter().take(hashes).all(|&c| c == b'#') {
                    return Some((j + 1 + hashes).min(n));
                }
                j += 1;
            }
            Some(n)
        }
        b'\'' => {
            if b.get(i + 1) == Some(&b'\\') {
                let mut j = i + 2;
                while j < n && b[j] != b'\'' {
                    j += 1;
                }
                Some((j + 1).min(n))
            } else {
                // 'x' (one char, possibly multi-byte UTF-8) is a literal; 'a is a lifetime.
                let mut j = i + 1;
                if j < n {
                    j += 1;
                    while j < n && (b[j] & 0xC0) == 0x80 {
                        j += 1;
                    }
                }
                (b.get(j) == Some(&b'\'')).then_some(j + 1)
            }
        }
        _ => None,
    }
}

/// Is `b[i]` the start of a word (so a leading `r`/`b` is a prefix, not the
/// tail of an identifier)?
fn at_word_start(b: &[u8], i: usize) -> bool {
    i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')
}

/// Drop comments and collapse whitespace, keeping literals verbatim.
fn strip_comments(src: &str) -> Vec<u8> {
    let b = src.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let mut space = false;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            space = true;
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            let mut depth = 1;
            i += 2;
            while i < b.len() && depth > 0 {
                if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            space = true;
        } else if c.is_ascii_whitespace() {
            space = true;
            i += 1;
        } else {
            if space && !out.is_empty() {
                out.push(b' ');
            }
            space = false;
            let lit =
                if matches!(c, b'"' | b'\'') || (matches!(c, b'r' | b'b') && at_word_start(b, i)) {
                    literal_end(b, i)
                } else {
                    None
                };
            match lit {
                Some(end) => {
                    out.extend_from_slice(&b[i..end]);
                    i = end;
                }
                None => {
                    out.push(c);
                    i += 1;
                }
            }
        }
    }
    out
}

/// Remove every `#[cfg(test)]` item (the attribute and what it covers).
fn drop_cfg_test(b: &[u8]) -> Vec<u8> {
    const ATTR: &[u8] = b"#[cfg(test)]";
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'#' && b[i..].starts_with(ATTR) {
            i = item_end(b, i + ATTR.len());
            continue;
        }
        let lit = if matches!(b[i], b'"' | b'\'')
            || (matches!(b[i], b'r' | b'b') && at_word_start(b, i))
        {
            literal_end(b, i)
        } else {
            None
        };
        match lit {
            Some(end) => {
                out.extend_from_slice(&b[i..end]);
                i = end;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    out
}

/// Where the item starting at `i` ends (exclusive): at a `;` or `,` at depth
/// zero, or at the `}` that closes a block opened at depth zero, unless a
/// `;`, `,`, `.` or `)` follows it (a struct literal or call).
fn item_end(b: &[u8], mut i: usize) -> usize {
    let mut depth = 0i32;
    while i < b.len() {
        if matches!(b[i], b'"' | b'\'' | b'r' | b'b')
            && at_word_start(b, i)
            && let Some(end) = literal_end(b, i)
        {
            i = end;
            continue;
        }
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' => depth -= 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let rest = b[i + 1..].iter().find(|c| **c != b' ');
                    if !matches!(rest, Some(b';' | b',' | b'.' | b')')) {
                        return i + 1;
                    }
                }
            }
            b';' | b',' if depth == 0 => return i + 1,
            _ => {}
        }
        if depth < 0 {
            return i;
        }
        i += 1;
    }
    b.len()
}

/// The source as the fingerprint sees it.
pub fn normalise(src: &str) -> Vec<u8> {
    let unix = src.replace("\r\n", "\n");
    drop_cfg_test(&strip_comments(&unix))
}

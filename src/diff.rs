//! Unified diff parsing. Every line number that reaches the tool output is
//! computed here from hunk headers, never by a model.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    /// Line number in the old file (context and removed lines).
    pub old_no: Option<u32>,
    /// Line number in the new file (context and added lines).
    pub new_no: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_len: u32,
    pub new_start: u32,
    pub new_len: u32,
    pub lines: Vec<DiffLine>,
}

impl Hunk {
    /// New-file line numbers of added lines.
    pub fn added_lines(&self) -> impl Iterator<Item = u32> + '_ {
        self.lines
            .iter()
            .filter(|l| l.kind == LineKind::Added)
            .filter_map(|l| l.new_no)
    }

    /// The new-file line range this hunk touches, as an inclusive pair. For a
    /// pure deletion this is the (possibly empty) position where lines were
    /// removed, clamped to at least line 1.
    pub fn new_range(&self) -> (u32, u32) {
        let start = self.new_start.max(1);
        let end = if self.new_len == 0 {
            start
        } else {
            self.new_start + self.new_len - 1
        };
        (start, end.max(start))
    }

    pub fn has_removed(&self) -> bool {
        self.lines.iter().any(|l| l.kind == LineKind::Removed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub status: FileStatus,
    pub is_binary: bool,
    pub hunks: Vec<Hunk>,
}

impl FileDiff {
    /// The path to report: the new path, or the old one for deletions.
    pub fn path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or("")
    }
}

/// Parse `git diff` output produced with `--no-color --no-ext-diff
/// --src-prefix=a/ --dst-prefix=b/`. Unknown lines are ignored rather than
/// treated as errors: the parser must never panic on odd input.
pub fn parse(input: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut cur: Option<FileDiff> = None;
    let mut hunk: Option<Hunk> = None;
    let mut old_no = 0u32;
    let mut new_no = 0u32;

    let flush_hunk = |cur: &mut Option<FileDiff>, hunk: &mut Option<Hunk>| {
        if let (Some(f), Some(h)) = (cur.as_mut(), hunk.take()) {
            f.hunks.push(h);
        }
    };

    for raw in input.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);

        if let Some(rest) = line.strip_prefix("diff --git ") {
            flush_hunk(&mut cur, &mut hunk);
            if let Some(f) = cur.take() {
                files.push(f);
            }
            let (old, new) = split_git_header(rest);
            cur = Some(FileDiff {
                old_path: old,
                new_path: new,
                status: FileStatus::Modified,
                is_binary: false,
                hunks: Vec::new(),
            });
            continue;
        }

        if cur.is_none() {
            continue;
        }

        // Inside a hunk, body lines take priority over header detection so a
        // removed line that happens to start with "-- " is not misread.
        if let Some(h) = hunk.as_mut() {
            let in_body = old_no < h.old_start + h.old_len || new_no < h.new_start + h.new_len;
            if in_body {
                if let Some(t) = line.strip_prefix('+') {
                    h.lines.push(DiffLine {
                        kind: LineKind::Added,
                        old_no: None,
                        new_no: Some(new_no),
                        text: t.to_string(),
                    });
                    new_no += 1;
                    continue;
                } else if let Some(t) = line.strip_prefix('-') {
                    h.lines.push(DiffLine {
                        kind: LineKind::Removed,
                        old_no: Some(old_no),
                        new_no: None,
                        text: t.to_string(),
                    });
                    old_no += 1;
                    continue;
                } else if let Some(t) = line.strip_prefix(' ') {
                    h.lines.push(DiffLine {
                        kind: LineKind::Context,
                        old_no: Some(old_no),
                        new_no: Some(new_no),
                        text: t.to_string(),
                    });
                    old_no += 1;
                    new_no += 1;
                    continue;
                } else if line.is_empty() {
                    // Some tools strip the leading space of empty context lines.
                    h.lines.push(DiffLine {
                        kind: LineKind::Context,
                        old_no: Some(old_no),
                        new_no: Some(new_no),
                        text: String::new(),
                    });
                    old_no += 1;
                    new_no += 1;
                    continue;
                }
            }
            if line.starts_with('\\') {
                // "\ No newline at end of file"
                continue;
            }
        }

        if let Some(rest) = line.strip_prefix("@@ ") {
            flush_hunk(&mut cur, &mut hunk);
            if let Some((os, ol, ns, nl)) = parse_hunk_header(rest) {
                old_no = os;
                new_no = ns;
                hunk = Some(Hunk {
                    old_start: os,
                    old_len: ol,
                    new_start: ns,
                    new_len: nl,
                    lines: Vec::new(),
                });
            }
            continue;
        }

        let Some(file) = cur.as_mut() else { continue };
        if line.starts_with("new file mode") {
            file.status = FileStatus::Added;
            file.old_path = None;
        } else if line.starts_with("deleted file mode") {
            file.status = FileStatus::Deleted;
            file.new_path = None;
        } else if let Some(p) = line.strip_prefix("rename from ") {
            file.status = FileStatus::Renamed;
            file.old_path = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("rename to ") {
            file.status = FileStatus::Renamed;
            file.new_path = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("copy from ") {
            file.status = FileStatus::Copied;
            file.old_path = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("copy to ") {
            file.status = FileStatus::Copied;
            file.new_path = Some(unquote(p));
        } else if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
            file.is_binary = true;
        } else if let Some(p) = line.strip_prefix("--- ") {
            if p == "/dev/null" {
                file.old_path = None;
                file.status = FileStatus::Added;
            } else if let Some(s) = strip_side(p, "a/") {
                file.old_path = Some(s);
            }
        } else if let Some(p) = line.strip_prefix("+++ ") {
            if p == "/dev/null" {
                file.new_path = None;
                file.status = FileStatus::Deleted;
            } else if let Some(s) = strip_side(p, "b/") {
                file.new_path = Some(s);
            }
        }
    }
    flush_hunk(&mut cur, &mut hunk);
    if let Some(f) = cur.take() {
        files.push(f);
    }
    files
}

fn strip_side(p: &str, prefix: &str) -> Option<String> {
    let p = unquote(p);
    p.strip_prefix(prefix).map(str::to_string)
}

/// Split the `a/x b/y` part of a `diff --git` header. Paths may contain
/// spaces, so prefer the symmetric split; the `---`/`+++` and rename lines
/// that follow correct it whenever it is ambiguous.
fn split_git_header(rest: &str) -> (Option<String>, Option<String>) {
    if rest.starts_with('"') {
        // Quoted paths: "a/x y" "b/x y"
        let mut parts = Vec::new();
        let mut chars = rest.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            if c == '"' {
                let start = i;
                let mut end = rest.len();
                let mut escaped = false;
                for (j, d) in chars.by_ref() {
                    if escaped {
                        escaped = false;
                    } else if d == '\\' {
                        escaped = true;
                    } else if d == '"' {
                        end = j + 1;
                        break;
                    }
                }
                parts.push(unquote(&rest[start..end]));
            } else if c != ' ' {
                let tail: String = std::iter::once(c)
                    .chain(chars.by_ref().map(|(_, c)| c))
                    .collect();
                parts.push(tail);
            }
        }
        let a = parts
            .first()
            .and_then(|p| p.strip_prefix("a/").map(str::to_string));
        let b = parts
            .get(1)
            .and_then(|p| p.strip_prefix("b/").map(str::to_string));
        return (a, b);
    }
    // Symmetric split: "a/<p> b/<p>" where both halves are equal length.
    let bytes = rest.len();
    if bytes >= 5 && (bytes - 1).is_multiple_of(2) {
        let half = (bytes - 1) / 2;
        if rest.is_char_boundary(half) && rest.as_bytes().get(half) == Some(&b' ') {
            let (a, b) = (&rest[..half], &rest[half + 1..]);
            if let (Some(a), Some(b)) = (a.strip_prefix("a/"), b.strip_prefix("b/")) {
                return (Some(a.to_string()), Some(b.to_string()));
            }
        }
    }
    match rest.find(" b/") {
        Some(i) => (
            rest[..i].strip_prefix("a/").map(str::to_string),
            Some(rest[i + 3..].to_string()),
        ),
        None => (None, None),
    }
}

/// Undo git's C-style path quoting (`"a/sp\303\244ce"`).
pub fn unquote(p: &str) -> String {
    let Some(inner) = p.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return p.to_string();
    };
    let mut out: Vec<u8> = Vec::with_capacity(inner.len());
    let b = inner.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            let c = b[i + 1];
            match c {
                b'n' => out.push(b'\n'),
                b't' => out.push(b'\t'),
                b'"' => out.push(b'"'),
                b'\\' => out.push(b'\\'),
                b'0'..=b'7' if i + 4 <= b.len() => {
                    let oct = std::str::from_utf8(&b[i + 1..i + 4]).unwrap_or("x");
                    match u8::from_str_radix(oct, 8) {
                        Ok(v) => {
                            out.push(v);
                            i += 4;
                            continue;
                        }
                        Err(_) => out.push(c),
                    }
                }
                other => out.push(other),
            }
            i += 2;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse `-a,b +c,d @@ ...`.
fn parse_hunk_header(rest: &str) -> Option<(u32, u32, u32, u32)> {
    let mut it = rest.split_whitespace();
    let old = it.next()?.strip_prefix('-')?;
    let new = it.next()?.strip_prefix('+')?;
    let (os, ol) = parse_range(old)?;
    let (ns, nl) = parse_range(new)?;
    Some((os, ol, ns, nl))
}

fn parse_range(s: &str) -> Option<(u32, u32)> {
    match s.split_once(',') {
        Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
        None => Some((s.parse().ok()?, 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunk_header_forms() {
        assert_eq!(parse_hunk_header("-1,3 +1,4 @@ fn x()"), Some((1, 3, 1, 4)));
        assert_eq!(parse_hunk_header("-5 +5 @@"), Some((5, 1, 5, 1)));
        assert_eq!(parse_hunk_header("-0,0 +1,2 @@"), Some((0, 0, 1, 2)));
        assert_eq!(parse_hunk_header("garbage"), None);
    }

    #[test]
    fn unquote_octal() {
        assert_eq!(unquote("\"a/sp\\303\\244ce.rs\""), "a/späce.rs");
        assert_eq!(unquote("plain.rs"), "plain.rs");
        assert_eq!(unquote("\"a\\\"b\""), "a\"b");
    }
}

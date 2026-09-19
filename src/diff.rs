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
    let mut c = Cursor::default();
    for raw in input.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = line.strip_prefix("diff --git ") {
            c.flush_file();
            let (old_path, new_path) = split_git_header(rest);
            c.cur = Some(FileDiff {
                old_path,
                new_path,
                status: FileStatus::Modified,
                is_binary: false,
                hunks: Vec::new(),
            });
        } else if c.cur.is_none() || c.body_line(line) {
            // Outside any file, or consumed as a hunk body line. Body lines
            // take priority so a removed line starting with "-- " is not
            // misread as a header.
        } else if let Some(rest) = line.strip_prefix("@@ ") {
            c.start_hunk(rest);
        } else if let Some(file) = c.cur.as_mut() {
            apply_header(file, line);
        }
    }
    c.flush_file();
    c.files
}

/// Parser state: finished files, the file and hunk being read, and the next
/// old/new line numbers.
#[derive(Default)]
struct Cursor {
    files: Vec<FileDiff>,
    cur: Option<FileDiff>,
    hunk: Option<Hunk>,
    old_no: u32,
    new_no: u32,
}

impl Cursor {
    fn flush_hunk(&mut self) {
        if let (Some(f), Some(h)) = (self.cur.as_mut(), self.hunk.take()) {
            f.hunks.push(h);
        }
    }

    fn flush_file(&mut self) {
        self.flush_hunk();
        if let Some(f) = self.cur.take() {
            self.files.push(f);
        }
    }

    fn start_hunk(&mut self, header: &str) {
        self.flush_hunk();
        if let Some((os, ol, ns, nl)) = parse_hunk_header(header) {
            self.old_no = os;
            self.new_no = ns;
            self.hunk = Some(Hunk {
                old_start: os,
                old_len: ol,
                new_start: ns,
                new_len: nl,
                lines: Vec::new(),
            });
        }
    }

    /// Consume a hunk body line. Returns false if the line is not part of
    /// the current hunk's body.
    fn body_line(&mut self, line: &str) -> bool {
        let Some(h) = self.hunk.as_mut() else {
            return false;
        };
        if line.starts_with('\\') {
            // "\ No newline at end of file"
            return true;
        }
        let in_body =
            self.old_no < h.old_start + h.old_len || self.new_no < h.new_start + h.new_len;
        if !in_body {
            return false;
        }
        let (kind, text) = if let Some(t) = line.strip_prefix('+') {
            (LineKind::Added, t)
        } else if let Some(t) = line.strip_prefix('-') {
            (LineKind::Removed, t)
        } else if let Some(t) = line.strip_prefix(' ') {
            (LineKind::Context, t)
        } else if line.is_empty() {
            // Some tools strip the leading space of empty context lines.
            (LineKind::Context, "")
        } else {
            return false;
        };
        let old_no = (kind != LineKind::Added).then_some(self.old_no);
        let new_no = (kind != LineKind::Removed).then_some(self.new_no);
        h.lines.push(DiffLine {
            kind,
            old_no,
            new_no,
            text: text.to_string(),
        });
        self.old_no += u32::from(old_no.is_some());
        self.new_no += u32::from(new_no.is_some());
        true
    }
}

/// Apply an extended header line (`new file mode`, `rename from`, `---`, ...).
fn apply_header(file: &mut FileDiff, line: &str) {
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
        apply_side(file, p, true);
    } else if let Some(p) = line.strip_prefix("+++ ") {
        apply_side(file, p, false);
    }
}

fn apply_side(file: &mut FileDiff, p: &str, old: bool) {
    match (p == "/dev/null", old) {
        (true, true) => {
            file.old_path = None;
            file.status = FileStatus::Added;
        }
        (true, false) => {
            file.new_path = None;
            file.status = FileStatus::Deleted;
        }
        (false, true) => {
            if let Some(s) = strip_side(p, "a/") {
                file.old_path = Some(s);
            }
        }
        (false, false) => {
            if let Some(s) = strip_side(p, "b/") {
                file.new_path = Some(s);
            }
        }
    }
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
        // Quoted paths: "a/x y" "b/x y" (the second may be unquoted).
        let first_len = quoted_len(rest);
        let first = unquote(&rest[..first_len]);
        let second = rest[first_len..].trim_start();
        let second = if second.starts_with('"') {
            unquote(&second[..quoted_len(second)])
        } else {
            second.to_string()
        };
        return (
            first.strip_prefix("a/").map(str::to_string),
            second.strip_prefix("b/").map(str::to_string),
        );
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

/// Byte length of the quoted string at the start of `s` (which begins with
/// `"`), including both quotes; the whole string if it is unterminated.
fn quoted_len(s: &str) -> usize {
    let mut escaped = false;
    for (j, d) in s.char_indices().skip(1) {
        match (escaped, d) {
            (true, _) => escaped = false,
            (false, '\\') => escaped = true,
            (false, '"') => return j + 1,
            _ => {}
        }
    }
    s.len()
}

/// Undo git's C-style path quoting (`"a/sp\303\244ce"`).
pub fn unquote(p: &str) -> String {
    let Some(inner) = p.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return p.to_string();
    };
    let mut out: Vec<u8> = Vec::with_capacity(inner.len());
    let b = inner.as_bytes();
    let mut i = 0;
    while let Some(&byte) = b.get(i) {
        let Some(&c) = b.get(i + 1).filter(|_| byte == b'\\') else {
            out.push(byte);
            i += 1;
            continue;
        };
        let octal = b
            .get(i + 1..i + 4)
            .and_then(|d| std::str::from_utf8(d).ok())
            .and_then(|d| u8::from_str_radix(d, 8).ok());
        match (c, octal) {
            (b'0'..=b'7', Some(v)) => {
                out.push(v);
                i += 4;
                continue;
            }
            (b'n', _) => out.push(b'\n'),
            (b't', _) => out.push(b'\t'),
            (other, _) => out.push(other),
        }
        i += 2;
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
    fn quoted_headers() {
        assert_eq!(
            split_git_header("\"a/sp ace.rs\" \"b/sp ace.rs\""),
            (Some("sp ace.rs".into()), Some("sp ace.rs".into()))
        );
        assert_eq!(
            split_git_header("a/x.rs b/x.rs"),
            (Some("x.rs".into()), Some("x.rs".into()))
        );
    }

    #[test]
    fn unquote_octal() {
        assert_eq!(unquote("\"a/sp\\303\\244ce.rs\""), "a/späce.rs");
        assert_eq!(unquote("plain.rs"), "plain.rs");
        assert_eq!(unquote("\"a\\\"b\""), "a\"b");
    }
}

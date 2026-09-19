//! Turn a parsed diff plus file contents into small, relevant evaluation
//! units: the changed lines, expanded to their enclosing items.

use crate::diff::{FileDiff, Hunk, LineKind};
use crate::questions::UnitKind;
use crate::redact::{self, Redactions};
use crate::rust_project::Role;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Lines of context around a change that has no enclosing item.
const WINDOW: u32 = 6;
/// Lines around each hunk when an enclosing item is too large.
const HUNK_WINDOW: u32 = 15;
/// Cap on the `imports` field.
const MAX_IMPORT_LINES: usize = 40;

#[derive(Debug, Clone, Serialize)]
pub struct Unit {
    pub id: String,
    pub file: String,
    #[serde(skip)]
    pub kind: UnitKind,
    pub role: Role,
    /// Inclusive new-file line range covered by `code`.
    pub lines: (u32, u32),
    /// Inclusive new-file ranges of added lines inside the unit.
    pub changed_lines: Vec<(u32, u32)>,
    pub has_removed_lines: bool,
    #[serde(skip)]
    pub code: String,
    #[serde(skip)]
    pub imports: String,
    #[serde(skip)]
    pub enclosing: Option<String>,
    /// True when an unchanged file is reviewed whole (Path scope).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub whole_file: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Skip {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<(u32, u32)>,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct ItemSpan {
    pub start: u32,
    pub end: u32,
    /// The enclosing `impl`/`trait`/`mod` line, trimmed before `{`.
    pub header: Option<String>,
}

/// Leaf item spans (fns, methods, structs, ...) with their enclosing
/// impl/trait/mod header, from a syn parse. `None` if the file does not
/// parse, in which case callers fall back to line windows.
pub fn item_spans(src: &str) -> Option<Vec<ItemSpan>> {
    use syn::spanned::Spanned;
    let file = syn::parse_file(src).ok()?;
    let lines: Vec<&str> = src.lines().collect();
    let header_of = |start: usize| -> String {
        let l = lines.get(start.saturating_sub(1)).copied().unwrap_or("");
        l.split('{').next().unwrap_or(l).trim().to_string()
    };
    let mut out = Vec::new();
    fn span_lines<T: Spanned>(t: &T) -> (u32, u32) {
        let s = t.span();
        (s.start().line as u32, s.end().line as u32)
    }
    fn walk(
        items: &[syn::Item],
        header: Option<String>,
        out: &mut Vec<ItemSpan>,
        header_of: &dyn Fn(usize) -> String,
    ) {
        for item in items {
            match item {
                syn::Item::Impl(i) => {
                    let (s, e) = span_lines(i);
                    // Header is the line holding `impl`, skipping attributes.
                    let h = header_of(span_lines(&i.impl_token).0 as usize);
                    let before = out.len();
                    for ii in &i.items {
                        let (a, b) = span_lines(ii);
                        out.push(ItemSpan {
                            start: a,
                            end: b,
                            header: Some(h.clone()),
                        });
                    }
                    if out.len() == before {
                        out.push(ItemSpan {
                            start: s,
                            end: e,
                            header: header.clone(),
                        });
                    }
                }
                syn::Item::Trait(t) => {
                    let h = header_of(span_lines(&t.trait_token).0 as usize);
                    let (s, e) = span_lines(t);
                    let before = out.len();
                    for ti in &t.items {
                        let (a, b) = span_lines(ti);
                        out.push(ItemSpan {
                            start: a,
                            end: b,
                            header: Some(h.clone()),
                        });
                    }
                    if out.len() == before {
                        out.push(ItemSpan {
                            start: s,
                            end: e,
                            header: header.clone(),
                        });
                    }
                }
                syn::Item::Mod(m) if m.content.is_some() => {
                    let h = header_of(span_lines(&m.mod_token).0 as usize);
                    if let Some((_, inner)) = &m.content {
                        walk(inner, Some(h), out, header_of);
                    }
                }
                syn::Item::Use(_) => {}
                other => {
                    let (s, e) = span_lines(other);
                    out.push(ItemSpan {
                        start: s,
                        end: e,
                        header: header.clone(),
                    });
                }
            }
        }
    }
    walk(&file.items, None, &mut out, &header_of);
    Some(out)
}

/// Top-level `use` lines, joined.
fn imports(src: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut in_use = false;
    for l in src.lines() {
        let t = l.trim_start();
        let starts =
            t.starts_with("use ") || t.starts_with("pub use ") || t.starts_with("pub(crate) use ");
        if (starts && l.len() - t.len() == 0) || in_use {
            out.push(l);
            in_use = !l.trim_end().ends_with(';');
            if out.len() >= MAX_IMPORT_LINES {
                break;
            }
        }
    }
    out.join("\n")
}

/// Map removed lines to the new-file line they sit before.
pub fn removed_anchors(hunks: &[Hunk]) -> BTreeMap<u32, Vec<String>> {
    let mut m: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for h in hunks {
        let mut pending: Vec<String> = Vec::new();
        for l in &h.lines {
            match l.kind {
                LineKind::Removed => pending.push(l.text.clone()),
                _ => {
                    if !pending.is_empty()
                        && let Some(n) = l.new_no
                    {
                        m.entry(n).or_default().append(&mut pending);
                    }
                }
            }
        }
        if !pending.is_empty() {
            m.entry(h.new_start + h.new_len)
                .or_default()
                .append(&mut pending);
        }
    }
    m
}

fn to_ranges(lines: &BTreeSet<u32>) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &l in lines {
        match out.last_mut() {
            Some((_, e)) if *e + 1 == l => *e = l,
            _ => out.push((l, l)),
        }
    }
    out
}

fn merge(mut ranges: Vec<(u32, u32)>, gap: u32) -> Vec<(u32, u32)> {
    ranges.sort();
    let mut out: Vec<(u32, u32)> = Vec::new();
    for (s, e) in ranges {
        match out.last_mut() {
            Some((_, pe)) if s <= *pe + gap + 1 => *pe = (*pe).max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

pub struct FileInput<'a> {
    pub diff: &'a FileDiff,
    pub content: &'a str,
    pub role: Role,
    /// Line ranges of inline test code (`#[cfg(test)]`, `#[test]`).
    pub test_ranges: &'a [(u32, u32)],
    pub whole_file: bool,
}

/// Build the units for one Rust file.
pub fn rust_units(
    input: &FileInput,
    max_unit_tokens: usize,
    next_id: &mut usize,
    redactions: &mut Redactions,
    skipped: &mut Vec<Skip>,
) -> Vec<Unit> {
    let file = input.diff.path().to_string();
    let lines: Vec<&str> = input.content.split('\n').collect();
    let n_lines = lines.len().max(1) as u32;
    let added: BTreeSet<u32> = input
        .diff
        .hunks
        .iter()
        .flat_map(|h| h.added_lines())
        .collect();
    let anchors = removed_anchors(&input.diff.hunks);

    // Touch points: non-blank added lines, plus the position of pure
    // removals. A blank added line carries nothing to review on its own.
    let is_blank = |n: u32| {
        lines
            .get(n as usize - 1)
            .is_none_or(|l| l.trim().is_empty())
    };
    let mut touch: BTreeSet<u32> = added.iter().copied().filter(|&n| !is_blank(n)).collect();
    for &a in anchors.keys() {
        touch.insert(a.clamp(1, n_lines));
    }
    if touch.is_empty() {
        return Vec::new();
    }

    let spans = item_spans(input.content);
    let mut regions: Vec<(u32, u32)> = Vec::new();
    let mut headers: BTreeMap<(u32, u32), String> = BTreeMap::new();
    for &t in &touch {
        let leaf = spans.as_ref().and_then(|sp| {
            sp.iter()
                .filter(|s| s.start <= t && t <= s.end)
                .min_by_key(|s| s.end - s.start)
        });
        match leaf {
            Some(s) => {
                regions.push((s.start, s.end));
                if let Some(h) = &s.header {
                    headers.insert((s.start, s.end), h.clone());
                }
            }
            None => {
                // Outside any item (imports, attributes) a parsed file needs
                // little context; without a parse, take more.
                let w = if spans.is_some() { 2 } else { WINDOW };
                regions.push((t.saturating_sub(w).max(1), (t + w).min(n_lines)))
            }
        }
    }
    let regions = merge(regions, 1);

    let imports_text = redact::redact_text(&imports(input.content), redactions);
    let mut units = Vec::new();
    for (rs, re) in regions {
        // Split oversized regions into windows around their touch points.
        let pieces = if est_tokens(&render(&lines, rs, re, &added, &anchors)) > max_unit_tokens {
            let windows: Vec<(u32, u32)> = touch
                .iter()
                .filter(|&&t| rs <= t && t <= re)
                .map(|&t| {
                    (
                        t.saturating_sub(HUNK_WINDOW).max(rs),
                        (t + HUNK_WINDOW).min(re),
                    )
                })
                .collect();
            let mut out = Vec::new();
            for (ws, we) in merge(windows, 0) {
                out.extend(hard_split(
                    &lines,
                    ws,
                    we,
                    &added,
                    &anchors,
                    max_unit_tokens,
                ));
            }
            out
        } else {
            vec![(rs, re)]
        };
        let enclosing = headers
            .iter()
            .find(|((s, e), _)| *s <= rs && re <= *e || (rs <= *s && *e <= re))
            .map(|(_, h)| h.clone());
        for (s, e) in pieces {
            let text = render(&lines, s, e, &added, &anchors);
            if est_tokens(&text) > max_unit_tokens {
                skipped.push(Skip {
                    file: file.clone(),
                    lines: Some((s, e)),
                    reason: "a single changed line is larger than the unit budget".into(),
                });
                continue;
            }
            let changed: BTreeSet<u32> = added.range(s..=e).copied().collect();
            let has_removed = anchors.range(s..=e + 1).next().is_some();
            if changed.is_empty() && !has_removed {
                continue;
            }
            let role = if input.role == Role::Library || input.role == Role::Binary {
                let mut substantive = changed.iter().filter(|&&l| !is_blank(l)).peekable();
                let all_test = substantive.peek().is_some()
                    && substantive.all(|l| input.test_ranges.iter().any(|(a, b)| a <= l && l <= b));
                if all_test { Role::Test } else { input.role }
            } else {
                input.role
            };
            *next_id += 1;
            units.push(Unit {
                id: format!("u{next_id}"),
                file: file.clone(),
                kind: UnitKind::RustCode,
                role,
                lines: (s, e),
                changed_lines: to_ranges(&changed),
                has_removed_lines: has_removed,
                code: redact::redact_text(&text, redactions),
                imports: imports_text.clone(),
                enclosing: enclosing.clone(),
                whole_file: input.whole_file,
            });
        }
    }
    units
}

/// Split a window into line chunks that each fit the budget.
fn hard_split(
    lines: &[&str],
    s: u32,
    e: u32,
    added: &BTreeSet<u32>,
    anchors: &BTreeMap<u32, Vec<String>>,
    budget: usize,
) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut start = s;
    while start <= e {
        let mut end = start;
        while end < e && est_tokens(&render(lines, start, end + 1, added, anchors)) <= budget {
            end += 1;
        }
        out.push((start, end));
        start = end + 1;
    }
    out
}

/// Render lines `s..=e` diff-style: `+` added, `-` removed, ` ` context.
pub fn render(
    lines: &[&str],
    s: u32,
    e: u32,
    added: &BTreeSet<u32>,
    anchors: &BTreeMap<u32, Vec<String>>,
) -> String {
    let mut out = String::new();
    for n in s..=e {
        if let Some(removed) = anchors.get(&n) {
            for r in removed {
                out.push('-');
                out.push_str(r);
                out.push('\n');
            }
        }
        let text = lines.get(n as usize - 1).copied().unwrap_or("");
        out.push(if added.contains(&n) { '+' } else { ' ' });
        out.push_str(text.strip_suffix('\r').unwrap_or(text));
        out.push('\n');
    }
    // Removals after the last rendered line.
    if let Some(removed) = anchors.get(&(e + 1))
        && e as usize >= lines.len().saturating_sub(1)
    {
        for r in removed {
            out.push('-');
            out.push_str(r);
            out.push('\n');
        }
    }
    out
}

pub fn est_tokens(s: &str) -> usize {
    s.len().div_ceil(3)
}

/// A manifest (`Cargo.toml`) unit: the hunks, rendered diff-style.
pub fn manifest_unit(
    diff: &FileDiff,
    max_unit_tokens: usize,
    next_id: &mut usize,
    redactions: &mut Redactions,
    skipped: &mut Vec<Skip>,
) -> Option<Unit> {
    let mut text = String::new();
    let mut added = BTreeSet::new();
    for h in &diff.hunks {
        text.push_str("...\n");
        for l in &h.lines {
            text.push(match l.kind {
                LineKind::Added => '+',
                LineKind::Removed => '-',
                LineKind::Context => ' ',
            });
            text.push_str(&l.text);
            text.push('\n');
        }
        added.extend(h.added_lines());
    }
    if est_tokens(&text) > max_unit_tokens {
        skipped.push(Skip {
            file: diff.path().into(),
            lines: None,
            reason: "manifest diff larger than the unit budget".into(),
        });
        return None;
    }
    let first = diff.hunks.first().map(|h| h.new_range().0).unwrap_or(1);
    let last = diff.hunks.last().map(|h| h.new_range().1).unwrap_or(1);
    *next_id += 1;
    Some(Unit {
        id: format!("u{next_id}"),
        file: diff.path().into(),
        kind: UnitKind::Manifest,
        role: Role::Library,
        lines: (first, last),
        changed_lines: to_ranges(&added),
        has_removed_lines: diff.hunks.iter().any(|h| h.has_removed()),
        code: redact::redact_text(&text, redactions),
        imports: String::new(),
        enclosing: None,
        whole_file: false,
    })
}

/// Synthesize an all-added diff for a file (untracked or whole-file review).
pub fn synthetic_added(path: &str, content: &str) -> FileDiff {
    use crate::diff::{DiffLine, FileStatus};
    let lines: Vec<&str> = content.split('\n').collect();
    let n = if content.ends_with('\n') {
        lines.len() - 1
    } else {
        lines.len()
    } as u32;
    FileDiff {
        old_path: None,
        new_path: Some(path.to_string()),
        status: FileStatus::Added,
        is_binary: false,
        hunks: vec![Hunk {
            old_start: 0,
            old_len: 0,
            new_start: 1,
            new_len: n,
            lines: (1..=n)
                .map(|i| DiffLine {
                    kind: LineKind::Added,
                    old_no: None,
                    new_no: Some(i),
                    text: lines
                        .get(i as usize - 1)
                        .copied()
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff;

    const SRC: &str = "use std::sync::Mutex;\n\nstruct Cache {\n    m: Mutex<u32>,\n}\n\nimpl Cache {\n    fn a(&self) -> u32 {\n        1\n    }\n\n    fn b(&self) -> u32 {\n        let g = self.m.lock().unwrap();\n        *g + 1\n    }\n}\n";

    fn units_for(diff_text: &str, src: &str, budget: usize) -> (Vec<Unit>, Vec<Skip>) {
        let d = diff::parse(diff_text);
        let mut id = 0;
        let mut r = Redactions::default();
        let mut s = Vec::new();
        let u = rust_units(
            &FileInput {
                diff: &d[0],
                content: src,
                role: Role::Library,
                test_ranges: &[],
                whole_file: false,
            },
            budget,
            &mut id,
            &mut r,
            &mut s,
        );
        (u, s)
    }

    #[test]
    fn expands_to_enclosing_method_with_header_and_imports() {
        let d = "diff --git a/src/c.rs b/src/c.rs\n--- a/src/c.rs\n+++ b/src/c.rs\n@@ -13,2 +13,2 @@\n         let g = self.m.lock().unwrap();\n-        *g\n+        *g + 1\n     }\n";
        let (u, _) = units_for(d, SRC, 6000);
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].lines, (12, 15));
        assert_eq!(u[0].changed_lines, vec![(14, 14)]);
        assert!(u[0].has_removed_lines);
        assert_eq!(u[0].enclosing.as_deref(), Some("impl Cache"));
        assert_eq!(u[0].imports, "use std::sync::Mutex;");
        assert!(
            u[0].code.contains("-        *g\n+        *g + 1\n"),
            "{}",
            u[0].code
        );
        assert!(!u[0].code.contains("fn a("));
    }

    #[test]
    fn falls_back_to_window_on_parse_error() {
        let src = "fn broken( {\n".repeat(30);
        let d = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -15,1 +15,1 @@\n-old\n+fn broken( {\n";
        let (u, _) = units_for(d, &src, 6000);
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].lines, (9, 21));
    }

    #[test]
    fn oversized_item_is_split() {
        let mut src = String::from("fn big() {\n");
        for i in 0..400 {
            src.push_str(&format!("    let v{i} = compute_something_long({i});\n"));
        }
        src.push_str("}\n");
        let d = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -10,1 +10,1 @@\n-old\n+    let v8 = compute_something_long(8);\n@@ -300,1 +300,1 @@\n-old\n+    let v298 = compute_something_long(298);\n";
        let (u, _) = units_for(d, &src, 600);
        assert!(u.len() >= 2, "{}", u.len());
        for unit in &u {
            assert!(est_tokens(&unit.code) <= 600);
        }
        assert!(u.iter().any(|x| x.changed_lines == vec![(10, 10)]));
        assert!(u.iter().any(|x| x.changed_lines == vec![(300, 300)]));
    }

    #[test]
    fn synthetic_added_numbers_lines() {
        let d = synthetic_added("a.rs", "fn a() {}\nfn b() {}\n");
        assert_eq!(d.hunks[0].new_len, 2);
        assert_eq!(d.hunks[0].added_lines().collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn imports_multiline() {
        let src = "use a::{\n    b,\n    c,\n};\nuse d;\nfn x() { use inner; }\n";
        assert_eq!(imports(src), "use a::{\n    b,\n    c,\n};\nuse d;");
    }
}

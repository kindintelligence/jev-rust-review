//! What a claim names that lies outside the excerpt. Jev reads one excerpt,
//! so a claim about a helper in another file used to come back `dismiss`.
//! The definitions a claim names are found here and sent along, and whatever
//! still cannot be shown is reported, so that a non-answer is never read as
//! a refutation.

use crate::context;
use crate::git::{Git, NewSide};
use regex::Regex;
use std::collections::BTreeSet;
use std::sync::LazyLock;

/// Most definitions sent with one finding, and their total size.
const MAX_DEFINITIONS: usize = 4;
const MAX_TOKENS: usize = 1_500;
/// A definition longer than this is cut, with a marker.
const MAX_LINES: usize = 40;

/// Words that are common both in prose and as item names.
const TOO_COMMON: &[&str] = &[
    "new", "get", "set", "run", "from", "into", "main", "test", "default", "the", "and", "for",
    "with", "that", "this", "then", "when", "while", "which",
];

static WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z_][A-Za-z0-9_]{2,}").expect("a fixed pattern"));

/// Text in a claim that is written as code: backticked, a path, snake case,
/// a call, or camel case.
static CODE_LIKE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"`[^`]+`|\b\w+(?:::\w+)+|\b[a-z0-9]+(?:_[a-z0-9]+)+\b|\b\w+\(\)|\b[A-Z][a-z0-9]+(?:[A-Z][a-z0-9]*)+\b",
    )
    .expect("a fixed pattern")
});

/// Every word of the claim that could be an item name.
fn candidate_names(claim: &str) -> BTreeSet<&str> {
    WORD.find_iter(claim)
        .map(|m| m.as_str())
        .filter(|w| !TOO_COMMON.contains(&w.to_ascii_lowercase().as_str()))
        .take(24)
        .collect()
}

/// A definition of something the claim names, from outside the excerpt.
#[derive(Debug, PartialEq, Eq)]
pub struct Definition {
    pub file: String,
    pub start: u32,
    pub text: String,
}

/// Find where the repository defines the items `claim` names, leaving out
/// anything inside `excerpt` (file, first line, last line), which Jev sees
/// already.
pub fn definitions(
    git: &Git,
    side: &NewSide,
    claim: &str,
    excerpt: (&str, u32, u32),
) -> Vec<Definition> {
    let names = candidate_names(claim);
    if names.is_empty() {
        return Vec::new();
    }
    // Names are `[A-Za-z0-9_]` only, so the pattern cannot carry an option
    // or a regex metacharacter. POSIX classes only: macOS git uses the BSD
    // regex engine, which has no `\b`.
    let pattern = format!(
        "(^|[^A-Za-z0-9_])(fn|struct|enum|trait|type|const|static)[[:space:]]+({})([^A-Za-z0-9_]|$)",
        names.into_iter().collect::<Vec<_>>().join("|")
    );
    let mut out: Vec<Definition> = Vec::new();
    let mut budget = MAX_TOKENS;
    for (file, line) in git.grep_rust(side, &pattern) {
        let (ex_file, lo, hi) = excerpt;
        if out.len() >= MAX_DEFINITIONS || (file == ex_file && lo <= line && line <= hi) {
            continue;
        }
        let Ok(Some(content)) = git.read_new(side, &file) else {
            continue;
        };
        let Some(def) = definition_at(&file, &content, line) else {
            continue;
        };
        let cost = context::est_tokens(&def.text);
        if cost > budget
            || out
                .iter()
                .any(|d| d.file == def.file && d.start == def.start)
        {
            continue;
        }
        budget -= cost;
        out.push(def);
    }
    out
}

/// The innermost item around `line`, cut to `MAX_LINES`.
fn definition_at(file: &str, content: &str, line: u32) -> Option<Definition> {
    let spans = context::item_spans(content)?;
    let span = spans
        .iter()
        .filter(|s| s.start <= line && line <= s.end)
        .min_by_key(|s| s.end - s.start)?;
    let lines: Vec<&str> = content
        .lines()
        .skip(span.start as usize - 1)
        .take((span.end - span.start + 1) as usize)
        .collect();
    let mut text = lines
        .iter()
        .take(MAX_LINES)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    if lines.len() > MAX_LINES {
        text.push_str("\n// ... cut");
    }
    Some(Definition {
        file: file.to_string(),
        start: span.start,
        text,
    })
}

/// One string for the request's `related_code` field.
pub fn render(defs: &[Definition]) -> String {
    defs.iter()
        .map(|d| format!("// {}:{}\n{}\n", d.file, d.start, d.text))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The code-like names in `claim` that appear nowhere in what Jev is shown.
/// A claim that rests on one of these cannot be refuted from the excerpt.
pub fn unseen(claim: &str, shown: &str) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for m in CODE_LIKE.find_iter(claim) {
        for part in WORD.find_iter(m.as_str()) {
            if !shown.contains(part.as_str()) {
                out.insert(part.as_str().to_string());
            }
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_written_as_code_are_checked_against_what_jev_sees() {
        let claim = "In relay, forward(&tx, event) is raced in tokio::select!; `forward` awaits Sender::send, so the event is lost when heartbeat_tick wins.";
        let shown = "tokio::select! { _ = heartbeat.tick() => {} () = forward(&tx, event) => {} }";
        // `relay`, `event` and `lost` are prose here; only code-like text counts.
        assert_eq!(unseen(claim, shown), ["Sender", "heartbeat_tick", "send"]);
        let with_helper = format!(
            "{shown}\nasync fn forward(tx: &Sender<Event>) {{ tx.send(e).await }} heartbeat_tick"
        );
        assert_eq!(unseen(claim, &with_helper), Vec::<String>::new());
    }

    #[test]
    fn prose_words_are_not_searched_for() {
        let names =
            candidate_names("The guard is dropped when `refresh` returns from the New state");
        assert!(names.contains("refresh") && names.contains("guard"));
        assert!(!names.contains("the") && !names.contains("from") && !names.contains("New"));
    }

    #[test]
    fn a_definition_is_the_innermost_item_and_is_cut_when_long() {
        let src = "impl Shutdown {\n    pub fn trigger(&self) {\n        self.wake.notify_waiters();\n    }\n}\n";
        let d = definition_at("src/signal.rs", src, 2).expect("an item");
        assert_eq!(d.start, 2);
        assert!(d.text.contains("notify_waiters") && !d.text.contains("impl Shutdown"));
        assert_eq!(
            render(&[d]),
            "// src/signal.rs:2\n    pub fn trigger(&self) {\n        self.wake.notify_waiters();\n    }\n"
        );

        let long = format!("fn big() {{\n{}}}\n", "    step();\n".repeat(60));
        let d = definition_at("a.rs", &long, 1).expect("an item");
        assert!(d.text.ends_with("// ... cut"));
        assert_eq!(d.text.lines().count(), MAX_LINES + 1);
    }
}

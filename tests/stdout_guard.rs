//! stdout carries the MCP protocol. A stray print corrupts the transport, so
//! no source file may write to stdout except the one audited `--version`
//! path in main.rs, which runs before the protocol starts.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test code: failures should panic loudly"
)]

use std::path::Path;
use std::sync::LazyLock;

/// Word-bounded so `eprintln!` (stderr) does not match `println!`.
static FORBIDDEN: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\bprintln!|\bprint!|\bio::stdout\b|\bdbg!").unwrap());

/// String literals, raw or plain.
static STRINGS: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r##"r#*"[^"]*"#*|"(?:[^"\\]|\\.)*""##).unwrap());

/// Blank out string literals so patterns inside them (for example the gate
/// regexes in questions.rs) are not mistaken for code.
fn code_only(line: &str) -> String {
    STRINGS.replace_all(line, "\"\"").into_owned()
}

fn offending(path: &Path, line: &str) -> bool {
    if line.trim_start().starts_with("//") {
        return false;
    }
    let code = code_only(line);
    let hit = FORBIDDEN.is_match(&code);
    let audited = path.ends_with("main.rs") && code.contains("std::io::stdout().lock()");
    hit && !audited
}

/// Every `.rs` file under `dir`, including nested module directories.
fn rust_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    out
}

#[test]
fn no_stdout_writes_in_src() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    let files = rust_files(&src);
    assert!(
        files.iter().any(|f| f.ends_with("review/verify.rs")),
        "the scan must include nested modules"
    );
    for path in files {
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        offenders.extend(
            text.lines()
                .enumerate()
                .filter(|(_, l)| offending(&path, l))
                .map(|(i, l)| format!("{}:{}: {}", path.display(), i + 1, l.trim())),
        );
    }
    assert!(
        offenders.is_empty(),
        "stdout writes found:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn guard_catches_prints_and_ignores_strings() {
    let p = Path::new("src/x.rs");
    assert!(offending(p, "    println!(\"hi\");"));
    assert!(offending(p, "    let o = std::io::stdout();"));
    assert!(!offending(p, r#"    &[r"println!", "print!("],"#));
    assert!(!offending(p, "    // println!(\"commented\")"));
    assert!(!offending(p, "    eprintln!(\"to stderr\");"));
}

#[test]
fn crate_denies_print_stdout_lint() {
    for f in ["src/lib.rs", "src/main.rs"] {
        let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(f)).unwrap();
        assert!(
            text.contains("#![deny(clippy::print_stdout)]"),
            "{f} must deny clippy::print_stdout"
        );
    }
}

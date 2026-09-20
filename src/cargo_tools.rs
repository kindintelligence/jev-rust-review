//! Deterministic tools: Clippy (which includes every rustc diagnostic) and
//! cargo-semver-checks. Their output is fact, so it is collected and
//! filtered here in code and never read by a model as raw text.
//!
//! This is the second audited subprocess door (git.rs is the first). Every
//! argument is built in this file: fixed flags, lint names from
//! [`EXTRA_LINTS`], crate names read from the project's manifests, and a
//! baseline revision that git.rs already resolved to a full SHA.

use crate::rust_project::ProjectInfo;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

/// An off-by-default Clippy lint the review turns on, with its group. The
/// `lints_exist_in_installed_clippy` test checks both against the toolchain.
#[derive(Debug, Clone, Copy)]
pub struct ExtraLint {
    pub name: &'static str,
    pub group: &'static str,
}

/// Each lint answers a defect that was once a Jev question. They are reported
/// on changed lines only, and never where the project allows them.
pub const EXTRA_LINTS: &[ExtraLint] = &[
    ExtraLint::new("cast_possible_truncation", "pedantic"),
    ExtraLint::new("cast_sign_loss", "pedantic"),
    ExtraLint::new("cast_possible_wrap", "pedantic"),
    ExtraLint::new("needless_pass_by_value", "pedantic"),
    ExtraLint::new("redundant_clone", "nursery"),
    ExtraLint::new("non_send_fields_in_send_ty", "nursery"),
    ExtraLint::new("let_underscore_must_use", "restriction"),
    ExtraLint::new("map_err_ignore", "restriction"),
    ExtraLint::new("wildcard_enum_match_arm", "restriction"),
    ExtraLint::new("undocumented_unsafe_blocks", "restriction"),
];

impl ExtraLint {
    const fn new(name: &'static str, group: &'static str) -> ExtraLint {
        ExtraLint { name, group }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Error,
    Warning,
}

/// One compiler or Clippy diagnostic, reduced to what a review needs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Diagnostic {
    /// Repo-relative path with `/` separators.
    pub file: String,
    pub lines: (u32, u32),
    pub level: Level,
    /// `clippy::cast_possible_truncation`, `unused_must_use`, `E0382`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
    /// The other places the diagnostic points at: secondary labels and the
    /// spans of its notes. `await_holding_lock` puts the guard in the primary
    /// span and the `.await` it is held across in a note.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<(String, (u32, u32))>,
    pub on_changed_lines: bool,
}

impl Diagnostic {
    /// Whether any span of the diagnostic touches these lines of the file.
    pub fn overlaps(&self, file: &str, start: u32, end: u32) -> bool {
        std::iter::once((&self.file, self.lines))
            .chain(self.related.iter().map(|(f, l)| (f, *l)))
            .any(|(f, (s, e))| f == file && s <= end && start <= e)
    }
}

/// What the project has already decided about lint levels. Command-line `-W`
/// beats a `[lints]` table (checked against cargo 1.97), so the manifest has
/// to be read here. `#[allow]` attributes need no help: they beat `-W`.
#[derive(Debug, Default)]
pub struct LintPolicy {
    /// Crate dir to lint or group name to level, from `[lints.clippy]` or the
    /// inherited `[workspace.lints.clippy]`.
    by_crate: BTreeMap<String, BTreeMap<String, String>>,
    /// Lints allowed for every crate by `-A` in `.cargo/config.toml`.
    allowed_by_rustflags: BTreeSet<String>,
}

impl LintPolicy {
    pub fn load(root: &Path, project: &ProjectInfo) -> LintPolicy {
        let read = |p: &Path| -> Option<toml::Table> {
            std::fs::read_to_string(p).ok()?.parse::<toml::Table>().ok()
        };
        let workspace = read(&root.join("Cargo.toml"))
            .as_ref()
            .and_then(|m| m.get("workspace")?.get("lints")?.get("clippy"))
            .map(levels)
            .unwrap_or_default();
        let by_crate = project
            .crates
            .iter()
            .map(|c| {
                let manifest = read(&root.join(&c.dir).join("Cargo.toml"));
                let lints = manifest.as_ref().and_then(|m| m.get("lints"));
                let inherits = lints
                    .and_then(|l| l.get("workspace"))
                    .and_then(toml::Value::as_bool)
                    .unwrap_or(false);
                let own = lints.and_then(|l| l.get("clippy")).map(levels);
                let table = if inherits {
                    workspace.clone()
                } else {
                    own.unwrap_or_default()
                };
                (c.dir.clone(), table)
            })
            .collect();
        let allowed_by_rustflags = read(&root.join(".cargo/config.toml"))
            .map(|c| rustflag_allows(&toml::Value::Table(c)))
            .unwrap_or_default();
        LintPolicy {
            by_crate,
            allowed_by_rustflags,
        }
    }

    fn level(&self, crate_dir: &str, lint: &ExtraLint) -> Option<&str> {
        let table = self.by_crate.get(crate_dir)?;
        table
            .get(lint.name)
            .or_else(|| table.get(lint.group))
            .map(String::as_str)
    }

    /// The project switched this lint off for the crate, by name or through
    /// its group.
    pub fn allows(&self, crate_dir: &str, lint: &ExtraLint) -> bool {
        self.allowed_by_rustflags.contains(lint.name)
            || self.level(crate_dir, lint) == Some("allow")
    }

    /// Lints to pass with `-W`: those that at least one crate has left
    /// undecided. A lint the project names at any level is the project's
    /// call everywhere it is named.
    pub fn lints_to_enable(&self) -> Vec<&'static ExtraLint> {
        EXTRA_LINTS
            .iter()
            .filter(|l| !self.allowed_by_rustflags.contains(l.name))
            .filter(|l| {
                self.by_crate.is_empty() || self.by_crate.keys().any(|c| self.level(c, l).is_none())
            })
            .collect()
    }

    /// Whether a diagnostic is one the project asked not to hear about.
    pub fn silences(&self, crate_dir: &str, code: &str) -> bool {
        let Some(name) = code.strip_prefix("clippy::") else {
            return false;
        };
        EXTRA_LINTS
            .iter()
            .any(|l| l.name == name && self.allows(crate_dir, l))
    }
}

/// `lint = "allow"` or `lint = { level = "allow", priority = 1 }`.
fn levels(table: &toml::Value) -> BTreeMap<String, String> {
    let Some(t) = table.as_table() else {
        return BTreeMap::new();
    };
    t.iter()
        .filter_map(|(k, v)| {
            let level = v.as_str().or_else(|| v.get("level")?.as_str())?;
            Some((k.replace('-', "_"), level.to_string()))
        })
        .collect()
}

/// Every `-A clippy::x` / `-Aclippy::x` in any `rustflags` key of a cargo
/// config, whether the value is a string or an array.
fn rustflag_allows(v: &toml::Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(table) = v.as_table() else {
        return out;
    };
    for (key, value) in table {
        if key != "rustflags" {
            out.extend(rustflag_allows(value));
            continue;
        }
        let words: Vec<&str> = match value {
            toml::Value::String(s) => s.split_whitespace().collect(),
            toml::Value::Array(a) => a.iter().filter_map(toml::Value::as_str).collect(),
            _ => Vec::new(),
        };
        let mut after_allow = false;
        for w in words {
            let lint = match w.strip_prefix("-A") {
                Some("") => {
                    after_allow = true;
                    continue;
                }
                Some(rest) => Some(rest),
                None if after_allow => Some(w),
                None => None,
            };
            after_allow = false;
            out.extend(
                lint.and_then(|l| l.strip_prefix("clippy::"))
                    .map(|l| l.replace('-', "_")),
            );
        }
    }
    out
}

/// Turn `cargo --message-format=json` output into diagnostics. Errors and
/// warnings with a primary span inside the repository are kept; the same
/// message from a second target (lib and lib-test) is kept once.
pub fn parse_messages(stdout: &str, root: &Path) -> (Vec<Diagnostic>, Option<bool>) {
    let mut out = BTreeSet::new();
    let mut build_ok = None;
    for line in stdout.lines().filter(|l| l.starts_with('{')) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match v.get("reason").and_then(|r| r.as_str()) {
            Some("build-finished") => build_ok = v.get("success").and_then(|s| s.as_bool()),
            Some("compiler-message") => {
                out.extend(v.get("message").and_then(|m| diagnostic(m, root)))
            }
            _ => {}
        }
    }
    (out.into_iter().collect(), build_ok)
}

fn diagnostic(m: &serde_json::Value, root: &Path) -> Option<Diagnostic> {
    let level = match m.get("level")?.as_str()? {
        "error" | "error: internal compiler error" => Level::Error,
        "warning" => Level::Warning,
        _ => return None,
    };
    let spans = m.get("spans")?.as_array()?;
    let primary = spans
        .iter()
        .find(|s| s.get("is_primary").and_then(|p| p.as_bool()) == Some(true))?;
    let (file, lines) = repo_span(primary, root)?;
    let children = m.get("children").and_then(|c| c.as_array());
    let related = spans
        .iter()
        .chain(
            children
                .into_iter()
                .flatten()
                .filter_map(|c| c.get("spans")?.as_array())
                .flatten(),
        )
        .filter_map(|s| repo_span(s, root))
        .filter(|s| *s != (file.clone(), lines))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Some(Diagnostic {
        file,
        lines,
        level,
        code: m
            .get("code")
            .and_then(|c| c.get("code"))
            .and_then(|c| c.as_str())
            .map(str::to_string),
        message: m.get("message")?.as_str()?.to_string(),
        related,
        on_changed_lines: false,
    })
}

/// The span's position inside the repository. Code produced by a macro from
/// another crate points into that crate, so walk out to the call site.
fn repo_span(span: &serde_json::Value, root: &Path) -> Option<(String, (u32, u32))> {
    let mut span = span;
    loop {
        let name = span.get("file_name")?.as_str()?;
        if let Some(file) = repo_relative(name, root) {
            let line = |k: &str| span.get(k)?.as_u64().and_then(|n| u32::try_from(n).ok());
            return Some((file, (line("line_start")?, line("line_end")?)));
        }
        span = span.get("expansion")?.get("span")?;
    }
}

fn repo_relative(name: &str, root: &Path) -> Option<String> {
    let p = Path::new(name);
    // `has_root` as well: a Unix-style absolute path has no drive on Windows.
    let rel = if p.is_absolute() || p.has_root() {
        p.strip_prefix(root).ok()?
    } else {
        p
    };
    let s = rel.to_string_lossy().replace('\\', "/");
    (!s.starts_with("../") && !s.is_empty()).then_some(s)
}

/// Mark each diagnostic that touches a changed line, then keep those plus
/// every error: a change can break code it did not touch. Diagnostics the
/// project's lint policy silences are dropped and counted.
pub fn filter_to_change(
    all: Vec<Diagnostic>,
    changed: &BTreeMap<String, Vec<(u32, u32)>>,
    policy: &LintPolicy,
    project: &ProjectInfo,
) -> Filtered {
    let mut f = Filtered::default();
    for mut d in all {
        let crate_dir = project.crate_for(&d.file).map_or(".", |c| c.dir.as_str());
        if d.code
            .as_deref()
            .is_some_and(|c| policy.silences(crate_dir, c))
        {
            f.silenced_by_policy += 1;
            continue;
        }
        d.on_changed_lines = changed
            .iter()
            .any(|(file, ranges)| ranges.iter().any(|&(s, e)| d.overlaps(file, s, e)));
        if d.on_changed_lines || d.level == Level::Error {
            f.kept.push(d);
        } else {
            f.warnings_outside_change += 1;
        }
    }
    f
}

#[derive(Debug, Default)]
pub struct Filtered {
    pub kept: Vec<Diagnostic>,
    pub warnings_outside_change: usize,
    pub silenced_by_policy: usize,
}

#[derive(Debug)]
pub struct ToolOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("cannot run cargo: {0}")]
    Spawn(std::io::Error),
    #[error("cargo did not finish within {} s", .0.as_secs())]
    Timeout(Duration),
}

/// Where and how cargo runs for one review.
#[derive(Debug, Clone, Copy)]
pub struct Cargo<'a> {
    pub root: &'a Path,
    /// Upper bound on one run.
    pub timeout: Duration,
    /// A target directory of the server's own, so its builds never wait on
    /// the lock of the user's `cargo build` or rust-analyzer.
    pub target_dir: Option<&'a str>,
}

/// The spawn site for cargo. No shell, no stdin, and stdout is piped so a
/// tool can never write into the MCP transport.
#[expect(
    clippy::disallowed_methods,
    reason = "the audited subprocess door for cargo tools; see clippy.toml"
)]
async fn cargo(run: Cargo<'_>, args: &[String]) -> Result<ToolOutput, ToolError> {
    let Cargo {
        root,
        timeout,
        target_dir,
    } = run;
    let mut command = tokio::process::Command::new("cargo");
    if let Some(dir) = target_dir {
        command.env("CARGO_TARGET_DIR", dir);
        // A user's `build.build-dir` sends intermediate artifacts elsewhere,
        // one directory per workspace path. Keep them with the target dir.
        command.env("CARGO_BUILD_BUILD_DIR", dir);
    }
    let child = command
        .args(args)
        .current_dir(root)
        .env("CARGO_TERM_COLOR", "never")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(ToolError::Spawn)?;
    let out = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| ToolError::Timeout(timeout))?
        .map_err(ToolError::Spawn)?;
    Ok(ToolOutput {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        success: out.status.success(),
    })
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_string()).collect()
}

/// `cargo clippy` with these extra lints, or `cargo check` when `clippy` is
/// `None`, with JSON messages. Returns the argument vector too, so the
/// report can show it.
pub async fn run_build(
    run: Cargo<'_>,
    workspace: bool,
    clippy: Option<&[&'static ExtraLint]>,
) -> Result<(Vec<String>, ToolOutput), ToolError> {
    let mut args = strings(&[if clippy.is_some() { "clippy" } else { "check" }]);
    if workspace {
        args.push("--workspace".into());
    }
    args.extend(strings(&["--all-targets", "--message-format=json"]));
    if let Some(lints) = clippy.filter(|l| !l.is_empty()) {
        args.push("--".into());
        for l in lints {
            args.extend(["-W".to_string(), format!("clippy::{}", l.name)]);
        }
    }
    let out = cargo(run, &args).await?;
    Ok((args, out))
}

pub async fn semver_checks_installed(run: Cargo<'_>) -> bool {
    let quick = Cargo {
        timeout: Duration::from_secs(20),
        ..run
    };
    cargo(quick, &strings(&["semver-checks", "--version"]))
        .await
        .is_ok_and(|o| o.success)
}

/// `cargo semver-checks` for the named library crates against a baseline
/// commit. `baseline` must be a full SHA from `Git::resolve`.
pub async fn run_semver_checks(
    run: Cargo<'_>,
    crates: &[String],
    baseline: &str,
) -> Result<ToolOutput, ToolError> {
    let mut args = strings(&["semver-checks", "--baseline-rev", baseline]);
    for c in crates {
        args.extend(["--package".to_string(), c.clone()]);
    }
    cargo(run, &args).await
}

/// One failed cargo-semver-checks lint with the items it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SemverBreak {
    pub lint: String,
    pub summary: String,
    pub items: Vec<String>,
}

/// Read the `--- failure <lint>: <summary> ---` blocks of the text report.
/// The items are the indented lines that follow `Failed in:`.
pub fn parse_semver_report(text: &str) -> Vec<SemverBreak> {
    let mut out: Vec<SemverBreak> = Vec::new();
    let mut in_items = false;
    for line in text.lines() {
        let t = line.trim();
        if let Some(head) = t
            .strip_prefix("--- failure ")
            .and_then(|r| r.strip_suffix(" ---"))
        {
            let (lint, summary) = head.split_once(": ").unwrap_or((head, ""));
            out.push(SemverBreak {
                lint: lint.to_string(),
                summary: summary.to_string(),
                items: Vec::new(),
            });
            in_items = false;
        } else if t == "Failed in:" {
            in_items = true;
        } else if t.is_empty() || !line.starts_with(' ') {
            in_items = false;
        } else if in_items && let Some(last) = out.last_mut() {
            last.items.push(t.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rust_project::CrateInfo;

    fn lint(name: &str) -> &'static ExtraLint {
        EXTRA_LINTS
            .iter()
            .find(|l| l.name == name)
            .expect("a lint from EXTRA_LINTS")
    }

    fn policy(tables: &[(&str, &str)]) -> LintPolicy {
        LintPolicy {
            by_crate: tables
                .iter()
                .map(|(dir, t)| {
                    let v: toml::Value = toml::Value::Table(t.parse().expect("valid toml"));
                    ((*dir).to_string(), levels(&v))
                })
                .collect(),
            allowed_by_rustflags: BTreeSet::new(),
        }
    }

    #[test]
    fn a_named_allow_or_a_group_allow_silences_a_lint() {
        let p = policy(&[
            (
                ".",
                "cast_sign_loss = \"allow\"\nnursery = { level = \"allow\", priority = -1 }",
            ),
            ("b", "cast-possible-wrap = \"deny\""),
        ]);
        assert!(p.allows(".", lint("cast_sign_loss")));
        assert!(p.allows(".", lint("redundant_clone")));
        assert!(!p.allows(".", lint("map_err_ignore")));
        assert!(!p.allows("b", lint("cast_sign_loss")));
        assert!(p.silences(".", "clippy::cast_sign_loss"));
        assert!(!p.silences("b", "clippy::cast_sign_loss"));
        assert!(!p.silences(".", "unused_must_use"));

        // `cast_sign_loss` stays on for crate `b`, which has not decided it.
        let on: Vec<&str> = p.lints_to_enable().iter().map(|l| l.name).collect();
        assert!(on.contains(&"cast_sign_loss"));
        assert!(on.contains(&"cast_possible_wrap"));
    }

    #[test]
    fn a_lint_every_crate_decided_is_not_passed() {
        let p = policy(&[(".", "map_err_ignore = \"allow\"\ncast_sign_loss = \"deny\"")]);
        let on: Vec<&str> = p.lints_to_enable().iter().map(|l| l.name).collect();
        assert!(!on.contains(&"map_err_ignore"));
        assert!(!on.contains(&"cast_sign_loss"));
        assert!(on.contains(&"redundant_clone"));
    }

    #[test]
    fn rustflags_allows_are_read_in_both_spellings() {
        let cfg: toml::Table = "[build]\nrustflags = [\"-A\", \"clippy::map_err_ignore\", \"-Dwarnings\"]\n[target.x]\nrustflags = \"-Aclippy::redundant-clone -C opt-level=1\"\n"
            .parse()
            .expect("valid toml");
        let got = rustflag_allows(&toml::Value::Table(cfg));
        let want: BTreeSet<String> = ["map_err_ignore", "redundant_clone"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(got, want);
    }

    const MESSAGES: &str = r#"{"reason":"compiler-artifact","package_id":"x"}
{"reason":"compiler-message","message":{"level":"warning","code":{"code":"clippy::cast_possible_truncation"},"message":"casting `usize` to `u16` may truncate","spans":[{"file_name":"src\\lib.rs","line_start":2,"line_end":2,"is_primary":true,"expansion":null}]}}
{"reason":"compiler-message","message":{"level":"warning","code":{"code":"clippy::cast_possible_truncation"},"message":"casting `usize` to `u16` may truncate","spans":[{"file_name":"src\\lib.rs","line_start":2,"line_end":2,"is_primary":true,"expansion":null}]}}
{"reason":"compiler-message","message":{"level":"warning","code":null,"message":"unused variable","spans":[{"file_name":"src/other.rs","line_start":40,"line_end":41,"is_primary":true,"expansion":null}]}}
{"reason":"compiler-message","message":{"level":"error","code":{"code":"E0308"},"message":"mismatched types","spans":[{"file_name":"/registry/m/src/lib.rs","line_start":9,"line_end":9,"is_primary":true,"expansion":{"span":{"file_name":"src/caller.rs","line_start":7,"line_end":7,"is_primary":false,"expansion":null}}}]}}
{"reason":"compiler-message","message":{"level":"warning","code":{"code":"clippy::await_holding_lock"},"message":"this `MutexGuard` is held across an await point","spans":[{"file_name":"src/cache.rs","line_start":21,"line_end":21,"is_primary":true,"expansion":null}],"children":[{"level":"note","message":"these are all the await points this lock is held through","spans":[{"file_name":"src/cache.rs","line_start":23,"line_end":23,"is_primary":true,"expansion":null}]}]}}
{"reason":"compiler-message","message":{"level":"warning","code":null,"message":"2 warnings emitted","spans":[]}}
{"reason":"build-finished","success":false}
"#;

    #[test]
    fn messages_become_repo_relative_diagnostics_once() {
        let (d, ok) = parse_messages(MESSAGES, Path::new("/repo"));
        assert_eq!(ok, Some(false));
        let got: Vec<(&str, (u32, u32), Level)> = d
            .iter()
            .map(|d| (d.file.as_str(), d.lines, d.level))
            .collect();
        assert_eq!(
            got,
            [
                ("src/cache.rs", (21, 21), Level::Warning),
                ("src/caller.rs", (7, 7), Level::Error),
                ("src/lib.rs", (2, 2), Level::Warning),
                ("src/other.rs", (40, 41), Level::Warning),
            ]
        );
    }

    #[test]
    fn only_changed_lines_errors_and_unsilenced_lints_survive() {
        let (all, _) = parse_messages(MESSAGES, Path::new("/repo"));
        let project = ProjectInfo {
            crates: vec![CrateInfo {
                dir: ".".into(),
                ..CrateInfo::default()
            }],
            ..ProjectInfo::default()
        };
        let changed = BTreeMap::from([
            // The guard on line 21 is old; the change added the await on 23.
            ("src/cache.rs".to_string(), vec![(23, 23)]),
            ("src/lib.rs".to_string(), vec![(1, 3)]),
            ("src/other.rs".to_string(), vec![(1, 39)]),
        ]);
        let f = filter_to_change(all.clone(), &changed, &LintPolicy::default(), &project);
        let kept: Vec<(&str, bool)> = f
            .kept
            .iter()
            .map(|d| (d.file.as_str(), d.on_changed_lines))
            .collect();
        // The error is outside the change and is kept; the warning at
        // other.rs:40 is outside it and is only counted.
        assert_eq!(
            kept,
            [
                ("src/cache.rs", true),
                ("src/caller.rs", false),
                ("src/lib.rs", true)
            ]
        );
        assert_eq!(f.warnings_outside_change, 1);

        let quiet = policy(&[(".", "cast_possible_truncation = \"allow\"")]);
        let f = filter_to_change(all, &changed, &quiet, &project);
        assert_eq!(f.silenced_by_policy, 1);
        assert!(f.kept.iter().all(|d| d.file != "src/lib.rs"));
    }

    #[test]
    fn semver_report_blocks_are_parsed() {
        let text = "\
     Checking demo v0.1.0 -> v0.1.0 (no change)
      Checked [   0.012s] 196 checks: 195 pass, 1 fail, 0 warn, 49 skip

--- failure function_missing: pub fn removed or renamed ---

Description:
A publicly-visible function cannot be imported by its prior path.
        ref: https://doc.rust-lang.org/cargo/reference/semver.html#item-remove

Failed in:
  function demo::parse, previously in file src/lib.rs:4
  function demo::load, previously in file src/lib.rs:9

     Summary semver requires new major version: 1 major and 0 minor checks failed
";
        let got = parse_semver_report(text);
        assert_eq!(got.len(), 1);
        let b = got.first().expect("one block");
        assert_eq!(b.lint, "function_missing");
        assert_eq!(b.summary, "pub fn removed or renamed");
        assert_eq!(b.items.len(), 2);
    }
}

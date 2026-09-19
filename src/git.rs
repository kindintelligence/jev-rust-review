//! Git access. Every invocation uses an argument vector (never a shell),
//! scope strings are validated as untrusted input, and revisions are
//! resolved to full SHAs before they are placed on any command line.

use crate::error::{Error, Result};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// The well-known SHA-1 of the empty tree.
pub const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// Staged + unstaged changes vs HEAD, plus untracked files.
    Working,
    Staged,
    /// `a..b` (or `a...b` when `merge_base`).
    Range {
        from: String,
        to: String,
        merge_base: bool,
    },
    /// The changes a single commit introduced.
    Rev(String),
    /// Uncommitted changes under a repo-relative path; whole files if none.
    Path(String),
}

/// Where the "new" side of the diff lives, for reading full file contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewSide {
    WorkingTree,
    Index,
    Commit(String),
}

#[derive(Debug, Clone)]
pub struct ResolvedScope {
    pub scope: Scope,
    /// Human-readable description with resolved SHAs.
    pub description: String,
    pub new_side: NewSide,
    /// The commit on the old side (None when it is the empty tree).
    pub old_commit: Option<String>,
    /// Arguments passed to `git diff` after the fixed option prefix.
    diff_args: Vec<String>,
    /// Restrict to this path (for Path scopes).
    pathspec: Option<String>,
    include_untracked: bool,
}

/// Reject anything that could be parsed as an option or break argv framing.
fn validate_token(s: &str, what: &str) -> Result<()> {
    if s.is_empty() {
        return Err(Error::InvalidScope(format!("{what} is empty")));
    }
    if s.starts_with('-') {
        return Err(Error::InvalidScope(format!(
            "{what} must not begin with '-' (got {s:?})"
        )));
    }
    if s.chars().any(|c| c == '\0' || c == '\n' || c == '\r') {
        return Err(Error::InvalidScope(format!(
            "{what} contains a control character"
        )));
    }
    if s.len() > 512 {
        return Err(Error::InvalidScope(format!("{what} is too long")));
    }
    Ok(())
}

/// Parse a user-supplied scope string. `repo` is used only to decide
/// whether a bare string names an existing path.
pub fn parse_scope(raw: Option<&str>, repo: &Path) -> Result<Scope> {
    let s = raw.map(str::trim).unwrap_or("");
    if s.is_empty() || s == "working" || s == "worktree" || s == "working-tree" {
        return Ok(Scope::Working);
    }
    validate_token(s, "scope")?;
    if s == "staged" || s == "cached" {
        return Ok(Scope::Staged);
    }
    if let Some(p) = s.strip_prefix("path:") {
        let p = p.trim();
        validate_token(p, "path")?;
        return Ok(Scope::Path(safe_rel_path(p)?));
    }
    if let Some(r) = s.strip_prefix("rev:") {
        let r = r.trim();
        return parse_rev_or_range(r);
    }
    if s.contains("..") {
        return parse_rev_or_range(s);
    }
    if repo.join(s).exists() {
        return Ok(Scope::Path(safe_rel_path(s)?));
    }
    parse_rev_or_range(s)
}

fn parse_rev_or_range(s: &str) -> Result<Scope> {
    validate_token(s, "revision")?;
    if let Some((a, b)) = s.split_once("...") {
        let (a, b) = (non_empty_or_head(a), non_empty_or_head(b));
        validate_token(&a, "range start")?;
        validate_token(&b, "range end")?;
        return Ok(Scope::Range {
            from: a,
            to: b,
            merge_base: true,
        });
    }
    if let Some((a, b)) = s.split_once("..") {
        let (a, b) = (non_empty_or_head(a), non_empty_or_head(b));
        validate_token(&a, "range start")?;
        validate_token(&b, "range end")?;
        return Ok(Scope::Range {
            from: a,
            to: b,
            merge_base: false,
        });
    }
    Ok(Scope::Rev(s.to_string()))
}

fn non_empty_or_head(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        "HEAD".into()
    } else {
        s.into()
    }
}

/// Normalise a repo-relative path and refuse anything escaping the repo.
pub fn safe_rel_path(p: &str) -> Result<String> {
    let path = Path::new(p);
    if path.is_absolute() {
        return Err(Error::InvalidScope(format!(
            "path must be relative to the repository: {p:?}"
        )));
    }
    let mut parts: Vec<String> = Vec::new();
    for c in path.components() {
        match c {
            Component::Normal(x) => parts.push(x.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return Err(Error::InvalidScope(format!(
                        "path escapes the repository: {p:?}"
                    )));
                }
            }
            _ => {
                return Err(Error::InvalidScope(format!("unsupported path: {p:?}")));
            }
        }
    }
    let joined = parts.join("/");
    if joined.starts_with('-') {
        return Err(Error::InvalidScope(format!(
            "path must not begin with '-': {p:?}"
        )));
    }
    Ok(if joined.is_empty() {
        ".".into()
    } else {
        joined
    })
}

pub struct Git {
    root: PathBuf,
}

/// The only process spawn site in the crate: `git` with an argument vector,
/// never a shell. Callers pass validated arguments only.
#[expect(
    clippy::disallowed_methods,
    reason = "the single audited subprocess door; see clippy.toml"
)]
fn git_command() -> Command {
    let mut c = Command::new("git");
    c.env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_EXTERNAL_DIFF");
    c
}

impl Git {
    /// Open the repository containing `path`.
    pub fn open(path: &Path) -> Result<Git> {
        let out = git_command()
            .arg("-C")
            .arg(path)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .map_err(|e| Error::NotARepo(format!("cannot run git: {e}")))?;
        if !out.status.success() {
            return Err(Error::NotARepo(format!(
                "{}: {}",
                path.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let top = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(Git {
            root: PathBuf::from(top),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn run(&self, args: &[&str]) -> Result<Vec<u8>> {
        let out = git_command()
            .arg("-C")
            .arg(&self.root)
            // Neutralise user config that would change diff output.
            .args([
                "-c",
                "core.quotePath=false",
                "-c",
                "diff.noprefix=false",
                "-c",
                "diff.mnemonicPrefix=false",
                "-c",
                "color.ui=false",
            ])
            .args(args)
            .output()
            .map_err(|e| Error::Git(format!("cannot run git: {e}")))?;
        if !out.status.success() {
            return Err(Error::Git(format!(
                "git {}: {}",
                args.first().unwrap_or(&""),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(out.stdout)
    }

    fn run_str(&self, args: &[&str]) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.run(args)?).into_owned())
    }

    /// Resolve a revision to a full commit SHA. The input was validated to
    /// not begin with '-', and `--end-of-options` makes that belt-and-braces.
    pub fn resolve_commit(&self, rev: &str) -> Result<String> {
        validate_token(rev, "revision")?;
        let spec = format!("{rev}^{{commit}}");
        let out = self
            .run_str(&[
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &spec,
            ])
            .map_err(|_| Error::InvalidScope(format!("unknown revision: {rev:?}")))?;
        let sha = out.trim().to_string();
        if sha.len() < 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::InvalidScope(format!("unknown revision: {rev:?}")));
        }
        Ok(sha)
    }

    fn has_head(&self) -> bool {
        self.resolve_commit("HEAD").is_ok()
    }

    pub fn resolve(&self, scope: Scope) -> Result<ResolvedScope> {
        let base_or_empty = || -> String {
            if self.has_head() {
                "HEAD".to_string()
            } else {
                EMPTY_TREE.to_string()
            }
        };
        Ok(match &scope {
            Scope::Working => {
                let base = base_or_empty();
                let base_sha = if base == "HEAD" {
                    self.resolve_commit("HEAD")?
                } else {
                    base
                };
                ResolvedScope {
                    description: format!("working tree vs {}", short(&base_sha)),
                    new_side: NewSide::WorkingTree,
                    old_commit: non_empty_tree(&base_sha),
                    diff_args: vec![base_sha],
                    pathspec: None,
                    include_untracked: true,
                    scope,
                }
            }
            Scope::Staged => {
                let mut args = vec!["--cached".to_string()];
                if !self.has_head() {
                    args.push(EMPTY_TREE.into());
                }
                ResolvedScope {
                    description: "staged changes (index vs HEAD)".into(),
                    new_side: NewSide::Index,
                    old_commit: self.resolve_commit("HEAD").ok(),
                    diff_args: args,
                    pathspec: None,
                    include_untracked: false,
                    scope,
                }
            }
            Scope::Range {
                from,
                to,
                merge_base,
            } => {
                let to_sha = self.resolve_commit(to)?;
                let from_sha = self.resolve_commit(from)?;
                let base = if *merge_base {
                    self.run_str(&["merge-base", "--end-of-options", &from_sha, &to_sha])?
                        .trim()
                        .to_string()
                } else {
                    from_sha.clone()
                };
                ResolvedScope {
                    description: format!(
                        "{}{}{}",
                        short(&from_sha),
                        if *merge_base { "..." } else { ".." },
                        short(&to_sha)
                    ),
                    new_side: NewSide::Commit(to_sha.clone()),
                    old_commit: Some(base.clone()),
                    diff_args: vec![base, to_sha],
                    pathspec: None,
                    include_untracked: false,
                    scope,
                }
            }
            Scope::Rev(r) => {
                let sha = self.resolve_commit(r)?;
                let parent = self
                    .resolve_commit(&format!("{sha}^"))
                    .unwrap_or_else(|_| EMPTY_TREE.to_string());
                ResolvedScope {
                    description: format!("commit {}", short(&sha)),
                    new_side: NewSide::Commit(sha.clone()),
                    old_commit: non_empty_tree(&parent),
                    diff_args: vec![parent, sha],
                    pathspec: None,
                    include_untracked: false,
                    scope,
                }
            }
            Scope::Path(p) => {
                let base = base_or_empty();
                let base_sha = if base == "HEAD" {
                    self.resolve_commit("HEAD")?
                } else {
                    base
                };
                ResolvedScope {
                    description: format!("uncommitted changes under {p}"),
                    new_side: NewSide::WorkingTree,
                    old_commit: non_empty_tree(&base_sha),
                    diff_args: vec![base_sha],
                    pathspec: Some(p.clone()),
                    include_untracked: true,
                    scope,
                }
            }
        })
    }

    /// Unified diff text for a resolved scope.
    pub fn diff(&self, rs: &ResolvedScope) -> Result<String> {
        let mut args: Vec<&str> = vec![
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--find-renames",
            "--unified=3",
            "--src-prefix=a/",
            "--dst-prefix=b/",
        ];
        args.extend(rs.diff_args.iter().map(String::as_str));
        args.push("--");
        if let Some(p) = &rs.pathspec {
            args.push(p);
        }
        self.run_str(&args)
    }

    /// Untracked, non-ignored files (only for working-tree scopes).
    pub fn untracked(&self, rs: &ResolvedScope) -> Result<Vec<String>> {
        if !rs.include_untracked {
            return Ok(Vec::new());
        }
        let mut args = vec!["ls-files", "--others", "--exclude-standard", "-z", "--"];
        if let Some(p) = &rs.pathspec {
            args.push(p);
        }
        let out = self.run(&args)?;
        Ok(out
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect())
    }

    /// Tracked files under a path (for whole-file review of a Path scope).
    pub fn tracked_under(&self, path: &str) -> Result<Vec<String>> {
        let out = self.run(&["ls-files", "-z", "--", path])?;
        Ok(out
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect())
    }

    /// Read a file's content on the new side of the scope.
    pub fn read_new(&self, side: &NewSide, rel: &str) -> Result<Option<String>> {
        let rel = safe_rel_path(rel)?;
        match side {
            NewSide::WorkingTree => {
                let p = self.root.join(&rel);
                // Refuse symlinks that point outside the repository.
                match std::fs::canonicalize(&p) {
                    Ok(canon) => {
                        let root = std::fs::canonicalize(&self.root)?;
                        if !canon.starts_with(&root) {
                            return Err(Error::InvalidInput(format!(
                                "{rel} resolves outside the repository"
                            )));
                        }
                    }
                    Err(_) => return Ok(None),
                }
                match std::fs::read(&p) {
                    Ok(b) => Ok(Some(String::from_utf8_lossy(&b).into_owned())),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(e) => Err(e.into()),
                }
            }
            NewSide::Index => self.show(&format!(":{rel}")),
            NewSide::Commit(sha) => self.show(&format!("{sha}:{rel}")),
        }
    }

    /// Read a file's content on the old side of the scope.
    pub fn read_old(&self, rs: &ResolvedScope, rel: &str) -> Result<Option<String>> {
        let rel = safe_rel_path(rel)?;
        match &rs.old_commit {
            Some(sha) => self.show(&format!("{sha}:{rel}")),
            None => Ok(None),
        }
    }

    fn show(&self, object: &str) -> Result<Option<String>> {
        match self.run(&["cat-file", "blob", "--end-of-options", object]) {
            Ok(b) => Ok(Some(String::from_utf8_lossy(&b).into_owned())),
            Err(_) => Ok(None),
        }
    }
}

fn non_empty_tree(sha: &str) -> Option<String> {
    (sha != EMPTY_TREE).then(|| sha.to_string())
}

fn short(sha: &str) -> String {
    sha.chars().take(12).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Scope> {
        parse_scope(Some(s), Path::new("/nonexistent-repo-root"))
    }

    #[test]
    fn keywords() {
        assert_eq!(parse_scope(None, Path::new(".")).unwrap(), Scope::Working);
        assert_eq!(p("  ").unwrap(), Scope::Working);
        assert_eq!(p("staged").unwrap(), Scope::Staged);
    }

    #[test]
    fn ranges() {
        assert_eq!(
            p("main...HEAD").unwrap(),
            Scope::Range {
                from: "main".into(),
                to: "HEAD".into(),
                merge_base: true
            }
        );
        assert_eq!(
            p("a..").unwrap(),
            Scope::Range {
                from: "a".into(),
                to: "HEAD".into(),
                merge_base: false
            }
        );
        assert_eq!(p("rev:abc123").unwrap(), Scope::Rev("abc123".into()));
        assert_eq!(p("v1.0").unwrap(), Scope::Rev("v1.0".into()));
    }

    #[test]
    fn option_injection_rejected() {
        for bad in [
            "--output=/tmp/x",
            "-p",
            "rev:--all",
            "main..--output=x",
            "--upload-pack=evil..HEAD",
            "path:-rf",
            "a\nb",
            "a\0b",
        ] {
            assert!(p(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn path_escape_rejected() {
        assert!(safe_rel_path("../etc/passwd").is_err());
        assert!(safe_rel_path("src/../../x").is_err());
        assert!(safe_rel_path("/etc/passwd").is_err());
        assert_eq!(safe_rel_path("./src/../src/lib.rs").unwrap(), "src/lib.rs");
        assert!(p("path:../x").is_err());
    }
}

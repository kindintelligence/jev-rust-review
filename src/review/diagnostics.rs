//! Tool diagnostics: run Clippy and cargo-semver-checks, keep what touches
//! the change, and remember it so triage and verification never repeat a
//! defect a tool already reported.

use super::evaluate::collect_files;
use crate::cargo_tools::{
    self, Cargo, Diagnostic, Filtered, LintPolicy, SemverBreak, ToolError, ToolOutput,
};
use crate::config::Config;
use crate::diff::{FileDiff, LineKind};
use crate::error::{Error, Result};
use crate::git::{self, Git, NewSide};
use crate::questions;
use crate::rust_project::{self, ProjectInfo, Role};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::Instant;

#[derive(Debug, Serialize)]
pub struct SemverOutput {
    /// `breaking`, `compatible`, `absent`, `no_baseline` or `failed`.
    pub status: &'static str,
    pub crates: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<SemverBreak>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticsOutput {
    /// `ok`, `disabled`, `skipped`, `failed` or `timeout`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub repo: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Whether the build succeeded. `false` means there are errors below.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_ok: Option<bool>,
    /// Off-by-default lints this run switched on.
    pub extra_lints: Vec<String>,
    /// Errors anywhere, and warnings on changed lines. Facts, not flags.
    pub diagnostics: Vec<Diagnostic>,
    pub warnings_outside_change: usize,
    pub silenced_by_project_policy: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semver: Option<SemverOutput>,
    /// One-line suggestions for tools this server does not run, such as Miri.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub advice: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    pub wall_ms: u128,
}

/// What the tools reported for one repository and scope.
#[derive(Debug, Default)]
pub struct ToolReport {
    pub diagnostics: Vec<Diagnostic>,
    /// Crate dirs cargo-semver-checks gave a verdict on.
    pub semver_checked_dirs: BTreeSet<String>,
    pub semver_breaks: Vec<SemverBreak>,
}

impl ToolReport {
    /// A tool diagnostic on these lines whose lint is one of `lints`.
    pub fn covering(
        &self,
        file: &str,
        ranges: &[(u32, u32)],
        lints: &BTreeSet<&str>,
    ) -> Option<&Diagnostic> {
        self.diagnostics.iter().find(|d| {
            d.code.as_deref().is_some_and(|c| lints.contains(c))
                && ranges.iter().any(|&(s, e)| d.overlaps(file, s, e))
        })
    }

    pub fn semver_checked(&self, project: &ProjectInfo, file: &str) -> bool {
        project
            .crate_for(file)
            .is_some_and(|c| self.semver_checked_dirs.contains(&c.dir))
    }
}

type Ledger = Mutex<HashMap<(String, String), Arc<ToolReport>>>;

/// The server is one long-lived process per session, so the last report for
/// each (repository, scope) lives here. Triage and verification read it; a
/// review that never ran the tools finds nothing and deduplicates nothing.
static LEDGER: LazyLock<Ledger> = LazyLock::new(Ledger::default);

fn record(repo: &str, scope: &str, report: ToolReport) {
    LEDGER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert((repo.to_string(), scope.to_string()), Arc::new(report));
}

pub fn tool_report(repo: &str, scope: &str) -> Option<Arc<ToolReport>> {
    LEDGER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&(repo.to_string(), scope.to_string()))
        .cloned()
}

/// Everything read from git and the manifests before cargo runs.
struct Plan {
    root: PathBuf,
    scope: String,
    project: ProjectInfo,
    policy: LintPolicy,
    changed: BTreeMap<String, Vec<(u32, u32)>>,
    /// Library crates whose `pub` surface the change touches: (name, dir).
    pub_changed: Vec<(String, String)>,
    unsafe_changed: bool,
    baseline: Option<String>,
    notes: Vec<String>,
}

fn changed_ranges(f: &FileDiff) -> Vec<(u32, u32)> {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for n in f.hunks.iter().flat_map(|h| h.added_lines()) {
        match ranges.last_mut() {
            Some(last) if last.1 + 1 >= n => last.1 = last.1.max(n),
            _ => ranges.push((n, n)),
        }
    }
    ranges
}

/// The added and removed lines with their diff markers, which is the form
/// the question gates are written for.
fn changed_text(f: &FileDiff) -> String {
    let mut s = String::new();
    for l in f.hunks.iter().flat_map(|h| h.lines.iter()) {
        let marker = match l.kind {
            LineKind::Added => '+',
            LineKind::Removed => '-',
            LineKind::Context => continue,
        };
        s.push(marker);
        s.push_str(&l.text);
        s.push('\n');
    }
    s
}

fn plan(repo: &Path, scope: Option<&str>) -> Result<Plan> {
    let git = Git::open(repo)?;
    let root = git.root().to_path_buf();
    let rs = git.resolve(git::parse_scope(scope, &root)?)?;
    let project = ProjectInfo::load(&root);
    let (files, _) = collect_files(&git, &rs)?;
    let api_gate = questions::spec("api.breaking_change");
    let mut plan = Plan {
        policy: LintPolicy::load(&root, &project),
        scope: rs.description.clone(),
        changed: BTreeMap::new(),
        pub_changed: Vec::new(),
        unsafe_changed: false,
        baseline: rs.old_commit.clone(),
        notes: Vec::new(),
        project,
        root,
    };
    for f in files.iter().filter(|f| f.path().ends_with(".rs")) {
        let path = f.path();
        plan.changed.insert(path.to_string(), changed_ranges(f));
        let text = changed_text(f);
        plan.unsafe_changed |= text.lines().any(|l| l.contains("unsafe"));
        let Some(krate) = plan.project.crate_for(path) else {
            continue;
        };
        let library = rust_project::role_for(path, Some(krate)) == Role::Library;
        let entry = (krate.name.clone(), krate.dir.clone());
        if library
            && api_gate.is_some_and(|q| q.gate_open(&text))
            && !plan.pub_changed.contains(&entry)
        {
            plan.pub_changed.push(entry);
        }
    }
    if let NewSide::Commit(sha) = &rs.new_side
        && git.resolve_commit("HEAD").ok().as_ref() != Some(sha)
    {
        plan.notes.push(format!(
            "cargo compiles the working tree, which is not at {sha}; line numbers may not match the scope"
        ));
    }
    Ok(plan)
}

impl Plan {
    fn cargo<'a>(&'a self, cfg: &'a Config) -> Cargo<'a> {
        Cargo {
            root: &self.root,
            timeout: cfg.cargo_timeout,
            target_dir: cfg.cargo_target_dir.as_deref(),
        }
    }
}

fn tail(s: &str, lines: usize) -> String {
    let all: Vec<&str> = s.lines().collect();
    all.get(all.len().saturating_sub(lines)..)
        .unwrap_or_default()
        .join("\n")
}

async fn semver(cfg: &Config, plan: &Plan) -> Option<SemverOutput> {
    if plan.pub_changed.is_empty() || !cfg.run_semver_checks {
        return None;
    }
    let crates: Vec<String> = plan.pub_changed.iter().map(|(n, _)| n.clone()).collect();
    let mut out = SemverOutput {
        status: "absent",
        crates: crates.clone(),
        baseline: plan.baseline.clone(),
        breaks: Vec::new(),
        note: None,
    };
    let Some(baseline) = &plan.baseline else {
        out.status = "no_baseline";
        out.note = Some("the scope has no earlier commit to compare against".into());
        return Some(out);
    };
    if !cargo_tools::semver_checks_installed(plan.cargo(cfg)).await {
        out.note = Some(
            "cargo-semver-checks is not installed; the public API change was not checked by a tool"
                .into(),
        );
        return Some(out);
    }
    match cargo_tools::run_semver_checks(plan.cargo(cfg), &crates, baseline).await {
        Ok(o) => {
            let text = format!("{}\n{}", o.stdout, o.stderr);
            out.breaks = cargo_tools::parse_semver_report(&text);
            out.status = match (o.success, out.breaks.is_empty()) {
                (true, _) => "compatible",
                (false, false) => "breaking",
                (false, true) => {
                    out.note = Some(tail(&o.stderr, 6));
                    "failed"
                }
            };
        }
        Err(e) => {
            out.status = "failed";
            out.note = Some(e.to_string());
        }
    }
    Some(out)
}

/// Clippy first; `cargo check` when Clippy is not installed. A run that
/// never reaches `build-finished` did not compile anything.
async fn build(
    cfg: &Config,
    plan: &Plan,
) -> std::result::Result<(String, Vec<String>, ToolOutput), ToolError> {
    let lints = plan.policy.lints_to_enable();
    let names: Vec<String> = lints
        .iter()
        .map(|l| format!("clippy::{}", l.name))
        .collect();
    let ws = plan.project.is_workspace;
    let (args, out) = cargo_tools::run_build(plan.cargo(cfg), ws, Some(&lints)).await?;
    if out.stdout.contains("\"build-finished\"") {
        return Ok((format!("cargo {}", args.join(" ")), names, out));
    }
    let (args, out) = cargo_tools::run_build(plan.cargo(cfg), ws, None).await?;
    Ok((format!("cargo {}", args.join(" ")), Vec::new(), out))
}

pub async fn diagnostics(
    cfg: &Config,
    repo: &Path,
    scope: Option<String>,
) -> Result<DiagnosticsOutput> {
    let started = Instant::now();
    let plan = {
        let repo = repo.to_path_buf();
        tokio::task::spawn_blocking(move || plan(&repo, scope.as_deref()))
            .await
            .map_err(|e| Error::InvalidInput(format!("preparation task failed: {e}")))??
    };
    let mut out = DiagnosticsOutput {
        status: "ok",
        reason: None,
        repo: plan.root.display().to_string(),
        scope: plan.scope.clone(),
        command: None,
        build_ok: None,
        extra_lints: Vec::new(),
        diagnostics: Vec::new(),
        warnings_outside_change: 0,
        silenced_by_project_policy: 0,
        semver: None,
        advice: Vec::new(),
        notes: plan.notes.clone(),
        wall_ms: 0,
    };
    let stop = |mut out: DiagnosticsOutput, status, reason: String| {
        out.status = status;
        out.reason = Some(reason);
        out.wall_ms = started.elapsed().as_millis();
        Ok(out)
    };
    if !cfg.run_cargo {
        return stop(out, "disabled", "JEV_RUST_REVIEW_CARGO is off".into());
    }
    if plan.project.crates.is_empty() {
        return stop(
            out,
            "skipped",
            "no Cargo.toml at the repository root".into(),
        );
    }
    if plan.unsafe_changed {
        out.advice.push(
            "`unsafe` code changed. Miri finds undefined behaviour that no lint can: `cargo +nightly miri test`. It was not run."
                .into(),
        );
    }
    let (command, lints, built) = match build(cfg, &plan).await {
        Ok(b) => b,
        Err(e @ ToolError::Timeout(_)) => return stop(out, "timeout", e.to_string()),
        Err(e) => return stop(out, "failed", e.to_string()),
    };
    out.command = Some(command);
    out.extra_lints = lints;
    let (all, build_ok) = cargo_tools::parse_messages(&built.stdout, &plan.root);
    if build_ok.is_none() {
        return stop(out, "failed", tail(&built.stderr, 6));
    }
    if out.extra_lints.is_empty() {
        out.notes
            .push("Clippy is not installed; this is `cargo check` output only".into());
    }
    out.build_ok = build_ok;
    let Filtered {
        kept,
        warnings_outside_change,
        silenced_by_policy,
    } = cargo_tools::filter_to_change(all, &plan.changed, &plan.policy, &plan.project);
    out.warnings_outside_change = warnings_outside_change;
    out.silenced_by_project_policy = silenced_by_policy;
    out.diagnostics = kept;
    // Errors first, then by position.
    out.diagnostics
        .sort_by(|a, b| (a.level, &a.file, a.lines).cmp(&(b.level, &b.file, b.lines)));

    out.semver = semver(cfg, &plan).await;
    let mut report = ToolReport {
        diagnostics: out.diagnostics.clone(),
        ..ToolReport::default()
    };
    if let Some(s) = &out.semver
        && matches!(s.status, "breaking" | "compatible")
    {
        report.semver_checked_dirs = plan.pub_changed.iter().map(|(_, d)| d.clone()).collect();
        report.semver_breaks = s.breaks.clone();
    }
    record(&out.repo, &out.scope, report);
    out.wall_ms = started.elapsed().as_millis();
    Ok(out)
}

//! Three-mode end-to-end eval. Opt-in, never in CI: it builds the fixture
//! crates (network), and modes C and J run the real product headless through
//! `claude -p --plugin-dir`, which spends Claude and Jev tokens.
//!
//! - `T` tools only: `cargo_diagnostics` in process. No model.
//! - `C` Claude only: the plugin with no TypeSafe key (`jev_unavailable`).
//! - `J` the full pipeline with Jev.
//!
//! A bug counts towards usefulness only if mode T misses it. Grading is code:
//! a run finds the bug when a reported finding overlaps the fixture's claim
//! range in an expected dimension; every finding on a clean fixture is a
//! false positive; everything else is listed for a person to judge.
//!
//! ```text
//! cargo build --release
//! E2E_MODES=T,C,J E2E_RUNS=1 cargo test --test e2e -- --ignored --nocapture
//! ```
//!
//! Cells are written to `eval-results/e2e/cells/` as they finish and are
//! never re-run, so a crash or a raised `E2E_RUNS` only pays for what is
//! missing. `E2E_FIXTURES=a,b` narrows the corpus, `E2E_JOBS` sets how many
//! headless sessions run at once, and `E2E_MODEL` picks the Claude model.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::field_reassign_with_default,
    reason = "test code: failures should panic loudly, and tests tweak one config field at a time"
)]

mod common;

use common::fixtures::{Fixture, load_fixtures};
use jev_rust_review::cargo_tools::{Diagnostic, Level};
use jev_rust_review::config::{Config, USD_PER_INPUT_TOKEN};
use jev_rust_review::questions::{self, Dimension};
use jev_rust_review::review;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const PLUGIN_TOOLS: &str = "mcp__plugin_jev-rust-review_jev__cargo_diagnostics,mcp__plugin_jev-rust-review_jev__evaluate_rust_changes,mcp__plugin_jev-rust-review_jev__verify_rust_findings";
const PROMPT: &str = "/jev-rust-review:rust-review --json";
const DEFAULT_MODEL: &str = "claude-sonnet-5";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Reported {
    #[serde(default)]
    severity: String,
    dimension: String,
    file: String,
    start_line: u32,
    end_line: u32,
    #[serde(default)]
    title: String,
    /// `tool` for a compiler or lint diagnostic, `review` for a finding.
    #[serde(default)]
    source: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cell {
    mode: String,
    fixture: String,
    kind: String,
    run: usize,
    /// `ok`, or why the cell cannot be graded.
    status: String,
    findings: Vec<Reported>,
    found: bool,
    /// Findings that are neither the seeded bug nor, on a clean fixture,
    /// anything at all. On a clean fixture every finding lands here.
    unexpected: Vec<Reported>,
    claude_input_tokens: u64,
    claude_output_tokens: u64,
    claude_usd: f64,
    claude_models: Vec<String>,
    jev_input_tokens: u64,
    jev_requests: u64,
    evaluate_status: Option<String>,
    /// Units triage saw, and how many of them carried at least one flag.
    #[serde(default)]
    units: usize,
    #[serde(default)]
    flagged_units: usize,
    /// Whether a triage flag covered the seeded bug's lines (mode J).
    #[serde(default)]
    triage_flagged_seed: bool,
    /// Every candidate Claude sent to verification, with Jev's verdict.
    #[serde(default)]
    verified: Vec<Verified>,
    wall_ms: u128,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Verified {
    verdict: String,
    /// The candidate overlaps the seeded bug's lines.
    on_seed: bool,
    dimension: String,
    claim: String,
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn out_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("eval-results/e2e")
}

fn norm_dimension(d: &str) -> String {
    Dimension::parse(d).map_or_else(|| d.to_ascii_lowercase(), |d| d.name().to_string())
}

fn overlaps(f: &Reported, file: &str, (s, e): (u32, u32)) -> bool {
    f.file.replace('\\', "/").trim_start_matches("./") == file
        && f.start_line <= e
        && s <= f.end_line
}

/// Grade one run in code. `errors_count` lets a compiler error on the claim
/// lines count for mode T whatever dimension it maps to.
fn grade(fx: &Fixture, findings: &[Reported]) -> (bool, Vec<Reported>) {
    if !fx.buggy() {
        return (false, findings.to_vec());
    }
    let mut wanted: BTreeSet<String> = fx
        .spec
        .expected_dimensions
        .iter()
        .map(|d| norm_dimension(d))
        .collect();
    wanted.insert(norm_dimension(&fx.spec.claim.dimension));
    let claim = fx.claim_lines();
    let hit = |f: &Reported| {
        overlaps(f, &fx.spec.file, claim)
            && (wanted.contains(&norm_dimension(&f.dimension)) || f.dimension == "compiler_error")
    };
    let found = findings.iter().any(hit);
    let unexpected = findings.iter().filter(|f| !hit(f)).cloned().collect();
    (found, unexpected)
}

// ------------------------------------------------------------------ mode T

/// The dimension a lint speaks for. A lint can speak for several, so prefer
/// one the fixture expects and fall back to the first.
fn lint_dimension(code: &str, fx: &Fixture) -> Option<&'static str> {
    let dims = questions::lint_dimensions(code);
    dims.iter()
        .find(|d| fx.spec.expected_dimensions.iter().any(|e| e == d.name()))
        .or(dims.first())
        .map(|d| d.name())
}

/// The related spans that lie in the diagnostic's own file.
fn same_file(d: &Diagnostic) -> impl Iterator<Item = (u32, u32)> + '_ {
    d.related
        .iter()
        .filter(|(file, _)| *file == d.file)
        .map(|(_, lines)| *lines)
}

async fn run_tools(fx: &Fixture, target_dir: &Path) -> Cell {
    let repo = fx.repo();
    let mut cfg = Config::default();
    cfg.cargo_target_dir = Some(target_dir.display().to_string());
    let started = Instant::now();
    let out = review::diagnostics(&cfg, &repo.path(), None).await.unwrap();
    let mut findings: Vec<Reported> = out
        .diagnostics
        .iter()
        .map(|d| Reported {
            severity: String::new(),
            dimension: match (
                d.level,
                d.code.as_deref().and_then(|c| lint_dimension(c, fx)),
            ) {
                (Level::Error, _) => "compiler_error".into(),
                (_, Some(dim)) => dim.into(),
                (_, None) => "tool".into(),
            },
            file: d.file.clone(),
            // Every line the diagnostic points at, notes included.
            start_line: same_file(d).map(|l| l.0).fold(d.lines.0, u32::min),
            end_line: same_file(d).map(|l| l.1).fold(d.lines.1, u32::max),
            title: format!("{}: {}", d.code.as_deref().unwrap_or("rustc"), d.message),
            source: "tool".into(),
        })
        .collect();
    // A semver break has no line in the new file; it speaks for the crate.
    for b in out.semver.iter().flat_map(|s| s.breaks.iter()) {
        findings.push(Reported {
            severity: String::new(),
            dimension: "api".into(),
            file: fx.spec.file.clone(),
            start_line: 1,
            end_line: u32::MAX,
            title: format!("cargo-semver-checks {}: {}", b.lint, b.summary),
            source: "tool".into(),
        });
    }
    let (found, unexpected) = grade(fx, &findings);
    Cell {
        mode: "T".into(),
        fixture: fx.name.clone(),
        kind: fx.kind.into(),
        run: 1,
        status: if out.status == "ok" {
            "ok".into()
        } else {
            format!("{}: {}", out.status, out.reason.unwrap_or_default())
        },
        findings,
        found,
        unexpected,
        wall_ms: started.elapsed().as_millis(),
        ..Cell::default()
    }
}

// ------------------------------------------------------------- modes C and J

/// The last fenced `json` block of the report.
fn findings_block(report: &str) -> Option<Vec<Reported>> {
    let start = report.rfind("```json")? + "```json".len();
    let rest = report.get(start..)?;
    let body = rest.get(..rest.find("```")?)?;
    #[derive(Deserialize)]
    struct Block {
        findings: Vec<Reported>,
    }
    serde_json::from_str::<Block>(body).ok().map(|b| b.findings)
}

/// One call to a plugin tool: its name, its input, and its parsed result.
type ToolCall = (String, serde_json::Value, serde_json::Value);

/// The plugin tool calls in a `stream-json` transcript, the cost of the
/// session, and the final report.
fn read_transcript(stdout: &str, cell: &mut Cell) -> (Vec<ToolCall>, Option<String>) {
    let mut uses: BTreeMap<String, (String, serde_json::Value)> = BTreeMap::new();
    let mut calls = Vec::new();
    let mut report = None;
    for v in stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
    {
        let blocks = v["message"]["content"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for b in &blocks {
            if b["type"] == "tool_use" {
                let name = b["name"].as_str().unwrap_or("").to_string();
                uses.insert(
                    b["id"].as_str().unwrap_or("").into(),
                    (name, b["input"].clone()),
                );
            }
            let used = uses.get(b["tool_use_id"].as_str().unwrap_or(""));
            let Some((name, input)) = used.filter(|(n, _)| n.contains("jev-rust-review")) else {
                continue;
            };
            let text = match &b["content"] {
                serde_json::Value::String(s) => s.clone(),
                other => other[0]["text"].as_str().unwrap_or("").to_string(),
            };
            if let Ok(result) = serde_json::from_str::<serde_json::Value>(&text) {
                calls.push((name.clone(), input.clone(), result));
            }
        }
        if v["type"] == "result" {
            read_cost(&v, cell);
            report = v["result"].as_str().map(str::to_string);
        }
    }
    (calls, report)
}

fn read_cost(result: &serde_json::Value, cell: &mut Cell) {
    cell.claude_usd = result["total_cost_usd"].as_f64().unwrap_or(0.0);
    for (model, u) in result["modelUsage"].as_object().into_iter().flatten() {
        cell.claude_models.push(model.clone());
        cell.claude_input_tokens += [
            "inputTokens",
            "cacheReadInputTokens",
            "cacheCreationInputTokens",
        ]
        .iter()
        .map(|k| u[*k].as_u64().unwrap_or(0))
        .sum::<u64>();
        cell.claude_output_tokens += u["outputTokens"].as_u64().unwrap_or(0);
    }
}

/// What the tool calls say about Jev: tokens, whether triage flagged the
/// seeded bug's lines, and what verification did to each candidate. This is
/// how triage and verification are judged apart from each other.
fn jev_facts(fx: &Fixture, calls: &[ToolCall], cell: &mut Cell) {
    let claim = fx.claim_lines();
    let on_seed = |file: &serde_json::Value, s: &serde_json::Value, e: &serde_json::Value| {
        fx.buggy()
            && file.as_str() == Some(fx.spec.file.as_str())
            && s.as_u64().unwrap_or(0) <= u64::from(claim.1)
            && u64::from(claim.0) <= e.as_u64().unwrap_or(0)
    };
    for (name, input, result) in calls {
        cell.jev_input_tokens += result["usage"]["input_tokens"].as_u64().unwrap_or(0);
        cell.jev_requests += result["usage"]["requests"].as_u64().unwrap_or(0);
        let flags = result["flagged"].as_array().cloned().unwrap_or_default();
        if name.ends_with("evaluate_rust_changes") {
            cell.evaluate_status = result["status"].as_str().map(str::to_string);
            cell.units = result["units"].as_array().map_or(0, Vec::len);
            cell.flagged_units = flags
                .iter()
                .filter_map(|f| f["unit"].as_str())
                .collect::<BTreeSet<_>>()
                .len();
            cell.triage_flagged_seed = flags
                .iter()
                .any(|f| on_seed(&f["file"], &f["lines"][0], &f["lines"][1]));
        }
        if !name.ends_with("verify_rust_findings") {
            continue;
        }
        let sent = input["findings"].as_array().cloned().unwrap_or_default();
        let results = result["results"].as_array().cloned().unwrap_or_default();
        for (f, r) in sent.iter().zip(&results) {
            cell.verified.push(Verified {
                verdict: r["verdict"].as_str().unwrap_or("").to_string(),
                on_seed: on_seed(&f["file"], &f["start_line"], &f["end_line"]),
                dimension: f["dimension"].as_str().unwrap_or("").to_string(),
                claim: f["claim"].as_str().unwrap_or("").to_string(),
            });
        }
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "the eval drives the real product through the claude CLI"
)]
async fn claude(
    repo: &Path,
    mode: &str,
    target_dir: &Path,
) -> std::io::Result<std::process::Output> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let exe = if cfg!(windows) {
        "jev-rust-review.exe"
    } else {
        "jev-rust-review"
    };
    let mut cmd = tokio::process::Command::new("claude");
    cmd.current_dir(repo)
        .args(["-p", PROMPT, "--plugin-dir"])
        .arg(root)
        .args(["--model", &env("E2E_MODEL").unwrap_or_else(|| DEFAULT_MODEL.into())])
        .args(["--output-format", "stream-json", "--verbose"])
        // No user or project settings: the run sees this plugin and nothing
        // else from the machine it happens to run on.
        .args(["--setting-sources", "", "--no-session-persistence"])
        .args(["--allowedTools", &format!("Read,Grep,Glob,Agent,Task,Skill,Bash(cargo test:*),Bash(cargo nextest:*),Bash(git diff:*),Bash(git show:*),Bash(git log:*),{PLUGIN_TOOLS}")])
        .env("JEV_RUST_REVIEW_BIN", root.join("target/release").join(exe))
        .env("JEV_RUST_REVIEW_CARGO_TARGET_DIR", target_dir)
        .env("CARGO_TARGET_DIR", target_dir)
        .env_remove("CLAUDE_PROJECT_DIR")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    if mode == "C" {
        cmd.env_remove("TYPESAFE_API_KEY")
            .env_remove("JEV_RUST_REVIEW_API_KEY");
    }
    let limit = Duration::from_secs(
        env("E2E_TIMEOUT_SECS")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1800),
    );
    tokio::time::timeout(limit, cmd.output())
        .await
        .unwrap_or_else(|_| Err(std::io::Error::other("timed out")))
}

async fn run_headless(fx: &Fixture, mode: &str, run: usize, target_dir: &Path) -> Cell {
    let repo = fx.repo();
    let started = Instant::now();
    let mut cell = Cell {
        mode: mode.into(),
        fixture: fx.name.clone(),
        kind: fx.kind.into(),
        run,
        ..Cell::default()
    };
    let output = claude(&repo.path(), mode, target_dir).await;
    cell.wall_ms = started.elapsed().as_millis();
    let stdout = match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(e) => {
            cell.status = format!("claude did not run: {e}");
            return cell;
        }
    };
    std::fs::write(
        out_dir()
            .join("transcripts")
            .join(format!("{mode}-{}-{run}.jsonl", fx.name)),
        &stdout,
    )
    .unwrap();
    let (calls, report) = read_transcript(&stdout, &mut cell);
    jev_facts(fx, &calls, &mut cell);
    let wanted = if mode == "C" { "jev_unavailable" } else { "ok" };
    let seen = cell.evaluate_status.clone().unwrap_or_default();
    cell.status = match report.as_deref().map(findings_block) {
        None => "no result message".into(),
        Some(None) => "report has no json findings block".into(),
        // A `partial` Jev run is still a Jev run.
        Some(Some(_)) if seen != wanted && !(mode == "J" && seen == "partial") => {
            format!("wrong mode: evaluate status was {seen:?}")
        }
        Some(Some(findings)) => {
            (cell.found, cell.unexpected) = grade(fx, &findings);
            cell.findings = findings;
            "ok".into()
        }
    };
    cell
}

// ------------------------------------------------------------------- report

fn cell_path(mode: &str, fixture: &str, run: usize) -> PathBuf {
    out_dir()
        .join("cells")
        .join(format!("{mode}-{fixture}-{run}.json"))
}

fn load_cells() -> Vec<Cell> {
    let mut cells: Vec<Cell> = std::fs::read_dir(out_dir().join("cells"))
        .unwrap()
        .filter_map(|e| std::fs::read_to_string(e.unwrap().path()).ok())
        .filter_map(|s| serde_json::from_str(&s).ok())
        .collect();
    cells.sort_by(|a: &Cell, b: &Cell| {
        (&a.mode, &a.kind, &a.fixture, a.run).cmp(&(&b.mode, &b.kind, &b.fixture, b.run))
    });
    cells
}

fn report(fixtures: &[Fixture], cells: &[Cell]) -> String {
    let tool_found: BTreeSet<&str> = cells
        .iter()
        .filter(|c| c.mode == "T" && c.found)
        .map(|c| c.fixture.as_str())
        .collect();
    let beyond: Vec<&Fixture> = fixtures
        .iter()
        .filter(|f| f.buggy() && !tool_found.contains(f.name.as_str()))
        .collect();
    let is_beyond = |c: &Cell| beyond.iter().any(|f| f.name == c.fixture);
    let mut md = String::from(
        "| Mode | Runs | Beyond-tooling bugs found | All seeded bugs found | False positives on clean fixtures | Extra findings on buggy fixtures | Claude tokens (in / out) | Claude cost | Jev tokens | Jev cost | Wall time |\n|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for mode in ["T", "C", "J"] {
        let mine: Vec<&Cell> = cells
            .iter()
            .filter(|c| c.mode == mode && c.status == "ok")
            .collect();
        if mine.is_empty() {
            continue;
        }
        let count = |p: &dyn Fn(&Cell) -> bool| mine.iter().filter(|c| p(c)).count();
        let buggy = count(&|c| c.kind == "buggy");
        let jev: u64 = mine.iter().map(|c| c.jev_input_tokens).sum();
        md.push_str(&format!(
            "| {mode} | {} | {}/{} | {}/{} | {} in {} runs | {} | {} / {} | ${:.2} | {} | ${:.4} | {:.0} min |\n",
            mine.len(),
            count(&|c| c.found && is_beyond(c)),
            count(&|c| is_beyond(c)),
            count(&|c| c.found),
            buggy,
            mine.iter().filter(|c| c.kind == "clean").map(|c| c.unexpected.len()).sum::<usize>(),
            count(&|c| c.kind == "clean"),
            mine.iter().filter(|c| c.kind == "buggy").map(|c| c.unexpected.len()).sum::<usize>(),
            mine.iter().map(|c| c.claude_input_tokens).sum::<u64>(),
            mine.iter().map(|c| c.claude_output_tokens).sum::<u64>(),
            mine.iter().map(|c| c.claude_usd).sum::<f64>(),
            jev,
            jev as f64 * USD_PER_INPUT_TOKEN,
            mine.iter().map(|c| c.wall_ms).sum::<u128>() as f64 / 60_000.0,
        ));
    }

    md.push_str("\n### Per fixture (runs that found the seeded bug)\n\n| Fixture | Kind | Label: tool catches | T | C | J |\n|---|---|---|---|---|---|\n");
    for f in fixtures {
        let score = |mode: &str| {
            let mine: Vec<&Cell> = cells
                .iter()
                .filter(|c| c.mode == mode && c.fixture == f.name && c.status == "ok")
                .collect();
            match (mine.len(), f.buggy()) {
                (0, _) => "-".to_string(),
                (n, true) => format!("{}/{n}", mine.iter().filter(|c| c.found).count()),
                (n, false) => format!(
                    "{} FP in {n}",
                    mine.iter().map(|c| c.unexpected.len()).sum::<usize>()
                ),
            }
        };
        let label = match (
            f.buggy(),
            f.spec.tool_catches,
            tool_found.contains(f.name.as_str()),
        ) {
            (false, ..) => "".to_string(),
            (_, l, t) if l == t => l.to_string(),
            (_, l, t) => format!("**label says {l}, tools say {t}**"),
        };
        md.push_str(&format!(
            "| {} | {} | {label} | {} | {} | {} |\n",
            f.name,
            f.kind,
            score("T"),
            score("C"),
            score("J")
        ));
    }

    md.push_str(&jev_stage_report(cells));
    md.push_str("\n### Findings for a person to judge\n\n");
    for c in cells.iter().filter(|c| !c.unexpected.is_empty()) {
        for u in &c.unexpected {
            md.push_str(&format!(
                "- {} {} run {} ({}): `{}:{}-{}` [{} / {}] {}\n",
                c.mode,
                c.fixture,
                c.run,
                c.kind,
                u.file,
                u.start_line,
                u.end_line,
                u.dimension,
                u.source,
                u.title
            ));
        }
    }
    let bad: Vec<&Cell> = cells.iter().filter(|c| c.status != "ok").collect();
    if !bad.is_empty() {
        md.push_str("\n### Cells that could not be graded\n\n");
        for c in bad {
            md.push_str(&format!(
                "- {} {} run {}: {}\n",
                c.mode, c.fixture, c.run, c.status
            ));
        }
    }
    md
}

/// Triage and verification judged apart, from mode J's own tool calls.
fn jev_stage_report(cells: &[Cell]) -> String {
    let j: Vec<&Cell> = cells
        .iter()
        .filter(|c| c.mode == "J" && c.status == "ok")
        .collect();
    if j.is_empty() {
        return String::new();
    }
    let buggy: Vec<&&Cell> = j.iter().filter(|c| c.kind == "buggy").collect();
    let verdicts = |on_seed: bool| {
        let mut by: BTreeMap<&str, usize> = BTreeMap::new();
        for v in j
            .iter()
            .flat_map(|c| &c.verified)
            .filter(|v| v.on_seed == on_seed)
        {
            *by.entry(v.verdict.as_str()).or_default() += 1;
        }
        let parts: Vec<String> = by.iter().map(|(k, n)| format!("{k} {n}")).collect();
        parts.join(", ")
    };
    format!(
        "\n### Jev's two stages, judged apart (mode J)\n\n- Triage flagged the seeded bug's lines in {}/{} buggy runs. It flagged {} of {} units overall.\n- Verification of candidates on the seeded bug: {}.\n- Verification of every other candidate: {}.\n",
        buggy.iter().filter(|c| c.triage_flagged_seed).count(),
        buggy.len(),
        j.iter().map(|c| c.flagged_units).sum::<usize>(),
        j.iter().map(|c| c.units).sum::<usize>(),
        verdicts(true),
        verdicts(false),
    )
}

#[tokio::test]
#[ignore = "opt-in: builds fixture crates and runs `claude -p` headless; see the module docs"]
async fn three_mode_eval() {
    let modes = env("E2E_MODES").unwrap_or_else(|| "T,C,J".into());
    let runs: usize = env("E2E_RUNS").and_then(|v| v.parse().ok()).unwrap_or(1);
    let jobs: usize = env("E2E_JOBS").and_then(|v| v.parse().ok()).unwrap_or(3);
    let only: Option<Vec<String>> =
        env("E2E_FIXTURES").map(|v| v.split(',').map(|s| s.trim().to_string()).collect());
    let fixtures: Arc<Vec<Fixture>> = Arc::new(
        load_fixtures()
            .into_iter()
            .filter(|f| only.as_ref().is_none_or(|o| o.contains(&f.name)))
            .collect(),
    );
    for d in ["cells", "transcripts", "target"] {
        std::fs::create_dir_all(out_dir().join(d)).unwrap();
    }
    let target_dir = out_dir().join("target");
    let save = |c: &Cell| {
        std::fs::write(
            cell_path(&c.mode, &c.fixture, c.run),
            serde_json::to_string_pretty(c).unwrap(),
        )
        .unwrap();
        eprintln!(
            "{} {} run {}: {} found={} unexpected={}",
            c.mode,
            c.fixture,
            c.run,
            c.status,
            c.found,
            c.unexpected.len()
        );
    };

    // T first and one at a time: it warms the shared target directory, and
    // it decides which bugs are beyond tooling.
    if modes.contains('T') {
        for f in fixtures.iter() {
            if !cell_path("T", &f.name, 1).exists() {
                save(&run_tools(f, &target_dir).await);
            }
        }
    }
    let beyond_or_clean: BTreeSet<String> = {
        let tool_found: BTreeSet<String> = load_cells()
            .into_iter()
            .filter(|c| c.mode == "T" && c.found)
            .map(|c| c.fixture)
            .collect();
        fixtures
            .iter()
            .filter(|f| !tool_found.contains(&f.name))
            .map(|f| f.name.clone())
            .collect()
    };

    // A bug the tools catch cannot count for C or J, so it is not run there.
    let pending: Vec<(&str, usize, usize)> = ["C", "J"]
        .into_iter()
        .filter(|m| modes.contains(m))
        .flat_map(|m| (1..=runs).map(move |run| (m, run)))
        .flat_map(|(m, run)| (0..fixtures.len()).map(move |i| (m, run, i)))
        .filter(|&(m, run, i)| {
            let name = &fixtures[i].name;
            !cell_path(m, name, run).exists() && beyond_or_clean.contains(name)
        })
        .collect();
    eprintln!("{} headless runs to do", pending.len());
    let sem = Arc::new(tokio::sync::Semaphore::new(jobs.max(1)));
    let mut set = tokio::task::JoinSet::new();
    for (mode, run, i) in pending {
        let (fixtures, sem, target_dir) = (fixtures.clone(), sem.clone(), target_dir.clone());
        set.spawn(async move {
            let _permit = sem.acquire_owned().await.unwrap();
            run_headless(&fixtures[i], mode, run, &target_dir).await
        });
    }
    while let Some(cell) = set.join_next().await {
        save(&cell.unwrap());
    }

    let md = report(&fixtures, &load_cells());
    std::fs::write(out_dir().join("report.md"), &md).unwrap();
    eprintln!("\n{md}");
}

#[test]
fn a_finding_is_graded_by_range_and_dimension() {
    let fx = load_fixtures()
        .into_iter()
        .find(|f| f.name == "select_cancellation")
        .unwrap();
    let (s, e) = fx.claim_lines();
    let finding = |dimension: &str, start, end| Reported {
        severity: "high".into(),
        dimension: dimension.into(),
        file: "src/conn.rs".into(),
        start_line: start,
        end_line: end,
        title: String::new(),
        source: "review".into(),
    };
    assert!(grade(&fx, &[finding("async", s, e)]).0);
    assert!(grade(&fx, &[finding("Async", s.saturating_sub(2), s)]).0);
    // The right lines in the wrong dimension, and the right dimension on the
    // wrong lines, are both listed for a person and not counted.
    for miss in [finding("idiom", s, e), finding("async", e + 1, e + 3)] {
        let (found, unexpected) = grade(&fx, std::slice::from_ref(&miss));
        assert!(!found);
        assert_eq!(unexpected, [miss]);
    }

    let clean = load_fixtures().into_iter().find(|f| !f.buggy()).unwrap();
    let (found, unexpected) = grade(&clean, &[finding("async", 1, 1)]);
    assert!(!found);
    assert_eq!(
        unexpected.len(),
        1,
        "any finding on a clean fixture is a false positive"
    );
}

#[test]
fn the_report_block_is_read_from_the_end() {
    let report = "Text\n```json\n{\"findings\":[]}\n```\nmore\n```json\n{\"findings\":[{\"severity\":\"high\",\"dimension\":\"async\",\"file\":\"src/a.rs\",\"start_line\":3,\"end_line\":5,\"title\":\"t\",\"source\":\"review\"}]}\n```\n";
    let got = findings_block(report).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].start_line, 3);
    assert!(findings_block("no block").is_none());
}

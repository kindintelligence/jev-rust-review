//! The deterministic layer, run against the real toolchain: Clippy's JSON
//! output is filtered to the change, project lint policy wins, and a defect a
//! tool reported is never reported again by triage or verification.
//!
//! These tests spawn `cargo clippy` on tiny dependency-free crates, so they
//! need no network. They skip themselves when Clippy is not installed.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::field_reassign_with_default,
    reason = "test code: failures should panic loudly, and tests tweak one config field at a time"
)]

mod common;

use common::{ScriptedJev, TestRepo};
use jev_rust_review::cargo_tools::EXTRA_LINTS;
use jev_rust_review::config::Config;
use jev_rust_review::jev::Client;
use jev_rust_review::questions;
use jev_rust_review::review::{self, DiagnosticsOutput, EvaluateParams, Finding};
use std::process::Command;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer};

const MANIFEST: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

const BEFORE: &str = "\
pub fn header(len: u16) -> [u8; 2] {
    len.to_be_bytes()
}

pub fn untouched(n: u64) -> u32 {
    n as u32
}
";

/// The change adds one lossy cast. `untouched` has the same defect on a line
/// the change does not touch.
const AFTER: &str = "\
pub fn header(len: u16) -> [u8; 2] {
    len.to_be_bytes()
}

pub fn untouched(n: u64) -> u32 {
    n as u32
}

pub fn frame(payload: &[u8]) -> [u8; 2] {
    header(payload.len() as u16)
}
";

#[expect(
    clippy::disallowed_methods,
    reason = "tests ask the toolchain what it has installed"
)]
fn tool_output(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn clippy_installed() -> bool {
    let ok = tool_output("cargo", &["clippy", "--version"]).is_some();
    if !ok {
        eprintln!("skipped: cargo clippy is not installed");
    }
    ok
}

fn repo(manifest: &str, after: &str) -> TestRepo {
    let r = TestRepo::new();
    r.write("Cargo.toml", manifest);
    r.write("src/lib.rs", BEFORE);
    r.write(".gitignore", "target\nCargo.lock\n");
    r.commit_all("before");
    r.write("src/lib.rs", after);
    r
}

async fn diagnose(r: &TestRepo) -> DiagnosticsOutput {
    // cargo-semver-checks has its own test; it is slow and optional.
    let mut cfg = Config::default();
    cfg.run_semver_checks = false;
    let out = review::diagnostics(&cfg, &r.path(), None).await.unwrap();
    assert_eq!(out.status, "ok", "{:?}", out.reason);
    out
}

fn codes(out: &DiagnosticsOutput) -> Vec<(&str, u32)> {
    out.diagnostics
        .iter()
        .map(|d| (d.code.as_deref().unwrap_or(""), d.lines.0))
        .collect()
}

#[tokio::test]
async fn only_the_changed_line_is_reported() {
    if !clippy_installed() {
        return;
    }
    let out = diagnose(&repo(MANIFEST, AFTER)).await;
    assert_eq!(codes(&out), [("clippy::cast_possible_truncation", 10)]);
    assert!(out.diagnostics[0].on_changed_lines);
    // The same lint fired on line 6, which the change did not touch.
    assert_eq!(out.warnings_outside_change, 1);
    assert!(
        out.extra_lints
            .contains(&"clippy::cast_possible_truncation".to_string())
    );
}

#[tokio::test]
async fn a_compiler_error_outside_the_change_is_kept() {
    if !clippy_installed() {
        return;
    }
    // Changing `header`'s parameter breaks `frame`'s caller further down.
    let broken = AFTER.replace("pub fn header(len: u16)", "pub fn header(len: u8)");
    let r = repo(MANIFEST, AFTER);
    r.commit_all("after");
    r.write("src/lib.rs", &broken);
    let out = diagnose(&r).await;
    assert_eq!(out.build_ok, Some(false));
    let errors: Vec<_> = out
        .diagnostics
        .iter()
        .filter(|d| d.level == jev_rust_review::cargo_tools::Level::Error)
        .collect();
    // The error sits in `frame`, on a line the change did not touch. It is
    // kept because it is an error, and its note points back at the changed
    // signature.
    let e = errors
        .iter()
        .find(|d| d.lines == (10, 10))
        .expect("a type error in the caller");
    assert!(e.related.iter().any(|(_, lines)| lines.0 == 1));
}

#[tokio::test]
async fn project_policy_wins_in_the_manifest_and_in_code() {
    if !clippy_installed() {
        return;
    }
    // Control: with no policy the lint is reported.
    assert_eq!(codes(&diagnose(&repo(MANIFEST, AFTER)).await).len(), 1);

    // `[lints]` loses to a command-line `-W`, so the server has to honour it.
    let allowed = format!("{MANIFEST}\n[lints.clippy]\ncast_possible_truncation = \"allow\"\n");
    let out = diagnose(&repo(&allowed, AFTER)).await;
    assert_eq!(codes(&out), []);
    assert!(
        !out.extra_lints
            .contains(&"clippy::cast_possible_truncation".to_string()),
        "a lint the project decided is not passed at all"
    );

    // A group allow counts too.
    let group = format!("{MANIFEST}\n[lints.clippy]\npedantic = \"allow\"\n");
    assert_eq!(codes(&diagnose(&repo(&group, AFTER)).await), []);

    // `#[allow]` in code.
    let in_code = AFTER.replace(
        "pub fn frame(",
        "#[allow(clippy::cast_possible_truncation)]\npub fn frame(",
    );
    assert_eq!(codes(&diagnose(&repo(MANIFEST, &in_code)).await), []);
}

#[tokio::test]
async fn a_tool_reported_defect_is_not_reported_again() {
    if !clippy_installed() {
        return;
    }
    let r = repo(MANIFEST, AFTER);
    let server = MockServer::start().await;
    // Jev flags the cast and would support any claim about it.
    Mock::given(method("POST"))
        .respond_with(ScriptedJev {
            high: vec!["correctness.cast".into(), "supported".into()],
        })
        .mount(&server)
        .await;
    let mut cfg = Config::default();
    cfg.api_url = server.uri();
    cfg.api_key = Some("k".into());
    let client = Client::new(&cfg);
    let cast = Finding {
        id: None,
        dimension: "correctness".into(),
        file: "src/lib.rs".into(),
        start_line: 10,
        end_line: 10,
        claim: "frame casts payload.len() to u16, which truncates long payloads.".into(),
        severity: "high".into(),
    };

    // Before the tools have run there is nothing to deduplicate against.
    let ev = review::evaluate(&cfg, &client, &r.path(), EvaluateParams::default())
        .await
        .unwrap();
    assert!(!ev.tool_diagnostics_seen);
    assert!(ev.flagged.iter().any(|f| f.question == "correctness.cast"));

    diagnose(&r).await;

    let ev = review::evaluate(&cfg, &client, &r.path(), EvaluateParams::default())
        .await
        .unwrap();
    assert!(ev.tool_diagnostics_seen);
    assert!(ev.flagged.iter().all(|f| f.question != "correctness.cast"));
    let covered = &ev.tool_covered[0];
    assert_eq!(covered.question, "correctness.cast");
    assert_eq!(covered.tool, "clippy::cast_possible_truncation");
    assert_eq!(covered.tool_lines, Some((10, 10)));

    // A Claude finding on the same lines in the same dimension is a
    // duplicate. A finding in another dimension there is not, and neither is
    // a correctness finding somewhere else.
    let elsewhere = Finding {
        start_line: 1,
        end_line: 2,
        ..cast.clone()
    };
    let other_dimension = Finding {
        dimension: "security".into(),
        ..cast.clone()
    };
    let vr = review::verify(
        &cfg,
        &client,
        &r.path(),
        None,
        vec![cast.clone(), elsewhere, other_dimension],
        false,
    )
    .await
    .unwrap();
    let verdicts: Vec<&str> = vr.results.iter().map(|x| x.verdict).collect();
    assert_eq!(verdicts, ["tool_reported", "report", "report"]);
    assert_eq!(
        vr.results[0].tool.as_ref().unwrap().tool,
        "clippy::cast_possible_truncation"
    );
    assert_eq!(vr.usage.requests, 2, "the duplicate is never sent to Jev");

    // The duplicate check is code, so it works with no API key at all.
    let mut offline = Config::default();
    offline.api_url = server.uri();
    let vr = review::verify(
        &offline,
        &Client::new(&offline),
        &r.path(),
        None,
        vec![cast],
        false,
    )
    .await
    .unwrap();
    assert_eq!(vr.status, "jev_unavailable");
    assert_eq!(vr.results[0].verdict, "tool_reported");
}

#[tokio::test]
async fn a_changed_pub_surface_goes_to_semver_checks_when_installed() {
    if !clippy_installed() {
        return;
    }
    let renamed = AFTER.replace("pub fn untouched(", "pub fn renamed(");
    let r = repo(MANIFEST, &renamed);
    let out = review::diagnostics(&Config::default(), &r.path(), None)
        .await
        .unwrap();
    let semver = out
        .semver
        .expect("a removed `pub fn` is a pub surface change");
    assert_eq!(semver.crates, ["demo"]);
    assert!(semver.baseline.is_some());
    if tool_output("cargo", &["semver-checks", "--version"]).is_some() {
        assert_eq!(semver.status, "breaking", "{:?}", semver.note);
        assert!(semver.breaks.iter().any(|b| b.lint == "function_missing"));
    } else {
        // Never installed for the user; one line says it was not checked.
        assert_eq!(semver.status, "absent");
        assert!(semver.note.unwrap().contains("not installed"));
    }
}

/// "Do not trust my list": every lint this crate names must exist in the
/// installed toolchain, at the level and in the group the code assumes.
#[test]
fn lints_exist_in_installed_clippy() {
    let Some(help) = tool_output("clippy-driver", &["-W", "help"]) else {
        eprintln!("skipped: clippy-driver is not installed");
        return;
    };
    // `clippy-driver -W help` lists rustc's lints and then Clippy's, one per
    // line as `name  level  description`, with `-` in place of `_`.
    let level = |lint: &str| -> Option<String> {
        let dashed = lint.replace('_', "-");
        help.lines().find_map(|l| {
            let mut words = l.split_whitespace();
            (words.next()? == dashed).then(|| words.next().unwrap_or("").to_string())
        })
    };
    let group_members = |group: &str| -> String {
        help.lines()
            .find(|l| l.trim_start().starts_with(&format!("clippy::{group} ")))
            .unwrap_or_else(|| panic!("no clippy::{group} group"))
            .replace('-', "_")
    };
    for l in EXTRA_LINTS {
        let name = format!("clippy::{}", l.name);
        assert_eq!(
            level(&name).as_deref(),
            Some("allow"),
            "{name} must exist and be off by default, or passing -W is pointless"
        );
        assert!(
            group_members(l.group).contains(&format!("{name},"))
                || group_members(l.group).ends_with(&name),
            "{name} is not in clippy::{}",
            l.group
        );
    }
    let named = questions::all_specs()
        .flat_map(|(q, _)| q.tool_overlap.iter().map(move |l| (q.id, *l)))
        .chain(questions::TOOL_OWNED.iter().map(|(d, l)| (d.name(), *l)));
    for (owner, lint) in named {
        if [questions::SEMVER_CHECKS, questions::DX_CHECK].contains(&lint) {
            continue;
        }
        assert!(
            level(lint).is_some(),
            "{owner}: `{lint}` is not a lint of the installed toolchain"
        );
    }
}

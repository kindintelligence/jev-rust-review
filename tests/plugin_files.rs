//! The plugin's Markdown and JSON must agree with the code: versions, MCP
//! tool names, and a reference file for every dimension and profile.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test code: failures should panic loudly"
)]

use jev_rust_review::questions::{self, Dimension};
use std::path::Path;

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)).unwrap()
}

#[test]
fn plugin_version_matches_crate() {
    let plugin: serde_json::Value =
        serde_json::from_str(&read(".claude-plugin/plugin.json")).unwrap();
    assert_eq!(plugin["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(plugin["name"], "jev-rust-review");
}

#[test]
fn skill_names_the_real_tools() {
    let mcp: serde_json::Value = serde_json::from_str(&read(".mcp.json")).unwrap();
    let server = mcp["mcpServers"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let server_src = read("src/mcp.rs");
    let skill = read("skills/rust-review/SKILL.md");
    for tool in [
        "cargo_diagnostics",
        "evaluate_rust_changes",
        "verify_rust_findings",
    ] {
        assert!(
            server_src.contains(&format!("async fn {tool}(")),
            "{tool} missing from mcp.rs"
        );
        let full = format!("mcp__plugin_jev-rust-review_{server}__{tool}");
        assert!(skill.contains(&full), "SKILL.md must allow {full}");
    }
}

#[test]
fn every_dimension_and_profile_has_a_reference() {
    for d in Dimension::ALL {
        let rel = format!("skills/rust-review/{}", questions::reference_for(*d));
        assert!(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(&rel).is_file(),
            "{rel}"
        );
    }
    for p in questions::PROFILES {
        let rel = format!("skills/rust-review/{}", p.reference);
        assert!(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(&rel).is_file(),
            "{rel}"
        );
    }
}

#[test]
fn verification_verdicts_are_documented() {
    let skill = read("skills/rust-review/SKILL.md");
    for verdict in [
        "report",
        "insufficient_context",
        "uncertain",
        "dismiss",
        "tool_reported",
    ] {
        assert!(
            skill.contains(&format!("`verdict: {verdict}`")),
            "{verdict}"
        );
    }
}

/// MCP clients other than Claude Code (Codex, for example) have only the tool
/// descriptions to go on, so they must describe the current verdict model.
#[test]
fn tool_descriptions_describe_the_current_model() {
    let src = read("src/mcp.rs");
    let verify = src
        .split("async fn verify_rust_findings")
        .next()
        .and_then(|before| before.rsplit("description = \"").next())
        .unwrap();
    for needed in [
        "supported / refuted / insufficient_context",
        "report | insufficient_context | uncertain | dismiss",
        "severity` score",
        "real_defect / debatable_tradeoff / style_preference",
    ] {
        assert!(
            verify.contains(needed),
            "verify description lacks {needed:?}"
        );
    }
    assert!(
        !verify.contains("not_supported"),
        "verify description mentions not_supported"
    );
    assert!(
        src.contains("whether the diff touches tests"),
        "evaluate description lacks test facts"
    );
}

/// The headline example must be a defect no tool reports. A guard held
/// across `.await` is Clippy's by default, so it must not come back.
#[test]
fn examples_lead_with_a_defect_beyond_tooling() {
    for file in [
        "README.md",
        "skills/rust-review/SKILL.md",
        "agents/rust-reviewer.md",
    ] {
        let text = read(file);
        assert!(text.contains("select!"), "{file} lost its headline example");
        assert!(
            !text.contains("MutexGuard is held across")
                && !text.contains("MutexGuard named guard is held across"),
            "{file} leads with a defect Clippy reports by default"
        );
    }
}

/// Every lint the diagnostics tool switches on is named in its description,
/// in plain words, so a client knows what the facts cover.
#[test]
fn skill_runs_the_tools_before_triage() {
    let skill = read("skills/rust-review/SKILL.md");
    let tools = skill.find("## 2. Collect the tool facts").unwrap();
    let triage = skill.find("## 3. Triage with Jev").unwrap();
    assert!(tools < triage);
    for needed in [
        "cargo-semver-checks",
        "Never install",
        "Miri",
        "tool_covered",
    ] {
        assert!(skill.contains(needed), "SKILL.md lacks {needed:?}");
    }
}

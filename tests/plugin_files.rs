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
    for tool in ["evaluate_rust_changes", "verify_rust_findings"] {
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
    for verdict in ["report", "insufficient_context", "uncertain", "dismiss"] {
        assert!(
            skill.contains(&format!("`verdict: {verdict}`")),
            "{verdict}"
        );
    }
}

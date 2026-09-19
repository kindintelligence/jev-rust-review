//! Output types of the evaluation tool.

use crate::context::{Skip, Unit};
use crate::jev::{self};
use crate::questions::{Dimension, QuestionSpec};
use crate::redact::Redactions;
use crate::rust_project::ProjectInfo;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Default)]
pub struct EvaluateParams {
    pub scope: Option<String>,
    pub dry_run: bool,
    pub profiles: Option<Vec<String>>,
    pub max_units: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct QuestionResult {
    pub question: &'static str,
    pub dimension: Dimension,
    pub primitive: &'static str,
    /// Noul probability, Choice option, or Score value, as Jev returned it.
    pub answer: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probabilities: Option<BTreeMap<String, f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// The number compared against `threshold`: the Noul probability, or the
    /// probability mass on the flagging levels/options.
    pub signal: f64,
    pub threshold: f64,
    pub flagged: bool,
}

#[derive(Debug, Serialize)]
pub struct UnitResult {
    #[serde(flatten)]
    pub unit: Unit,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Questions that were gated in and asked.
    pub asked: usize,
    pub results: Vec<QuestionResult>,
}

#[derive(Debug, Serialize)]
pub struct Flag {
    pub unit: String,
    pub file: String,
    pub lines: (u32, u32),
    pub changed_lines: Vec<(u32, u32)>,
    pub dimension: Dimension,
    pub question: &'static str,
    pub signal: f64,
    pub threshold: f64,
}

#[derive(Debug, Serialize, Default)]
pub struct DepChange {
    pub name: String,
    pub section: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct ManifestFacts {
    pub file: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub added_dependencies: Vec<DepChange>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed_dependencies: Vec<DepChange>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changed_dependencies: Vec<DepChange>,
    /// Newly added or changed dependencies with risky sources or versions.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed_features: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub default_features_changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edition_change: Option<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_version_change: Option<(String, String)>,
}

#[derive(Debug, Serialize, Default)]
pub struct LockfileFacts {
    pub file: String,
    pub packages_added: usize,
    pub packages_removed: usize,
    pub packages_updated: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sample: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct CargoFacts {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub manifests: Vec<ManifestFacts>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lockfiles: Vec<LockfileFacts>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub new_build_scripts: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub deleted_rust_files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CargoPlan {
    pub enabled: bool,
    pub commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct UsageOut {
    pub requests: usize,
    pub input_tokens: u64,
    pub estimated_usd: f64,
}

#[derive(Debug, Serialize)]
pub struct EvaluateOutput {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub repo: String,
    pub scope: String,
    pub model: String,
    pub project: ProjectInfo,
    pub active_profiles: Vec<String>,
    pub flagged: Vec<Flag>,
    /// Reference files the reviewer should load (relative to the skill dir).
    pub references: Vec<String>,
    pub units: Vec<UnitResult>,
    pub cargo_facts: CargoFacts,
    pub skipped: Vec<Skip>,
    pub redactions: BTreeMap<String, usize>,
    pub usage: UsageOut,
    pub cargo: CargoPlan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payloads: Option<Vec<serde_json::Value>>,
}

/// Everything computed before any network call.
pub struct Prepared {
    pub repo: String,
    pub scope: String,
    pub project: ProjectInfo,
    pub units: Vec<(Unit, Vec<&'static QuestionSpec>, jev::Request)>,
    pub skipped: Vec<Skip>,
    pub redactions: Redactions,
    pub cargo_facts: CargoFacts,
    pub active_profiles: BTreeSet<String>,
}

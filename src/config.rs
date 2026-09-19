//! Configuration from environment variables. No config framework: one
//! struct, one constructor, documented in the README table.

use std::collections::HashMap;
use std::time::Duration;

pub const DEFAULT_API_URL: &str = "https://api.typesafe.ai";
/// Pinned rather than `jev-latest`: the Models page advises pinning the
/// version thresholds were tuned against.
pub const DEFAULT_MODEL: &str = "jev-1.13.0";
/// USD per input token (docs: $0.042 per Mtok; output tokens are free).
pub const USD_PER_INPUT_TOKEN: f64 = 0.042 / 1_000_000.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Profiles {
    Auto,
    None,
    Only(Vec<String>),
}

#[derive(Clone)]
pub struct Config {
    pub api_key: Option<String>,
    pub api_url: String,
    pub model: String,
    pub triage_thresholds: HashMap<String, f64>,
    pub report_thresholds: HashMap<String, f64>,
    pub dismiss_below: f64,
    pub max_unit_tokens: usize,
    pub max_total_tokens: usize,
    pub max_units: usize,
    pub concurrency: usize,
    pub timeout: Duration,
    pub max_retries: u32,
    /// Upper bound on any single backoff sleep, including `Retry-After`.
    pub max_backoff: Duration,
    pub profiles: Profiles,
    pub run_cargo: bool,
    /// Run cargo-semver-checks when it is installed and a library's `pub`
    /// surface changed.
    pub run_semver_checks: bool,
    /// `CARGO_TARGET_DIR` for the server's own cargo runs. Unset means the
    /// project's target directory, which shares its build cache and its lock.
    pub cargo_target_dir: Option<String>,
    /// Upper bound on one cargo run (Clippy, or cargo-semver-checks).
    pub cargo_timeout: Duration,
    pub dry_run: bool,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("api_key", &self.api_key.as_ref().map(|_| "<set>"))
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .field("max_unit_tokens", &self.max_unit_tokens)
            .field("max_total_tokens", &self.max_total_tokens)
            .field("concurrency", &self.concurrency)
            .field("profiles", &self.profiles)
            .field("dry_run", &self.dry_run)
            .finish_non_exhaustive()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_lookup(|_| None)
    }
}

impl Config {
    pub fn from_env() -> Self {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        // An unset plugin userConfig value may arrive empty or as the literal
        // placeholder; treat both as absent.
        let present = |k: &str| {
            get(k)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty() && !v.starts_with("${"))
        };
        let num = |k: &str, d: usize| {
            present(k)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(d)
        };
        let float = |k: &str, d: f64| {
            present(k)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| (0.0..=1.0).contains(v))
                .unwrap_or(d)
        };
        let flag = |k: &str, d: bool| match present(k).as_deref() {
            Some("1" | "true" | "yes" | "on") => true,
            Some("0" | "false" | "no" | "off") => false,
            _ => d,
        };
        let profiles = match present("JEV_RUST_REVIEW_PROFILES").as_deref() {
            None | Some("auto") => Profiles::Auto,
            Some("none") => Profiles::None,
            Some(list) => Profiles::Only(
                list.split(',')
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect(),
            ),
        };
        Config {
            api_key: present("TYPESAFE_API_KEY").or_else(|| present("JEV_RUST_REVIEW_API_KEY")),
            api_url: present("JEV_RUST_REVIEW_API_URL")
                .unwrap_or_else(|| DEFAULT_API_URL.to_string())
                .trim_end_matches('/')
                .to_string(),
            model: present("JEV_RUST_REVIEW_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            triage_thresholds: parse_thresholds(
                present("JEV_RUST_REVIEW_TRIAGE_THRESHOLDS").as_deref(),
            ),
            report_thresholds: parse_thresholds(
                present("JEV_RUST_REVIEW_REPORT_THRESHOLDS").as_deref(),
            ),
            dismiss_below: float("JEV_RUST_REVIEW_DISMISS_BELOW", 0.40),
            max_unit_tokens: num("JEV_RUST_REVIEW_MAX_UNIT_TOKENS", 6_000).min(28_000),
            max_total_tokens: num("JEV_RUST_REVIEW_MAX_TOTAL_TOKENS", 400_000),
            max_units: num("JEV_RUST_REVIEW_MAX_UNITS", 60),
            concurrency: num("JEV_RUST_REVIEW_CONCURRENCY", 4).min(32),
            timeout: Duration::from_secs(num("JEV_RUST_REVIEW_TIMEOUT_SECS", 30) as u64),
            max_retries: num("JEV_RUST_REVIEW_MAX_RETRIES", 3).min(10) as u32,
            max_backoff: Duration::from_secs(30),
            profiles,
            run_cargo: flag("JEV_RUST_REVIEW_CARGO", true),
            cargo_target_dir: present("JEV_RUST_REVIEW_CARGO_TARGET_DIR"),
            run_semver_checks: flag("JEV_RUST_REVIEW_SEMVER_CHECKS", true),
            cargo_timeout: Duration::from_secs(
                num("JEV_RUST_REVIEW_CARGO_TIMEOUT_SECS", 600) as u64
            ),
            dry_run: flag("JEV_RUST_REVIEW_DRY_RUN", false),
        }
    }
}

/// Parse `"unsafe=0.2, idiom=0.7"`. Invalid entries are ignored.
pub fn parse_thresholds(s: Option<&str>) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for part in s.unwrap_or("").split(',') {
        if let Some((k, v)) = part.split_once('=')
            && let Ok(v) = v.trim().parse::<f64>()
            && (0.0..=1.0).contains(&v)
        {
            out.insert(k.trim().to_ascii_lowercase(), v);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(pairs: &[(&str, &str)]) -> Config {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(|k| m.get(k).cloned())
    }

    #[test]
    fn defaults() {
        let c = cfg(&[]);
        assert!(c.api_key.is_none());
        assert_eq!(c.model, DEFAULT_MODEL);
        assert_eq!(c.profiles, Profiles::Auto);
        assert!(c.run_cargo);
        assert!(!c.dry_run);
    }

    #[test]
    fn placeholder_key_is_absent() {
        assert!(
            cfg(&[("TYPESAFE_API_KEY", "${user_config.typesafe_api_key}")])
                .api_key
                .is_none()
        );
        assert!(cfg(&[("TYPESAFE_API_KEY", "  ")]).api_key.is_none());
        assert_eq!(
            cfg(&[("JEV_RUST_REVIEW_API_KEY", "k")]).api_key.as_deref(),
            Some("k")
        );
    }

    #[test]
    fn debug_masks_key() {
        let c = cfg(&[("TYPESAFE_API_KEY", "super-secret-value")]);
        assert!(!format!("{c:?}").contains("super-secret-value"));
    }

    #[test]
    fn thresholds_parse_and_reject_garbage() {
        let t = parse_thresholds(Some("unsafe=0.2, Idiom = 0.7, bad, x=2, y=abc"));
        assert_eq!(t.get("unsafe"), Some(&0.2));
        assert_eq!(t.get("idiom"), Some(&0.7));
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn profiles_and_flags() {
        let c = cfg(&[
            ("JEV_RUST_REVIEW_PROFILES", "Tokio, axum"),
            ("JEV_RUST_REVIEW_CARGO", "0"),
            ("JEV_RUST_REVIEW_DRY_RUN", "true"),
            ("JEV_RUST_REVIEW_MAX_UNIT_TOKENS", "999999"),
        ]);
        assert_eq!(
            c.profiles,
            Profiles::Only(vec!["tokio".into(), "axum".into()])
        );
        assert!(!c.run_cargo);
        assert!(c.dry_run);
        assert_eq!(c.max_unit_tokens, 28_000);
    }
}

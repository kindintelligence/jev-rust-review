#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "shared test helpers: failures should panic loudly"
)]

pub mod fixtures;

use std::path::{Path, PathBuf};
use std::process::Command;
use wiremock::{Request, Respond, ResponseTemplate};

pub struct TestRepo {
    pub dir: tempfile::TempDir,
}

#[expect(
    clippy::disallowed_methods,
    reason = "tests drive git directly to build fixture repositories"
)]
fn git_cmd() -> Command {
    Command::new("git")
}

impl TestRepo {
    pub fn new() -> TestRepo {
        let dir = tempfile::tempdir().unwrap();
        let r = TestRepo { dir };
        r.git(&["init", "-q", "-b", "main"]);
        r.git(&["config", "user.email", "t@example.com"]);
        r.git(&["config", "user.name", "t"]);
        r.git(&["config", "commit.gpgsign", "false"]);
        r.git(&["config", "core.autocrlf", "false"]);
        r
    }

    pub fn path(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    pub fn write(&self, rel: &str, content: &str) {
        self.write_bytes(rel, content.as_bytes());
    }

    pub fn write_bytes(&self, rel: &str, content: &[u8]) {
        let p = self.dir.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    pub fn git(&self, args: &[&str]) -> String {
        let out = git_cmd()
            .arg("-C")
            .arg(self.dir.path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    pub fn commit_all(&self, msg: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", msg]);
        self.git(&["rev-parse", "HEAD"])
    }
}

pub const MANIFEST: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\nrust-version = \"1.80\"\n\n[dependencies]\ntokio = { version = \"1\", features = [\"full\"] }\n";

/// A mock Jev that answers every question it receives, steered by `high`:
/// - a Noul whose id is in `high` gets 0.9, otherwise 0.05;
/// - a Choice picks the first option named in `high`, otherwise its first
///   option, with 0.85 on the pick;
/// - a Score puts 0.85 on level 2 (or the top level) if its id is in `high`,
///   otherwise on level 0.
pub struct ScriptedJev {
    pub high: Vec<String>,
}

impl ScriptedJev {
    fn wants(&self, name: &str) -> bool {
        self.high.iter().any(|h| h == name)
    }

    fn answer(&self, id: &str, q: &serde_json::Value) -> serde_json::Value {
        match q["type"].as_str().unwrap() {
            "noul" => {
                let v = if self.wants(id) { 0.9 } else { 0.05 };
                serde_json::json!({"type": "noul", "noul": v})
            }
            "choice" => self.choice_answer(q),
            "score" => {
                let n = q["criteria"].as_array().unwrap().len();
                let pick = if self.wants(id) { 2.min(n - 1) } else { 0 };
                spread_score(n, pick)
            }
            other => panic!("unexpected question type {other}"),
        }
    }

    fn choice_answer(&self, q: &serde_json::Value) -> serde_json::Value {
        let opts: Vec<String> = q["criteria"].as_object().unwrap().keys().cloned().collect();
        let pick = opts.iter().position(|o| self.wants(o)).unwrap_or(0);
        let rest = 0.15 / (opts.len() - 1) as f64;
        let probs: serde_json::Map<String, serde_json::Value> = opts
            .iter()
            .enumerate()
            .map(|(i, o)| (o.clone(), (if i == pick { 0.85 } else { rest }).into()))
            .collect();
        serde_json::json!({"type": "choice", "choice": opts[pick], "probabilities": probs, "confidence": 0.8})
    }
}

/// A Score answer over `n` levels with 0.85 on `pick`.
fn spread_score(n: usize, pick: usize) -> serde_json::Value {
    let rest = 0.15 / (n - 1) as f64;
    let probs: serde_json::Map<String, serde_json::Value> = (0..n)
        .map(|i| (i.to_string(), (if i == pick { 0.85 } else { rest }).into()))
        .collect();
    let score: f64 = (0..n)
        .map(|i| i as f64 * if i == pick { 0.85 } else { rest })
        .sum();
    serde_json::json!({"type": "score", "score": score, "confidence": 0.7, "probabilities": probs})
}

impl Respond for ScriptedJev {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        let answers: serde_json::Map<String, serde_json::Value> = body["questions"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(id, q)| (id.clone(), self.answer(id, q)))
            .collect();
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "jev-1.13.0",
            "answers": answers,
            "usage": {"input_tokens": 100, "output_tokens": 10}
        }))
    }
}

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

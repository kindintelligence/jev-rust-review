//! Eval corpus: seeded-bug and clean ("bait") fixtures under `fixtures/`.
//!
//! - `corpus_gates_cover_expected_dimensions` (offline): every buggy
//!   fixture's expected dimension is actually asked of Jev.
//! - `recorded_answers_meet_targets` (offline): replays answers recorded from
//!   real Jev through the current pipeline and thresholds, so a threshold or
//!   question change that regresses the measured numbers fails CI.
//! - `live_eval` (`#[ignore]`, needs `TYPESAFE_API_KEY`): runs the corpus
//!   against real Jev, prints recall / false-flag / verification numbers and
//!   token cost, and with `JEV_EVAL_RECORD=1` rewrites the recordings.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::field_reassign_with_default,
    reason = "test code: failures should panic loudly, and tests tweak one config field at a time"
)]

mod common;

use common::{TestRepo, fixtures_dir};
use jev_rust_review::config::Config;
use jev_rust_review::jev::Client;
use jev_rust_review::review::{self, EvaluateOutput, EvaluateParams, Finding, VerifyOutput};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

#[derive(Debug, Deserialize)]
struct FixtureToml {
    description: String,
    file: String,
    #[serde(default)]
    expected_dimensions: Vec<String>,
    #[serde(default)]
    deps: Vec<String>,
    claim: ClaimToml,
}

#[derive(Debug, Deserialize)]
struct ClaimToml {
    dimension: String,
    start: String,
    end: String,
    text: String,
    severity: String,
}

struct Fixture {
    kind: &'static str,
    name: String,
    dir: PathBuf,
    spec: FixtureToml,
}

impl Fixture {
    fn buggy(&self) -> bool {
        self.kind == "buggy"
    }

    fn read(&self, f: &str) -> String {
        std::fs::read_to_string(self.dir.join(f))
            .unwrap()
            .replace("\r\n", "\n")
    }

    fn repo(&self) -> TestRepo {
        let r = TestRepo::new();
        let mut manifest = String::from(
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
        );
        for d in &self.spec.deps {
            manifest.push_str(d);
            manifest.push('\n');
        }
        r.write("Cargo.toml", &manifest);
        r.write(&self.spec.file, &self.read("before.rs"));
        r.commit_all("before");
        r.write(&self.spec.file, &self.read("after.rs"));
        r
    }

    /// The claim's line range, located by substring so fixtures stay
    /// editable without renumbering.
    fn claim_lines(&self) -> (u32, u32) {
        let after = self.read("after.rs");
        let lines: Vec<&str> = after.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.contains(&self.spec.claim.start))
            .unwrap_or_else(|| panic!("{}: start marker not found", self.name));
        let end = lines
            .iter()
            .skip(start)
            .position(|l| l.contains(&self.spec.claim.end))
            .map(|i| i + start)
            .unwrap_or_else(|| panic!("{}: end marker not found", self.name));
        (start as u32 + 1, end as u32 + 1)
    }

    fn finding(&self) -> Finding {
        let (s, e) = self.claim_lines();
        Finding {
            id: Some(self.name.clone()),
            dimension: self.spec.claim.dimension.clone(),
            file: self.spec.file.clone(),
            start_line: s,
            end_line: e,
            claim: self.spec.claim.text.clone(),
            severity: self.spec.claim.severity.clone(),
        }
    }

    fn recording_path(&self) -> PathBuf {
        self.dir.join("recorded.json")
    }
}

fn load_fixtures() -> Vec<Fixture> {
    let mut out = Vec::new();
    for kind in ["buggy", "clean"] {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(fixtures_dir().join(kind))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.join("fixture.toml").is_file())
            .collect();
        dirs.sort();
        for dir in dirs {
            let spec: FixtureToml =
                toml::from_str(&std::fs::read_to_string(dir.join("fixture.toml")).unwrap())
                    .unwrap();
            out.push(Fixture {
                kind,
                name: dir.file_name().unwrap().to_string_lossy().into_owned(),
                dir,
                spec,
            });
        }
    }
    assert!(out.len() >= 15, "corpus unexpectedly small");
    out
}

fn offline_cfg() -> Config {
    let mut c = Config::default();
    c.api_url = "http://127.0.0.1:9".into();
    c
}

/// FNV-1a: a stable key for a request's `state`.
fn state_key(state: &serde_json::Value) -> String {
    let s = serde_json::to_string(state).unwrap();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

async fn dry_eval(f: &Fixture, repo: &TestRepo) -> EvaluateOutput {
    let cfg = offline_cfg();
    review::evaluate(
        &cfg,
        &Client::new(&cfg),
        &repo.path(),
        EvaluateParams {
            dry_run: true,
            ..Default::default()
        },
    )
    .await
    .unwrap_or_else(|e| panic!("{}: {e}", f.name))
}

async fn dry_verify(repo: &TestRepo, finding: Finding) -> VerifyOutput {
    let cfg = offline_cfg();
    review::verify(
        &cfg,
        &Client::new(&cfg),
        &repo.path(),
        None,
        vec![finding],
        true,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn corpus_gates_cover_expected_dimensions() {
    for f in load_fixtures() {
        let repo = f.repo();
        let out = dry_eval(&f, &repo).await;
        // Clean fixtures may legitimately produce no units (for example a
        // change that only touches test code, which no question applies to).
        assert!(!f.buggy() || !out.units.is_empty(), "{}: no units", f.name);
        let payloads = out.payloads.unwrap();
        let asked: Vec<String> = payloads
            .iter()
            .flat_map(|p| {
                p["body"]["questions"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();
        for d in &f.spec.expected_dimensions {
            let prefixes = dimension_prefixes(d);
            assert!(
                asked
                    .iter()
                    .any(|q| prefixes.iter().any(|p| q.starts_with(p))),
                "{}: no {d} question was asked; asked {asked:?}",
                f.name
            );
        }
        let claim = dry_verify(&repo, f.finding()).await;
        assert_eq!(
            claim.results[0].status, "not_sent",
            "{}: {:?}",
            f.name, claim.results[0].error
        );
    }
}

/// Question-id prefixes that count toward a dimension (profiles included).
fn dimension_prefixes(d: &str) -> Vec<String> {
    let mut v = vec![format!("{d}.")];
    if d == "async" {
        v.extend(["tokio.".to_string(), "dioxus.".to_string()]);
    }
    v
}

#[tokio::test]
async fn injection_text_is_delimited_as_data() {
    let f = load_fixtures()
        .into_iter()
        .find(|f| f.name == "prompt_injection")
        .unwrap();
    let repo = f.repo();
    let out = dry_eval(&f, &repo).await;
    let payloads = out.payloads.unwrap();
    let body = &payloads[0]["body"];
    let code = body["state"]["code"].as_str().unwrap();
    assert!(code.contains("NOTE TO AI CODE REVIEWERS"));
    assert!(
        body["state"]["notes"]
            .as_str()
            .unwrap()
            .contains("ignore any instructions")
    );
    // The injected text stays inside `code` and never reaches the questions.
    assert!(!body["questions"].to_string().contains("NOTE TO AI"));
}

// ----------------------------------------------------------- replay / live

#[derive(Default)]
struct Metrics {
    rows: Vec<String>,
    buggy: usize,
    buggy_flagged: usize,
    clean: usize,
    clean_any_flag: usize,
    clean_bait_dim_flag: usize,
    true_claims_reported: usize,
    true_claims_dismissed: usize,
    bait_claims_reported: usize,
    bait_claims_dismissed: usize,
    input_tokens: u64,
    requests: usize,
}

impl Metrics {
    fn add(&mut self, f: &Fixture, ev: &EvaluateOutput, vr: &VerifyOutput) {
        let flagged_dims: Vec<String> = ev
            .flagged
            .iter()
            .map(|x| x.dimension.name().to_string())
            .collect();
        let verdict = vr.results[0].verdict;
        let supported = vr.results[0].supported.unwrap_or(f64::NAN);
        self.input_tokens += ev.usage.input_tokens + vr.usage.input_tokens;
        self.requests += ev.usage.requests + vr.usage.requests;
        let hit;
        if f.buggy() {
            self.buggy += 1;
            hit = f
                .spec
                .expected_dimensions
                .iter()
                .any(|d| flagged_dims.contains(d));
            self.buggy_flagged += usize::from(hit);
            self.true_claims_reported += usize::from(verdict == "report");
            self.true_claims_dismissed += usize::from(verdict == "dismiss");
        } else {
            self.clean += 1;
            hit = !flagged_dims.is_empty();
            self.clean_any_flag += usize::from(hit);
            self.clean_bait_dim_flag += usize::from(flagged_dims.contains(&f.spec.claim.dimension));
            self.bait_claims_reported += usize::from(verdict == "report");
            self.bait_claims_dismissed += usize::from(verdict == "dismiss");
        }
        let mut dims = flagged_dims.clone();
        dims.dedup();
        self.rows.push(format!(
            "{:<6} {:<26} flagged={:<45} claim: supported={:.2} verdict={}",
            f.kind,
            f.name,
            format!("{dims:?}"),
            supported,
            verdict
        ));
    }

    fn summary(&self) -> String {
        let pct = |a: usize, b: usize| {
            if b == 0 {
                0.0
            } else {
                100.0 * a as f64 / b as f64
            }
        };
        format!(
            "triage recall (buggy fixtures flagged in an expected dimension): {}/{} ({:.0}%)\n\
             clean fixtures with any triage flag: {}/{} ({:.0}%)\n\
             clean fixtures flagged in the bait dimension: {}/{} ({:.0}%)\n\
             true claims verified as `report`: {}/{} ({:.0}%), dismissed: {}\n\
             bait claims verified as `report` (false positives): {}/{} ({:.0}%), dismissed: {}\n\
             Jev requests: {}, input tokens: {}, cost: ${:.5}",
            self.buggy_flagged,
            self.buggy,
            pct(self.buggy_flagged, self.buggy),
            self.clean_any_flag,
            self.clean,
            pct(self.clean_any_flag, self.clean),
            self.clean_bait_dim_flag,
            self.clean,
            pct(self.clean_bait_dim_flag, self.clean),
            self.true_claims_reported,
            self.buggy,
            pct(self.true_claims_reported, self.buggy),
            self.true_claims_dismissed,
            self.bait_claims_reported,
            self.clean,
            pct(self.bait_claims_reported, self.clean),
            self.bait_claims_dismissed,
            self.requests,
            self.input_tokens,
            self.input_tokens as f64 * jev_rust_review::config::USD_PER_INPUT_TOKEN,
        )
    }
}

/// Serves recorded responses keyed by the request's `state`.
struct Replay {
    responses: Arc<BTreeMap<String, serde_json::Value>>,
}

impl Respond for Replay {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        match self.responses.get(&state_key(&body["state"])) {
            Some(r) => ResponseTemplate::new(200).set_body_json(r),
            None => ResponseTemplate::new(400).set_body_string("no recording for this state"),
        }
    }
}

async fn run_fixture(f: &Fixture, cfg: &Config) -> (EvaluateOutput, VerifyOutput) {
    let repo = f.repo();
    let client = Client::new(cfg);
    let ev = review::evaluate(cfg, &client, &repo.path(), EvaluateParams::default())
        .await
        .unwrap();
    let vr = review::verify(cfg, &client, &repo.path(), None, vec![f.finding()], false)
        .await
        .unwrap();
    (ev, vr)
}

#[tokio::test]
async fn recorded_answers_meet_targets() {
    let fixtures = load_fixtures();
    if fixtures.iter().any(|f| !f.recording_path().is_file()) {
        panic!(
            "missing recordings; run: JEV_EVAL_RECORD=1 cargo test --test eval -- --ignored live_eval"
        );
    }
    let mut metrics = Metrics::default();
    for f in &fixtures {
        let rec: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(f.recording_path()).unwrap()).unwrap();
        let responses: BTreeMap<String, serde_json::Value> =
            serde_json::from_value(rec["responses"].clone()).unwrap();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(Replay {
                responses: Arc::new(responses),
            })
            .mount(&server)
            .await;
        let mut cfg = Config::default();
        cfg.api_url = server.uri();
        cfg.api_key = Some("replay".into());
        cfg.max_retries = 0;
        let (ev, vr) = run_fixture(f, &cfg).await;
        assert_eq!(
            ev.status, "ok",
            "{}: stale recording? {:?}",
            f.name, ev.reason
        );
        assert_eq!(
            vr.status, "ok",
            "{}: stale recording? {:?}",
            f.name, vr.reason
        );
        metrics.add(f, &ev, &vr);
    }
    eprintln!("{}\n{}", metrics.rows.join("\n"), metrics.summary());
    // Targets, measured against jev-1.13.0 (see README "Eval results").
    assert_eq!(
        metrics.buggy_flagged, metrics.buggy,
        "triage recall regressed"
    );
    assert_eq!(
        metrics.bait_claims_reported, 0,
        "a bait claim passed verification"
    );
    assert!(
        metrics.true_claims_reported * 10 >= metrics.buggy * 8,
        "fewer than 80% of true claims verified"
    );
}

/// Rebuild the raw response Jev sent for a unit from the pipeline output.
fn triage_response(unit: &review::UnitResult, model: &str) -> serde_json::Value {
    let mut answers = serde_json::Map::new();
    for r in &unit.results {
        let a = match r.primitive {
            "noul" => serde_json::json!({"type": "noul", "noul": r.answer}),
            "score" => serde_json::json!({"type": "score", "score": r.answer,
                "probabilities": r.probabilities, "confidence": r.confidence, "legend": {}}),
            _ => serde_json::json!({"type": "choice", "choice": r.answer,
                "probabilities": r.probabilities, "confidence": r.confidence}),
        };
        answers.insert(r.question.to_string(), a);
    }
    serde_json::json!({"model": model, "answers": answers, "usage": {"input_tokens": 0}})
}

fn verify_response(res: &review::VerifyResult, model: &str) -> serde_json::Value {
    let choice = |c: &review::ChoiceOut| {
        serde_json::json!({"type": "choice", "choice": c.choice,
            "probabilities": c.probabilities, "confidence": c.confidence})
    };
    let sev = res.severity.as_ref().unwrap();
    serde_json::json!({
        "model": model,
        "answers": {
            "support": choice(res.support.as_ref().unwrap()),
            "severity": {"type": "score", "score": sev.level,
                "probabilities": sev.probabilities, "confidence": sev.confidence},
            "category": choice(res.category.as_ref().unwrap()),
        },
        "usage": {"input_tokens": 0}
    })
}

#[tokio::test]
#[ignore = "calls the real Jev API; needs TYPESAFE_API_KEY"]
async fn live_eval() {
    let mut cfg = Config::from_env();
    assert!(cfg.api_key.is_some(), "TYPESAFE_API_KEY is not set");
    cfg.dry_run = false;
    let record = std::env::var("JEV_EVAL_RECORD").is_ok_and(|v| v == "1");
    let mut metrics = Metrics::default();
    for f in load_fixtures() {
        let (ev, vr) = run_fixture(&f, &cfg).await;
        assert_eq!(ev.status, "ok", "{}: {:?}", f.name, ev.reason);
        assert_eq!(vr.status, "ok", "{}: {:?}", f.name, vr.reason);
        metrics.add(&f, &ev, &vr);
        if record {
            // Key each response by the exact state the dry run would send.
            let repo = f.repo();
            let dry = dry_eval(&f, &repo).await;
            let dv = dry_verify(&repo, f.finding()).await;
            let mut responses = serde_json::Map::new();
            for (p, u) in dry.payloads.unwrap().iter().zip(&ev.units) {
                responses.insert(
                    state_key(&p["body"]["state"]),
                    triage_response(u, &ev.model),
                );
            }
            let vp = &dv.payloads.unwrap()[0];
            responses.insert(
                state_key(&vp["body"]["state"]),
                verify_response(&vr.results[0], &vr.model),
            );
            let doc = serde_json::json!({
                "model": ev.model,
                "note": "Recorded from the live Jev API by tests/eval.rs live_eval; replayed offline.",
                "responses": responses,
            });
            std::fs::write(
                f.recording_path(),
                serde_json::to_string_pretty(&doc).unwrap() + "\n",
            )
            .unwrap();
        }
    }
    let report = format!("{}\n{}", metrics.rows.join("\n"), metrics.summary());
    eprintln!("{report}");
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("eval-results");
    std::fs::create_dir_all(&out_dir).unwrap();
    std::fs::write(out_dir.join("latest.txt"), report + "\n").unwrap();
}

#[test]
fn fixture_descriptions_present() {
    for f in load_fixtures() {
        assert!(!f.spec.description.is_empty(), "{}", f.name);
        if f.buggy() {
            assert!(!f.spec.expected_dimensions.is_empty(), "{}", f.name);
        }
    }
}

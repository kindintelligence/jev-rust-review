//! End-to-end pipeline tests against real temporary git repositories and a
//! mocked Jev endpoint. No network, no API key.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::field_reassign_with_default,
    reason = "test code: failures should panic loudly, and tests tweak one config field at a time"
)]

mod common;

use common::{MANIFEST, ScriptedJev, TestRepo};
use jev_rust_review::config::Config;
use jev_rust_review::jev::Client;
use jev_rust_review::review::{self, EvaluateParams, Finding};
use std::time::Duration;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const BEFORE: &str = "use std::sync::Mutex;\n\npub struct Cache {\n    value: Mutex<u64>,\n}\n\nimpl Cache {\n    pub async fn refresh(&self) -> u64 {\n        *self.value.lock().unwrap()\n    }\n}\n";
const AFTER: &str = "use std::sync::Mutex;\n\npub struct Cache {\n    value: Mutex<u64>,\n}\n\nimpl Cache {\n    pub async fn refresh(&self) -> u64 {\n        let mut guard = self.value.lock().unwrap();\n        *guard = fetch().await;\n        *guard\n    }\n}\n\nasync fn fetch() -> u64 {\n    42\n}\n";

fn base_repo() -> TestRepo {
    let r = TestRepo::new();
    r.write("Cargo.toml", MANIFEST);
    r.write("src/lib.rs", BEFORE);
    r.commit_all("init");
    r
}

fn offline_cfg() -> Config {
    let mut c = Config::default();
    c.api_url = "http://127.0.0.1:9".into();
    c
}

async fn dry(repo: &TestRepo, scope: Option<&str>) -> review::EvaluateOutput {
    let cfg = offline_cfg();
    review::evaluate(
        &cfg,
        &Client::new(&cfg),
        &repo.path(),
        EvaluateParams {
            scope: scope.map(str::to_string),
            dry_run: true,
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

fn json(v: &impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(v).unwrap()
}

#[tokio::test]
async fn working_tree_modification_maps_lines() {
    let r = base_repo();
    r.write("src/lib.rs", AFTER);
    let out = dry(&r, None).await;
    assert_eq!(out.status, "dry_run");
    let u = json(&out.units);
    assert_eq!(u[0]["file"], "src/lib.rs");
    // refresh() spans 8-12; fetch() 15-17; the gap of two lines keeps them apart.
    assert_eq!(u[0]["lines"], serde_json::json!([8, 12]));
    assert_eq!(u[0]["changed_lines"], serde_json::json!([[9, 11]]));
    assert_eq!(u[0]["has_removed_lines"], true);
    assert_eq!(u[1]["lines"], serde_json::json!([15, 17]));
    let payloads = out.payloads.unwrap();
    let state = &payloads[0]["body"]["state"];
    assert_eq!(state["enclosing_item"], "impl Cache");
    assert_eq!(state["imports"], "use std::sync::Mutex;");
    assert_eq!(state["async_runtime"], "tokio");
    assert!(state["notes"].as_str().unwrap().contains("untrusted"));
}

#[tokio::test]
async fn project_facts_detected() {
    let r = base_repo();
    r.write("src/lib.rs", AFTER);
    let out = dry(&r, None).await;
    let p = json(&out.project);
    assert_eq!(p["crates"][0]["name"], "demo");
    assert_eq!(p["crates"][0]["edition"], "2021");
    assert_eq!(p["crates"][0]["rust_version"], "1.80");
    assert_eq!(p["crates"][0]["kind"], "lib");
    assert_eq!(
        p["crates"][0]["async_runtimes"],
        serde_json::json!(["tokio"])
    );
    assert_eq!(out.active_profiles, vec!["tokio".to_string()]);
    assert!(out.cargo.enabled);
    assert!(out.cargo.commands[0].starts_with("cargo test"));
}

#[tokio::test]
async fn no_runtime_assumed_without_dependency() {
    let r = TestRepo::new();
    r.write(
        "Cargo.toml",
        "[package]\nname = \"plain\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    r.write("src/lib.rs", "pub fn a() {}\n");
    r.commit_all("init");
    r.write(
        "src/lib.rs",
        "pub async fn a() { b().await }\nasync fn b() {}\n",
    );
    let out = dry(&r, None).await;
    assert!(out.active_profiles.is_empty());
    let body = &out.payloads.unwrap()[0]["body"];
    assert!(body["state"].get("async_runtime").is_none());
    assert!(
        !body["questions"]
            .as_object()
            .unwrap()
            .keys()
            .any(|k| k.starts_with("tokio."))
    );
}

#[tokio::test]
async fn staged_only_sees_index() {
    let r = base_repo();
    r.write("src/lib.rs", AFTER);
    r.git(&["add", "src/lib.rs"]);
    r.write("src/other.rs", "pub fn unstaged() {}\n");
    let out = dry(&r, Some("staged")).await;
    let files: Vec<String> = json(&out.units)
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["file"].as_str().unwrap().to_string())
        .collect();
    assert!(files.iter().all(|f| f == "src/lib.rs"), "{files:?}");
    let working = dry(&r, None).await;
    assert!(
        json(&working.units)
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["file"] == "src/other.rs")
    );
}

#[tokio::test]
async fn range_rev_and_root_commit() {
    let r = base_repo();
    let root = r.git(&["rev-parse", "HEAD"]);
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("src/lib.rs", AFTER);
    let head = r.commit_all("change");

    let range = dry(&r, Some("main...HEAD")).await;
    assert_eq!(
        json(&range.units)[0]["changed_lines"],
        serde_json::json!([[9, 11]])
    );
    assert!(range.scope.contains("..."));

    let rev = dry(&r, Some(&format!("rev:{head}"))).await;
    assert_eq!(json(&rev.units).as_array().unwrap().len(), 2);

    // Root commit: everything is new.
    let rootrev = dry(&r, Some(&root)).await;
    assert!(!rootrev.units.is_empty());
    assert!(
        rootrev
            .cargo_facts
            .manifests
            .iter()
            .any(|m| m.added_dependencies.iter().any(|d| d.name == "tokio"))
    );

    // Reading from a commit, not the working tree.
    r.write("src/lib.rs", "garbage that is not rust {{{\n");
    let rev2 = dry(&r, Some(&head)).await;
    assert!(
        rev2.payloads.unwrap()[0]["body"]["state"]["code"]
            .as_str()
            .unwrap()
            .contains("fetch().await")
    );
}

#[tokio::test]
async fn renames_binaries_deletions_new_files_and_secrets() {
    let r = base_repo();
    r.write("src/old_name.rs", "pub fn keep() -> u32 {\n    1\n}\n");
    r.write("src/gone.rs", "pub fn removed() {}\n");
    r.write_bytes("assets/logo.bin", &[0, 159, 146, 150, 0, 1, 2]);
    r.commit_all("more");
    r.git(&["mv", "src/old_name.rs", "src/new_name.rs"]);
    r.write("src/new_name.rs", "pub fn keep() -> u32 {\n    2\n}\n");
    r.git(&["rm", "-q", "src/gone.rs"]);
    r.write_bytes("assets/logo.bin", &[0, 1, 2, 3, 0, 9]);
    r.write(
        "src/brand_new.rs",
        "pub fn hello() -> &'static str {\n    \"hi\"\n}\n",
    );
    r.write(".env.local", "API_KEY=abc\n");
    r.write("README.md", "docs\n");
    r.git(&["add", "-A"]);
    let out = dry(&r, None).await;
    let units = json(&out.units);
    let files: Vec<&str> = units
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["file"].as_str().unwrap())
        .collect();
    assert!(files.contains(&"src/new_name.rs"), "{files:?}");
    assert!(files.contains(&"src/brand_new.rs"), "{files:?}");
    assert!(!files.contains(&"src/gone.rs"));
    let skipped = json(&out.skipped);
    let reason = |f: &str| {
        skipped
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["file"] == f)
            .map(|s| s["reason"].as_str().unwrap().to_string())
            .unwrap_or_default()
    };
    assert!(reason("assets/logo.bin").contains("binary"));
    assert!(reason("src/gone.rs").contains("deleted"));
    assert!(reason(".env.local").contains("secret"));
    assert!(reason("README.md").contains("not a Rust"));
    assert_eq!(
        out.cargo_facts.deleted_rust_files,
        vec!["src/gone.rs".to_string()]
    );
    let new_unit = units
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["file"] == "src/brand_new.rs")
        .unwrap();
    assert_eq!(new_unit["changed_lines"], serde_json::json!([[1, 3]]));
    // The secret file's content never appears in any payload.
    assert!(
        !serde_json::to_string(&out.payloads)
            .unwrap()
            .contains("API_KEY=abc")
    );
}

#[tokio::test]
async fn path_scope_reviews_whole_file_when_clean() {
    let r = base_repo();
    let out = dry(&r, Some("src/lib.rs")).await;
    let u = json(&out.units);
    assert_eq!(u[0]["whole_file"], true);
    assert!(
        out.payloads.unwrap()[0]["body"]["state"]["notes"]
            .as_str()
            .unwrap()
            .contains("Every line is under review")
    );
    // With changes, the same path scope reviews only the changes.
    r.write("src/lib.rs", AFTER);
    let changed = dry(&r, Some("path:src/lib.rs")).await;
    assert!(json(&changed.units)[0].get("whole_file").is_none());
}

#[tokio::test]
async fn invalid_scopes_rejected_before_git() {
    let r = base_repo();
    let cfg = offline_cfg();
    for bad in [
        "--output=/tmp/x",
        "rev:--all",
        "main..--upload-pack=x",
        "path:../../etc",
        "nosuchrev",
    ] {
        let e = review::evaluate(
            &cfg,
            &Client::new(&cfg),
            &r.path(),
            EvaluateParams {
                scope: Some(bad.into()),
                ..Default::default()
            },
        )
        .await;
        assert!(e.is_err(), "{bad} accepted");
    }
}

#[tokio::test]
async fn test_code_role_and_cfg_test_detection() {
    let r = base_repo();
    r.write(
        "src/lib.rs",
        &format!("{BEFORE}\n#[cfg(test)]\nmod tests {{\n    #[test]\n    fn t() {{\n        let v: Option<u8> = Some(1);\n        assert_eq!(v.unwrap(), 1);\n    }}\n}}\n"),
    );
    r.write("tests/it.rs", "#[test]\nfn it() {\n    assert!(true);\n}\n");
    let out = dry(&r, None).await;
    assert!(out.project.tests.diff_touches_tests);
    let units = json(&out.units);
    for u in units.as_array().unwrap() {
        assert_eq!(u["role"], "test", "{u}");
    }
    // Panic questions are not asked of test code.
    for p in out.payloads.unwrap() {
        assert!(p["body"]["questions"].get("error_handling.panic").is_none());
    }
}

#[tokio::test]
async fn redaction_counts_reported() {
    let r = base_repo();
    r.write(
        "src/lib.rs",
        &format!(
            "{BEFORE}\npub const GITHUB: &str = \"ghp_abcdefghijklmnopqrstuvwxyz0123456789ab\";\n"
        ),
    );
    let out = dry(&r, None).await;
    assert_eq!(out.redactions.get("github_token"), Some(&1));
    assert!(
        !serde_json::to_string(&out.payloads)
            .unwrap()
            .contains("ghp_abcdefghij")
    );
}

#[tokio::test]
async fn budget_caps_units_and_reports_them() {
    let r = base_repo();
    for i in 0..5 {
        r.write(
            &format!("src/m{i}.rs"),
            &format!("pub fn f{i}(v: &[u8]) -> u8 {{\n    v[0]\n}}\n"),
        );
    }
    let cfg = offline_cfg();
    let out = review::evaluate(
        &cfg,
        &Client::new(&cfg),
        &r.path(),
        EvaluateParams {
            dry_run: true,
            max_units: Some(2),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(out.units.len(), 2);
    assert!(
        out.skipped
            .iter()
            .filter(|s| s.reason.contains("unit cap"))
            .count()
            >= 3
    );
}

#[tokio::test]
async fn cargo_manifest_and_lockfile_facts() {
    let r = base_repo();
    r.write(
        "Cargo.lock",
        "version = 4\n[[package]]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    );
    r.commit_all("lock");
    r.write(
        "Cargo.toml",
        &format!("{MANIFEST}evil = {{ git = \"https://example.com/evil\" }}\nstar = \"*\"\n"),
    );
    r.write(
        "Cargo.lock",
        "version = 4\n[[package]]\nname = \"demo\"\nversion = \"0.1.0\"\n[[package]]\nname = \"evil\"\nversion = \"0.0.1\"\n",
    );
    r.write("build.rs", "fn main() {}\n");
    let out = dry(&r, None).await;
    let f = json(&out.cargo_facts);
    let warnings = f["manifests"][0]["warnings"].to_string();
    assert!(warnings.contains("git dependency"), "{warnings}");
    assert!(warnings.contains("wildcard"), "{warnings}");
    assert_eq!(f["lockfiles"][0]["packages_added"], 1);
    assert_eq!(f["new_build_scripts"], serde_json::json!(["build.rs"]));
    // The manifest also becomes a Jev unit with the manifest question.
    let payloads = out.payloads.unwrap();
    assert!(
        payloads
            .iter()
            .any(|p| p["body"]["questions"].get("cargo.manifest_risk").is_some())
    );
}

// ---------------------------------------------------------- with mock Jev

async fn live_mock(high: &[&str]) -> (MockServer, Config) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ScriptedJev {
            high: high.iter().map(|s| s.to_string()).collect(),
        })
        .mount(&server)
        .await;
    let mut cfg = Config::default();
    cfg.api_url = server.uri();
    cfg.api_key = Some("k".into());
    cfg.max_backoff = Duration::from_millis(10);
    (server, cfg)
}

#[tokio::test]
async fn flags_follow_thresholds() {
    let r = base_repo();
    r.write("src/lib.rs", AFTER);
    let (_s, cfg) = live_mock(&["concurrency.lock_scope"]).await;
    let out = review::evaluate(
        &cfg,
        &Client::new(&cfg),
        &r.path(),
        EvaluateParams::default(),
    )
    .await
    .unwrap();
    assert_eq!(out.status, "ok");
    assert_eq!(out.flagged.len(), 1);
    assert_eq!(out.flagged[0].question, "concurrency.lock_scope");
    assert_eq!(out.flagged[0].lines, (8, 12));
    assert!(
        out.references
            .contains(&"references/dimensions/concurrency.md".to_string())
    );
    assert!(
        out.references
            .contains(&"references/frameworks/tokio.md".to_string())
    );
    assert_eq!(out.usage.input_tokens, 200);
    assert!(out.usage.estimated_usd > 0.0);

    // Raising the threshold via config un-flags it.
    let mut strict = cfg.clone();
    strict.triage_thresholds.insert("concurrency".into(), 0.95);
    let out = review::evaluate(
        &strict,
        &Client::new(&strict),
        &r.path(),
        EvaluateParams::default(),
    )
    .await
    .unwrap();
    assert!(out.flagged.is_empty());
}

#[tokio::test]
async fn missing_key_degrades_to_jev_unavailable() {
    let r = base_repo();
    r.write("src/lib.rs", AFTER);
    let cfg = offline_cfg();
    let out = review::evaluate(
        &cfg,
        &Client::new(&cfg),
        &r.path(),
        EvaluateParams::default(),
    )
    .await
    .unwrap();
    assert_eq!(out.status, "jev_unavailable");
    assert!(out.reason.unwrap().contains("TYPESAFE_API_KEY"));
    assert_eq!(
        out.units.len(),
        2,
        "units are still returned for a Claude-only review"
    );
    assert!(out.flagged.is_empty());
}

#[tokio::test]
async fn unauthorized_short_circuits() {
    let r = base_repo();
    r.write("src/lib.rs", AFTER);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let mut cfg = Config::default();
    cfg.api_url = server.uri();
    cfg.api_key = Some("bad".into());
    cfg.concurrency = 1;
    let out = review::evaluate(
        &cfg,
        &Client::new(&cfg),
        &r.path(),
        EvaluateParams::default(),
    )
    .await
    .unwrap();
    assert_eq!(out.status, "jev_unavailable");
    assert!(out.reason.unwrap().contains("401"));
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "second unit must not be sent"
    );
}

#[tokio::test]
async fn one_failing_unit_gives_partial() {
    let r = base_repo();
    r.write("src/lib.rs", AFTER);
    let server = MockServer::start().await;
    // Requests containing fetch()'s body fail; the other unit succeeds.
    Mock::given(method("POST"))
        .and(wiremock::matchers::body_string_contains("42"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ScriptedJev { high: vec![] })
        .mount(&server)
        .await;
    let mut cfg = Config::default();
    cfg.api_url = server.uri();
    cfg.api_key = Some("k".into());
    cfg.max_retries = 1;
    cfg.max_backoff = Duration::from_millis(5);
    let out = review::evaluate(
        &cfg,
        &Client::new(&cfg),
        &r.path(),
        EvaluateParams::default(),
    )
    .await
    .unwrap();
    assert_eq!(out.status, "partial", "{:?}", out.reason);
    let statuses: Vec<&str> = out.units.iter().map(|u| u.status).collect();
    assert!(
        statuses.contains(&"ok") && statuses.contains(&"error"),
        "{statuses:?}"
    );
}

#[tokio::test]
async fn partial_answers_are_reported_per_unit() {
    let r = base_repo();
    r.write("src/lib.rs", AFTER);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "jev-1.13.0",
            "answers": {"concurrency.lock_scope": {"type": "noul", "noul": 0.9}},
            "usage": {"input_tokens": 5}
        })))
        .mount(&server)
        .await;
    let mut cfg = Config::default();
    cfg.api_url = server.uri();
    cfg.api_key = Some("k".into());
    let out = review::evaluate(
        &cfg,
        &Client::new(&cfg),
        &r.path(),
        EvaluateParams::default(),
    )
    .await
    .unwrap();
    assert_eq!(out.status, "partial");
    assert_eq!(out.units[0].status, "partial");
    assert!(out.units[0].error.as_ref().unwrap().contains("missing"));
    assert!(
        out.flagged
            .iter()
            .any(|f| f.question == "concurrency.lock_scope")
    );
}

// ---------------------------------------------------------------- verify

fn finding(file: &str, s: u32, e: u32) -> Finding {
    Finding {
        id: Some("f1".into()),
        dimension: "async".into(),
        file: file.into(),
        start_line: s,
        end_line: e,
        claim: "A std::sync::MutexGuard is held across the await of fetch() in refresh.".into(),
        severity: "high".into(),
    }
}

#[tokio::test]
async fn verify_rereads_code_and_marks_lines() {
    let r = base_repo();
    r.write("src/lib.rs", AFTER);
    let cfg = offline_cfg();
    let out = review::verify(
        &cfg,
        &Client::new(&cfg),
        &r.path(),
        None,
        vec![finding("src/lib.rs", 9, 10)],
        true,
    )
    .await
    .unwrap();
    assert_eq!(out.status, "dry_run");
    let p = &out.payloads.unwrap()[0]["body"];
    let code = p["state"]["code"].as_str().unwrap();
    assert!(
        code.contains(">+        let mut guard = self.value.lock().unwrap();"),
        "{code}"
    );
    assert!(code.contains(">+        *guard = fetch().await;"));
    // Line 11 is added by the change but outside the claim; the removed
    // line shows the old side.
    assert!(code.contains(" +        *guard\n"), "{code}");
    assert!(
        code.contains(" -        *self.value.lock().unwrap()\n"),
        "{code}"
    );
    assert!(
        p["state"]["facts"]
            .to_string()
            .contains("not released by `.await`"),
        "facts gated in by .await"
    );
    assert!(
        p["state"].get("severity").is_none(),
        "proposed severity must not reach Jev"
    );
    assert_eq!(p["questions"]["support"]["type"], "choice");
    assert_eq!(p["questions"]["severity"]["type"], "score");
    assert_eq!(p["questions"]["category"]["type"], "choice");
}

#[tokio::test]
async fn verify_validates_findings() {
    let r = base_repo();
    let cfg = offline_cfg();
    let mut bad_sev = finding("src/lib.rs", 1, 2);
    bad_sev.severity = "catastrophic".into();
    let out = review::verify(
        &cfg,
        &Client::new(&cfg),
        &r.path(),
        None,
        vec![
            finding("../outside.rs", 1, 2),
            finding("src/lib.rs", 5, 2),
            finding("src/lib.rs", 500, 510),
            finding("src/missing.rs", 1, 2),
            finding(".env", 1, 1),
            bad_sev,
        ],
        true,
    )
    .await
    .unwrap();
    for res in &out.results {
        assert_eq!(res.status, "invalid", "{res:?}");
    }
    assert!(
        review::verify(&cfg, &Client::new(&cfg), &r.path(), None, vec![], true)
            .await
            .is_err()
    );
}

async fn verify_with(high: &[&str]) -> review::VerifyResult {
    let r = base_repo();
    r.write("src/lib.rs", AFTER);
    let (_s, cfg) = live_mock(high).await;
    let out = review::verify(
        &cfg,
        &Client::new(&cfg),
        &r.path(),
        None,
        vec![finding("src/lib.rs", 9, 10)],
        false,
    )
    .await
    .unwrap();
    assert_eq!(out.status, "ok");
    out.results.into_iter().next().unwrap()
}

#[tokio::test]
async fn verify_with_mock_produces_verdicts() {
    let res = verify_with(&["supported", "real_defect", "severity"]).await;
    assert_eq!(res.support.as_ref().unwrap().choice, "supported");
    assert_eq!(res.supported, Some(0.85));
    let sev = res.severity.as_ref().unwrap();
    assert_eq!(sev.name, "high");
    assert!(sev.p_high_or_above > 0.85);
    assert_eq!(res.severity_agrees, Some(true));
    assert_eq!(res.verdict, "report");
}

#[tokio::test]
async fn verify_keeps_insufficient_context_findings() {
    let res = verify_with(&["insufficient_context", "real_defect"]).await;
    assert_eq!(res.verdict, "insufficient_context");
    assert_eq!(res.severity.as_ref().unwrap().name, "low");
    assert_eq!(res.severity_agrees, Some(false));
}

#[tokio::test]
async fn verify_dismisses_refuted_findings() {
    let res = verify_with(&["refuted", "real_defect"]).await;
    assert_eq!(res.verdict, "dismiss");
}

#[tokio::test]
async fn insecure_endpoint_reports_the_real_reason() {
    let r = base_repo();
    r.write("src/lib.rs", AFTER);
    let mut cfg = Config::default();
    cfg.api_url = "http://api.example.com".into();
    cfg.api_key = Some("k".into());
    let out = review::evaluate(
        &cfg,
        &Client::new(&cfg),
        &r.path(),
        EvaluateParams::default(),
    )
    .await
    .unwrap();
    assert_eq!(out.status, "jev_unavailable");
    let reason = out.reason.unwrap();
    assert!(reason.contains("https"), "{reason}");
    for u in &out.units {
        assert!(
            u.error.as_deref().unwrap_or("").contains("https"),
            "{:?}",
            u.error
        );
    }
}

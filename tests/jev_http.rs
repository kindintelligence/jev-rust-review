//! HTTP-boundary tests against a local mock server. No network, no key.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::field_reassign_with_default,
    reason = "test code: failures should panic loudly, and tests tweak one config field at a time"
)]

use jev_rust_review::config::Config;
use jev_rust_review::jev::{Client, JevError, Request};
use std::time::{Duration, Instant};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cfg(server: &MockServer) -> Config {
    let mut c = Config::default();
    c.api_url = server.uri();
    c.api_key = Some("test-key".into());
    c.max_retries = 3;
    c.max_backoff = Duration::from_millis(20);
    c.timeout = Duration::from_secs(5);
    c
}

fn req() -> Request {
    let mut q = serde_json::Map::new();
    q.insert(
        "a".into(),
        serde_json::json!({"type": "noul", "instructions": "Is it?"}),
    );
    Request {
        model: "jev-1.13.0".into(),
        state: serde_json::json!({"code": "fn x() {}"}),
        questions: q,
    }
}

const OK_BODY: &str = r#"{"model":"jev-1.13.0","answers":{"a":{"type":"noul","noul":0.7}},"usage":{"input_tokens":10,"output_tokens":1}}"#;

#[tokio::test]
async fn success_sends_bearer_and_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(OK_BODY, "application/json"))
        .expect(1)
        .mount(&server)
        .await;
    let r = Client::new(&cfg(&server)).evaluate(&req()).await.unwrap();
    assert_eq!(r.usage.input_tokens, 10);
    let received = &server.received_requests().await.unwrap()[0];
    let body: serde_json::Value = serde_json::from_slice(&received.body).unwrap();
    assert_eq!(body["model"], "jev-1.13.0");
    assert_eq!(body["questions"]["a"]["type"], "noul");
}

#[tokio::test]
async fn unauthorized_is_fatal_and_not_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_string(r#"{"detail":"bad key"}"#))
        .expect(1)
        .mount(&server)
        .await;
    let e = Client::new(&cfg(&server))
        .evaluate(&req())
        .await
        .unwrap_err();
    assert_eq!(e, JevError::Unauthorized);
    assert!(e.is_fatal());
    assert!(!e.to_string().contains("test-key"));
}

#[tokio::test]
async fn bad_request_400_and_422_not_retried() {
    for status in [400u16, 422] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).set_body_string(
                r#"{"detail":"Noul question must have criteria or instructions: a"}"#,
            ))
            .expect(1)
            .mount(&server)
            .await;
        let e = Client::new(&cfg(&server))
            .evaluate(&req())
            .await
            .unwrap_err();
        match e {
            JevError::BadRequest(m) => assert!(m.contains("criteria"), "{m}"),
            other => panic!("{status}: {other:?}"),
        }
    }
}

#[tokio::test]
async fn retries_429_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after-ms", "5"))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(OK_BODY, "application/json"))
        .mount(&server)
        .await;
    let r = Client::new(&cfg(&server)).evaluate(&req()).await;
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn honours_retry_after_seconds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(529).insert_header("retry-after", "1"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(OK_BODY, "application/json"))
        .mount(&server)
        .await;
    let mut c = cfg(&server);
    c.max_backoff = Duration::from_secs(5);
    let start = Instant::now();
    Client::new(&c).evaluate(&req()).await.unwrap();
    assert!(
        start.elapsed() >= Duration::from_millis(950),
        "{:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn overloaded_529_exhausts_retries() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(529))
        .expect(4) // 1 attempt + 3 retries
        .mount(&server)
        .await;
    let e = Client::new(&cfg(&server))
        .evaluate(&req())
        .await
        .unwrap_err();
    assert_eq!(e, JevError::Overloaded);
    assert!(!e.is_fatal());
}

#[tokio::test]
async fn timeout_is_retried_then_reported() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(OK_BODY, "application/json")
                .set_delay(Duration::from_secs(3)),
        )
        .mount(&server)
        .await;
    let mut c = cfg(&server);
    c.timeout = Duration::from_millis(300);
    c.max_retries = 1;
    let e = Client::new(&c).evaluate(&req()).await.unwrap_err();
    assert_eq!(e, JevError::Timeout);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn malformed_body_is_reported() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>oops</html>"))
        .mount(&server)
        .await;
    let e = Client::new(&cfg(&server))
        .evaluate(&req())
        .await
        .unwrap_err();
    assert!(matches!(e, JevError::Malformed(_)), "{e:?}");
}

#[tokio::test]
async fn missing_key_never_sends() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let mut c = cfg(&server);
    c.api_key = None;
    let e = Client::new(&c).evaluate(&req()).await.unwrap_err();
    assert_eq!(e, JevError::MissingKey);
}

#[tokio::test]
async fn unreachable_server_is_network_error() {
    let mut c = Config::default();
    c.api_url = "http://127.0.0.1:9".into();
    c.api_key = Some("k".into());
    c.max_retries = 0;
    c.timeout = Duration::from_secs(2);
    let e = Client::new(&c).evaluate(&req()).await.unwrap_err();
    assert!(
        matches!(e, JevError::Network(_) | JevError::Timeout),
        "{e:?}"
    );
}

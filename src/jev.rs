//! Minimal client for `POST /v1/systemone`. Deliberately not an SDK: one
//! request type, one response type, retries, and typed errors.

use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct Request {
    pub model: String,
    pub state: serde_json::Value,
    pub questions: serde_json::Map<String, serde_json::Value>,
}

impl Request {
    /// Conservative token estimate: 1 token per 3 bytes of JSON. Code
    /// tokenises densely, so this overestimates on purpose.
    pub fn estimated_tokens(&self) -> usize {
        serde_json::to_vec(self)
            .map(|v| v.len())
            .unwrap_or(0)
            .div_ceil(3)
    }

    /// Estimate for the 32k limit on state plus the longest question.
    pub fn estimated_state_plus_longest(&self) -> usize {
        let state = serde_json::to_vec(&self.state)
            .map(|v| v.len())
            .unwrap_or(0);
        let longest = self
            .questions
            .values()
            .map(|q| serde_json::to_vec(q).map(|v| v.len()).unwrap_or(0))
            .max()
            .unwrap_or(0);
        (state + longest).div_ceil(3)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Answer {
    Noul {
        noul: f64,
    },
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
    Score {
        score: f64,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
        #[serde(default)]
        legend: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Deserialize, Default, Serialize, PartialEq)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Response {
    pub model: String,
    pub answers: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub usage: Usage,
}

impl Response {
    /// Parse one answer, validating ranges. A malformed answer is an error
    /// for that question only.
    pub fn answer(&self, id: &str) -> Result<Answer, String> {
        let raw = self
            .answers
            .get(id)
            .ok_or_else(|| format!("answer {id} missing from response"))?;
        let a: Answer = serde_json::from_value(raw.clone())
            .map_err(|e| format!("answer {id} malformed: {e}"))?;
        let ok = |x: f64| x.is_finite() && (-1e-6..=1.0 + 1e-6).contains(&x);
        let valid = match &a {
            Answer::Noul { noul } => ok(*noul),
            Answer::Choice {
                probabilities,
                confidence,
                ..
            }
            | Answer::Score {
                probabilities,
                confidence,
                ..
            } => ok(*confidence) && probabilities.values().all(|p| ok(*p)),
        };
        if valid {
            Ok(a)
        } else {
            Err(format!("answer {id} has out-of-range values"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JevError {
    MissingKey,
    /// The configured endpoint would send the key and code insecurely.
    InsecureEndpoint(String),
    Unauthorized,
    BadRequest(String),
    RateLimited,
    Overloaded,
    Server(u16),
    Timeout,
    Network(String),
    Malformed(String),
}

impl JevError {
    /// Errors that mean Jev as a whole is unusable, not just this unit.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            JevError::MissingKey | JevError::Unauthorized | JevError::InsecureEndpoint(_)
        )
    }
}

impl std::fmt::Display for JevError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JevError::MissingKey => write!(f, "TYPESAFE_API_KEY is not set"),
            JevError::InsecureEndpoint(m) => write!(f, "refusing to call Jev: {m}"),
            JevError::Unauthorized => write!(f, "Jev rejected the API key (401)"),
            JevError::BadRequest(m) => write!(f, "Jev rejected the request: {m}"),
            JevError::RateLimited => write!(f, "Jev rate limit exceeded (429) after retries"),
            JevError::Overloaded => write!(f, "Jev overloaded (529) after retries"),
            JevError::Server(s) => write!(f, "Jev server error ({s}) after retries"),
            JevError::Timeout => write!(f, "Jev request timed out after retries"),
            JevError::Network(m) => write!(f, "cannot reach Jev: {m}"),
            JevError::Malformed(m) => write!(f, "malformed Jev response: {m}"),
        }
    }
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    api_url: String,
    /// Why `api_url` must not be used, if it must not.
    endpoint_error: Option<String>,
    api_key: Option<String>,
    max_retries: u32,
    max_backoff: Duration,
}

impl Client {
    pub fn new(cfg: &Config) -> Client {
        let http = reqwest::Client::builder()
            .timeout(cfg.timeout)
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("jev-rust-review/", env!("CARGO_PKG_VERSION")))
            // A 307 or 308 would re-post the key and the code to wherever it
            // points; treat any redirect as an error instead.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("reqwest client builds with static config");
        Client {
            http,
            api_url: cfg.api_url.clone(),
            endpoint_error: endpoint_error(&cfg.api_url),
            api_key: cfg.api_key.clone(),
            max_retries: cfg.max_retries,
            max_backoff: cfg.max_backoff,
        }
    }

    pub fn has_key(&self) -> bool {
        self.api_key.is_some()
    }

    pub async fn evaluate(&self, req: &Request) -> Result<Response, JevError> {
        if let Some(e) = &self.endpoint_error {
            return Err(JevError::InsecureEndpoint(e.clone()));
        }
        let key = self.api_key.as_deref().ok_or(JevError::MissingKey)?;
        let url = format!("{}/v1/systemone", self.api_url);
        let mut attempt = 0u32;
        loop {
            let result = self.http.post(&url).bearer_auth(key).json(req).send().await;
            let (err, retry_after) = match result {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let retry_after = retry_after(resp.headers());
                    match status {
                        200..=299 => {
                            let body = resp
                                .bytes()
                                .await
                                .map_err(|e| JevError::Network(e.without_url().to_string()))?;
                            return serde_json::from_slice::<Response>(&body)
                                .map_err(|e| JevError::Malformed(e.to_string()));
                        }
                        401 | 403 => return Err(JevError::Unauthorized),
                        400 | 404 | 413 | 422 => {
                            let body = resp.text().await.unwrap_or_default();
                            return Err(JevError::BadRequest(format!(
                                "{status}: {}",
                                truncate(&body, 300)
                            )));
                        }
                        408 => (JevError::Timeout, retry_after),
                        429 => (JevError::RateLimited, retry_after),
                        529 => (JevError::Overloaded, retry_after),
                        s if s >= 500 => (JevError::Server(s), retry_after),
                        s => {
                            return Err(JevError::BadRequest(format!("unexpected status {s}")));
                        }
                    }
                }
                Err(e) if e.is_timeout() => (JevError::Timeout, None),
                Err(e) => (JevError::Network(e.without_url().to_string()), None),
            };
            if attempt >= self.max_retries {
                return Err(err);
            }
            let delay = retry_after
                .unwrap_or_else(|| backoff(attempt))
                .min(self.max_backoff);
            tracing::warn!(attempt, ?delay, error = %err, "retrying Jev request");
            tokio::time::sleep(delay).await;
            attempt += 1;
        }
    }
}

/// The key and source code travel in the request, so the endpoint must use
/// https. Plain http is allowed only on loopback, which the tests' local mock
/// server uses.
pub fn endpoint_error(url: &str) -> Option<String> {
    let parsed = match reqwest::Url::parse(url) {
        Ok(u) => u,
        Err(e) => return Some(format!("invalid JEV_RUST_REVIEW_API_URL: {e}")),
    };
    let host = parsed.host_str().unwrap_or("");
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    match parsed.scheme() {
        "https" => None,
        "http" if loopback => None,
        scheme => Some(format!(
            "JEV_RUST_REVIEW_API_URL must use https (got {scheme}://{}); plain http is allowed only for localhost",
            parsed.host_str().unwrap_or("")
        )),
    }
}

/// Exponential backoff 0.5s, 1s, 2s, ... capped at 8s, with up to 25%
/// jitter subtracted so concurrent units do not retry in lockstep.
pub fn backoff(attempt: u32) -> Duration {
    let base = (500u64 << attempt.min(4)).min(8_000);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let jitter = base / 4 * (nanos % 1000) / 1000;
    Duration::from_millis(base - jitter)
}

/// Honour `retry-after-ms` (milliseconds) and `Retry-After` (seconds).
fn retry_after(h: &reqwest::header::HeaderMap) -> Option<Duration> {
    if let Some(ms) = h
        .get("retry-after-ms")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
    {
        return Some(Duration::from_millis(ms as u64));
    }
    h.get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|s| Duration::from_millis((s * 1000.0) as u64))
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        let mut end = n;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_documented_answers() {
        let body = r#"{"model":"jev-1.13.0","answers":{
            "a":{"type":"noul","noul":0.98},
            "b":{"type":"choice","choice":"high","confidence":0.82,"probabilities":{"low":0.09,"high":0.91}},
            "c":{"type":"score","score":0.51,"confidence":0.24,"legend":{"0":"poor","1":"ok","2":"good"},"probabilities":{"0":0.53,"1":0.43,"2":0.04}}
        },"usage":{"input_tokens":373,"output_tokens":61}}"#;
        let r: Response = serde_json::from_str(body).unwrap();
        assert_eq!(r.usage.input_tokens, 373);
        assert_eq!(r.answer("a").unwrap(), Answer::Noul { noul: 0.98 });
        assert!(
            matches!(r.answer("b").unwrap(), Answer::Choice { ref choice, .. } if choice == "high")
        );
        assert!(
            matches!(r.answer("c").unwrap(), Answer::Score { score, .. } if (score - 0.51).abs() < 1e-9)
        );
        assert!(r.answer("missing").is_err());
    }

    #[test]
    fn rejects_malformed_answers() {
        let body = r#"{"model":"m","answers":{
            "x":{"type":"noul","noul":1.7},
            "y":{"type":"noul"},
            "z":{"type":"mystery","v":1},
            "w":{"type":"choice","choice":"a","probabilities":{"a":"high"},"confidence":0.5}
        }}"#;
        let r: Response = serde_json::from_str(body).unwrap();
        for id in ["x", "y", "z", "w"] {
            assert!(r.answer(id).is_err(), "{id}");
        }
        assert_eq!(r.usage, Usage::default());
    }

    #[test]
    fn endpoint_must_be_https_or_loopback() {
        assert_eq!(endpoint_error("https://api.typesafe.ai"), None);
        assert_eq!(endpoint_error("http://127.0.0.1:9"), None);
        assert_eq!(endpoint_error("http://localhost:8080"), None);
        assert_eq!(endpoint_error("http://[::1]:8080"), None);
        for bad in [
            "http://api.typesafe.ai",
            "http://10.0.0.5",
            "http://localhost.evil.com",
            "ftp://127.0.0.1",
            "not a url",
        ] {
            assert!(endpoint_error(bad).is_some(), "{bad} accepted");
        }
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert!(backoff(0) <= Duration::from_millis(500));
        assert!(backoff(0) >= Duration::from_millis(375));
        assert!(backoff(10) <= Duration::from_millis(8_000));
        assert!(backoff(10) >= Duration::from_millis(6_000));
    }

    #[test]
    fn retry_after_headers() {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("retry-after", "2".parse().unwrap());
        assert_eq!(retry_after(&h), Some(Duration::from_secs(2)));
        h.insert("retry-after-ms", "150".parse().unwrap());
        assert_eq!(retry_after(&h), Some(Duration::from_millis(150)));
        let mut bad = reqwest::header::HeaderMap::new();
        bad.insert(
            "retry-after",
            "Wed, 21 Oct 2015 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(retry_after(&bad), None);
    }
}

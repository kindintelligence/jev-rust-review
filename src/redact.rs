//! Keep secrets out of requests: skip secret-bearing files by name and
//! redact token-shaped strings line by line.

use regex::Regex;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// True for files that must never be sent, even when they appear in the diff.
pub fn is_secret_file(path: &str) -> bool {
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase();
    name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".env")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
        || name.ends_with(".keystore")
        || name.ends_with(".jks")
        || name.starts_with("id_rsa")
        || name.starts_with("id_dsa")
        || name.starts_with("id_ecdsa")
        || name.starts_with("id_ed25519")
        || name == ".netrc"
        || name == ".npmrc"
        || name == ".pypirc"
        || name.contains("credential")
        || name.contains("secret")
}

struct Rule {
    kind: &'static str,
    re: Regex,
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let r = |kind, pat: &str| Rule {
        kind,
        re: Regex::new(pat).expect("static regex"),
    };
    vec![
        r("private_key", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
        r("aws_access_key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
        r(
            "github_token",
            r"\b(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{40,})\b",
        ),
        r("gitlab_token", r"\bglpat-[A-Za-z0-9_\-]{20,}\b"),
        r("slack_token", r"\bxox[abposr]-[A-Za-z0-9\-]{10,}\b"),
        r("stripe_key", r"\b[rs]k_(?:live|test)_[A-Za-z0-9]{16,}\b"),
        r("google_api_key", r"\bAIza[0-9A-Za-z_\-]{35}\b"),
        r("sk_token", r"\bsk-(?:[A-Za-z0-9_\-]{20,})\b"),
        r(
            "jwt",
            r"\beyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\b",
        ),
        r(
            "url_credentials",
            r"\b[a-zA-Z][a-zA-Z0-9+.\-]*://[^/\s:@]+:[^/\s@]+@",
        ),
    ]
});

/// `name = "value"` / `name: "value"` where the name looks secret-ish.
static SECRET_ASSIGN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)\b([a-z0-9_]*(?:api_?key|secret|token|passw(?:or)?d|passwd|pwd|auth|credential|private_?key)[a-z0-9_]*)\s*(?::\s*&?(?:'static\s+)?str\s*)?[:=]\s*(?:&?\s*)?"([^"]{8,})""#,
    )
    .expect("static regex")
});

/// Any string literal long enough to be a key.
static STRING_LITERAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""([A-Za-z0-9+/=_\-]{32,})""#).expect("static regex"));

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Redactions {
    pub by_kind: BTreeMap<String, usize>,
}

impl Redactions {
    pub fn total(&self) -> usize {
        self.by_kind.values().sum()
    }
    pub fn merge(&mut self, other: &Redactions) {
        for (k, v) in &other.by_kind {
            *self.by_kind.entry(k.clone()).or_default() += v;
        }
    }
    fn add(&mut self, kind: &str) {
        *self.by_kind.entry(kind.to_string()).or_default() += 1;
    }
}

/// Shannon entropy in bits per character.
fn entropy(s: &str) -> f64 {
    let mut counts = [0usize; 256];
    for b in s.bytes() {
        counts[b as usize] += 1;
    }
    let n = s.len() as f64;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / n;
            -p * p.log2()
        })
        .sum()
}

/// Redact one line, recording what was removed.
pub fn redact_line(line: &str, acc: &mut Redactions) -> String {
    let mut out = line.to_string();
    for rule in RULES.iter() {
        if rule.re.is_match(&out) {
            let n = rule.re.find_iter(&out).count();
            for _ in 0..n {
                acc.add(rule.kind);
            }
            let tag = format!("[REDACTED:{}]", rule.kind);
            out = rule.re.replace_all(&out, tag.as_str()).into_owned();
        }
    }
    if let Some(c) = SECRET_ASSIGN.captures(&out) {
        let value = c.get(2).map(|m| m.as_str()).unwrap_or("");
        // Skip obvious non-secrets such as env var names or placeholders.
        let placeholder = value.starts_with("[REDACTED")
            || value.chars().all(|c| c.is_ascii_uppercase() || c == '_')
            || value.contains("${")
            || value.contains('{') && value.contains('}');
        if !placeholder && entropy(value) >= 3.0 {
            let m = c.get(2).expect("group 2");
            out.replace_range(m.range(), "[REDACTED:secret_assignment]");
            acc.add("secret_assignment");
        }
    }
    let snapshot = out.clone();
    for c in STRING_LITERAL.captures_iter(&snapshot) {
        let v = c.get(1).expect("group 1").as_str();
        if entropy(v) >= 4.3 && v.chars().any(|c| c.is_ascii_digit()) {
            out = out.replacen(v, "[REDACTED:high_entropy]", 1);
            acc.add("high_entropy");
        }
    }
    out
}

/// Redact every line of a text block.
pub fn redact_text(text: &str, acc: &mut Redactions) -> String {
    let mut in_pem = false;
    let mut out = Vec::new();
    for line in text.split('\n') {
        if in_pem {
            if line.contains("-----END") {
                in_pem = false;
            }
            continue;
        }
        let r = redact_line(line, acc);
        if line.contains("PRIVATE KEY-----")
            && line.contains("-----BEGIN")
            && !line.contains("-----END")
        {
            in_pem = true;
        }
        out.push(r);
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_files() {
        for p in [
            ".env",
            "config/.env.production",
            "certs/server.pem",
            "a/b/tls.key",
            "home/id_rsa",
            "id_ed25519.pub",
            "aws_credentials.toml",
            "src/secrets.rs",
        ] {
            assert!(is_secret_file(p), "{p}");
        }
        for p in [
            "src/main.rs",
            "Cargo.toml",
            "src/keyboard.rs",
            "environment.rs",
        ] {
            assert!(!is_secret_file(p), "{p}");
        }
    }

    #[test]
    fn token_formats() {
        let mut acc = Redactions::default();
        let l = redact_line(
            r#"let k = "AKIAABCDEFGHIJKLMNOP"; let g = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";"#,
            &mut acc,
        );
        assert!(!l.contains("AKIAABCDEFGHIJKLMNOP"));
        assert!(!l.contains("ghp_abc"));
        assert_eq!(acc.by_kind["aws_access_key"], 1);
        assert_eq!(acc.by_kind["github_token"], 1);
    }

    #[test]
    fn secret_assignment_and_entropy() {
        let mut acc = Redactions::default();
        let l = redact_line(r#"const API_KEY: &str = "q8Zr2LmP0xVb7Nw4";"#, &mut acc);
        assert!(l.contains("[REDACTED:secret_assignment]"), "{l}");
        let l2 = redact_line(r#"let password = "PASSWORD_ENV";"#, &mut acc);
        assert!(l2.contains("PASSWORD_ENV"), "env var names are not secrets");
        let l3 = redact_line(
            r#"let blob = "Zm9vYmFyYmF6cXV4MTIzNDU2Nzg5MGFiY2RlZmdoaWprbG1ub3A0Nzk4";"#,
            &mut acc,
        );
        assert!(l3.contains("[REDACTED:high_entropy]"), "{l3}");
    }

    #[test]
    fn ordinary_code_untouched() {
        let mut acc = Redactions::default();
        let src = "let token = parser.next_token();\nlet s = \"hello world, this is plain text\";";
        assert_eq!(redact_text(src, &mut acc), src);
        assert_eq!(acc.total(), 0);
    }

    #[test]
    fn pem_block_removed() {
        let mut acc = Redactions::default();
        let src =
            "a\n-----BEGIN RSA PRIVATE KEY-----\nMIIEow\nabc\n-----END RSA PRIVATE KEY-----\nb";
        let out = redact_text(src, &mut acc);
        assert!(!out.contains("MIIEow"));
        assert!(out.starts_with("a\n") && out.ends_with("\nb"));
        assert_eq!(acc.by_kind["private_key"], 1);
    }
}

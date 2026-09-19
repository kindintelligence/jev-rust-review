//! MCP surface: three tools over stdio. Output is compact JSON text.

use crate::config::Config;
use crate::jev;
use crate::review::{self, EvaluateParams, Finding};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig},
    schemars::{self, JsonSchema},
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EvaluateArgs {
    /// Repository path. Defaults to the session's project directory.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// What to review: empty or "working" (uncommitted changes incl. untracked
    /// .rs files), "staged", a range like "main...HEAD" or "a..b", a single
    /// commit ("rev:<sha>" or a bare rev), or a path ("path:src/x.rs" or an
    /// existing path; reviewed whole if it has no uncommitted changes).
    #[serde(default)]
    pub scope: Option<String>,
    /// Return the exact request bodies that would be sent to Jev without
    /// sending anything.
    #[serde(default)]
    pub dry_run: Option<bool>,
    /// Override framework profile detection, e.g. ["tokio"] or ["none"].
    #[serde(default)]
    pub profiles: Option<Vec<String>>,
    /// Cap on the number of units evaluated (default 60).
    #[serde(default)]
    pub max_units: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DiagnosticsArgs {
    /// Repository path. Defaults to the session's project directory.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// The same scope string the review uses; it decides which lines count
    /// as changed.
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VerifyArgs {
    /// Repository path. Defaults to the session's project directory.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// The scope the findings came from; decides which revision of each file
    /// is read (working tree by default).
    #[serde(default)]
    pub scope: Option<String>,
    /// Return the exact request bodies without sending anything.
    #[serde(default)]
    pub dry_run: Option<bool>,
    /// Candidate findings (1-20). The server re-reads the code itself.
    pub findings: Vec<Finding>,
}

#[derive(Clone)]
pub struct Server {
    cfg: Config,
    client: jev::Client,
    tool_router: ToolRouter<Self>,
}

impl Server {
    pub fn new(cfg: Config) -> Self {
        let client = jev::Client::new(&cfg);
        Server {
            cfg,
            client,
            tool_router: Self::tool_router(),
        }
    }

    /// Repo path: explicit argument, then `CLAUDE_PROJECT_DIR` (exported to
    /// plugin MCP servers), then MCP roots if the client advertises them,
    /// then the process working directory.
    #[allow(deprecated)] // roots/list is deprecated (SEP-2577) but still served.
    async fn repo_path(&self, arg: Option<String>, ctx: &RequestContext<RoleServer>) -> PathBuf {
        if let Some(p) = arg.filter(|p| !p.trim().is_empty()) {
            return PathBuf::from(p);
        }
        if let Ok(p) = std::env::var("CLAUDE_PROJECT_DIR")
            && !p.trim().is_empty()
        {
            return PathBuf::from(p);
        }
        let has_roots = ctx.client_capabilities().is_some_and(|c| c.roots.is_some());
        if has_roots
            && let Ok(Ok(r)) =
                tokio::time::timeout(std::time::Duration::from_secs(5), ctx.peer.list_roots()).await
            && let Some(p) = r.roots.first().and_then(|root| file_uri_to_path(&root.uri))
        {
            return p;
        }
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

/// `file:///home/x` → `/home/x`; `file:///C:/x` → `C:/x`.
pub fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let mut decoded = Vec::with_capacity(rest.len());
    let b = rest.as_bytes();
    let mut i = 0;
    while let Some(&byte) = b.get(i) {
        let hex = b
            .get(i + 1..i + 3)
            .filter(|_| byte == b'%')
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(v) => {
                decoded.push(v);
                i += 3;
            }
            None => {
                decoded.push(byte);
                i += 1;
            }
        }
    }
    let s = String::from_utf8(decoded).ok()?;
    // "/C:/x" is a Windows drive path; drop the leading slash.
    let s = match s.as_bytes() {
        [b'/', _, b':', ..] => s.get(1..).unwrap_or(&s).to_string(),
        _ => s,
    };
    Some(PathBuf::from(s))
}

fn json_result<T: serde::Serialize>(v: &T) -> CallToolResult {
    match serde_json::to_string(v) {
        Ok(s) => CallToolResult::success(vec![ContentBlock::text(s)]),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(format!(
            "failed to serialize result: {e}"
        ))]),
    }
}

fn tool_error(e: impl std::fmt::Display) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(
        serde_json::json!({"status": "error", "reason": e.to_string()}).to_string(),
    )])
}

#[tool_router]
impl Server {
    #[tool(
        description = "Run the deterministic tools on a Rust change and return only what touches it. Runs `cargo clippy --all-targets --message-format=json` (every rustc diagnostic, Clippy's defaults, and a fixed set of off-by-default lints for lossy casts, needless ownership, redundant clones, ignored must-use values, discarded errors, non-Send fields and wildcard enum arms), then filters in code: errors anywhere, warnings on changed lines only, and nothing the project's own lint configuration allows. When a library crate's `pub` surface changed and cargo-semver-checks is installed, runs it against the scope's old commit; it never installs anything. Suggests Miri when `unsafe` changed and does not run it. The output is fact: report it as it is, and never report the same defect again as a finding of your own. cargo runs the project's build scripts and proc macros, so ask first in an untrusted repository. status is ok | disabled | skipped | failed | timeout. Call this before evaluate_rust_changes."
    )]
    async fn cargo_diagnostics(
        &self,
        Parameters(args): Parameters<DiagnosticsArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let repo = self.repo_path(args.repo_path, &ctx).await;
        Ok(
            match review::diagnostics(&self.cfg, &repo, args.scope).await {
                Ok(out) => json_result(&out),
                Err(e) => tool_error(e),
            },
        )
    }

    #[tool(
        description = "Triage Rust changes with TypeSafe Jev. Collects the diff for a scope itself, splits it into small units (changed lines plus enclosing items), asks typed yes/no and rating questions per review dimension, and returns per-unit answers, probabilities, thresholds, `flagged` (unit, dimension) pairs sorted by signal, project facts (edition, MSRV, runtime, framework profiles, and whether the diff touches tests), deterministic Cargo facts, redaction counts, token usage and cost. Every question asks for a judgement no compiler check, lint or cargo tool makes; a flag on lines where cargo_diagnostics already reported the same defect is moved to `tool_covered` and must not become a finding. status is ok | partial | jev_unavailable | dry_run. Flags mark where to look; they are not findings."
    )]
    async fn evaluate_rust_changes(
        &self,
        Parameters(args): Parameters<EvaluateArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let repo = self.repo_path(args.repo_path, &ctx).await;
        let params = EvaluateParams {
            scope: args.scope,
            dry_run: args.dry_run.unwrap_or(false),
            profiles: args.profiles,
            max_units: args.max_units.map(|n| n as usize),
        };
        Ok(
            match review::evaluate(&self.cfg, &self.client, &repo, params).await {
                Ok(out) => json_result(&out),
                Err(e) => tool_error(e),
            },
        )
    }

    #[tool(
        description = "Verify candidate Rust review findings with TypeSafe Jev before reporting them. For each finding (dimension, file, start_line, end_line, one-sentence claim naming identifiers rather than line numbers, proposed severity) the server re-reads the code itself and returns: `support` (Jev's choice of supported / refuted / insufficient_context, with probabilities), `supported` (the probability of `supported`, which the report bar applies to), Jev's independent `severity` score (level name, p_high_or_above, confidence), a `category` choice (real_defect / debatable_tradeoff / style_preference), and a `verdict`: report | insufficient_context | uncertain | dismiss | tool_reported. tool_reported means cargo_diagnostics already reported this defect on these lines (`tool` names the lint): drop the finding and let the tool's diagnostic stand. That check needs no API key, so call this tool even when Jev is unavailable. insufficient_context means the claim depends on code outside the excerpt; it is not a refutation, so keep such a finding only on strong independent evidence."
    )]
    async fn verify_rust_findings(
        &self,
        Parameters(args): Parameters<VerifyArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let repo = self.repo_path(args.repo_path, &ctx).await;
        Ok(
            match review::verify(
                &self.cfg,
                &self.client,
                &repo,
                args.scope,
                args.findings,
                args.dry_run.unwrap_or(false),
            )
            .await
            {
                Ok(out) => json_result(&out),
                Err(e) => tool_error(e),
            },
        )
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Rust code review helpers: deterministic tools first, then TypeSafe Jev for what no tool can answer. Call cargo_diagnostics for the compiler and lint facts on the changed lines, then evaluate_rust_changes to find where else to look, inspect the flagged code yourself, then call verify_rust_findings on your candidate findings before reporting them. Never report a defect a tool already reported. Jev numbers are routing signals; report them labelled as Jev's.",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris() {
        assert_eq!(
            file_uri_to_path("file:///home/me/my%20repo"),
            Some(PathBuf::from("/home/me/my repo"))
        );
        assert_eq!(
            file_uri_to_path("file:///C:/code/x"),
            Some(PathBuf::from("C:/code/x"))
        );
        assert_eq!(file_uri_to_path("https://x"), None);
    }
}

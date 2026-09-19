# jev-rust-review — design

v0.1. Keep this file short; it records decisions, not tutorials.

```text
rustc / cargo / clippy  = deterministic facts        (Claude runs them via Bash)
Jev                     = typed semantic judgment     (this MCP server: triage + verification)
Claude                  = reasoning, root cause, fix  (skill + rust-reviewer agent)
```

## 1. What was verified (2026-09-19)

| Fact | Source | Result |
|---|---|---|
| Endpoint `POST https://api.typesafe.ai/v1/systemone`, Bearer auth, body `{state, model, questions}` | [API reference](https://docs.typesafe.ai/api.md), live call | Confirmed |
| Noul answer `{type, noul}` with no confidence; Choice/Score carry `probabilities` + `confidence` | [API](https://docs.typesafe.ai/api.md), [Confidence](https://docs.typesafe.ai/confidence.md), live call | Confirmed. Response also carries `usage.{input_tokens,output_tokens}` |
| `GET /v1/models` | [Models](https://docs.typesafe.ai/models.md), live call | Lists only aliases `jev-latest`, `jev-preview` |
| Current model | live call | Response `model` is the versioned id `jev-1.13.0` even when the alias is sent |
| Context | [Models](https://docs.typesafe.ai/models.md) | 64k tokens per request **and 32k for `state` + the longest question** |
| Price | [Models](https://docs.typesafe.ai/models.md) | $0.042 per million input tokens; output tokens free |
| Rate limits | [Models](https://docs.typesafe.ai/models.md) | 250k tok/s, 1,200 req/min, "adjusting dynamically" |
| Retry headers | [Python SDK retries](https://docs.typesafe.ai/sdk/python/api/retries.md) | SDK honours `Retry-After` and `retry-after-ms`; retries 408, 429, 5xx; default timeout 10 s |
| Weak spots | [Jev 1.13 jaggedness](https://docs.typesafe.ai/model-jaggedness/jev-1.13.md) | Literal reading, arithmetic/counting, dates, indirection, large irrelevant state, adversarial content, contradictory instructions/criteria, no structural invariants between questions, no generation |
| Structured instructions/criteria (JSON objects) | [Advanced](https://docs.typesafe.ai/primitives/advanced.md) | Allowed for instructions, Choice option descriptions, Score levels, Noul `true`/`false` |
| rmcp | crates.io, repo tag `rmcp-v3.4.0`, compiled probe | 3.4.0, MSRV 1.88, features `server macros transport-io`, schemars 1.x, `ContentBlock` (not `Content`), `ServerConfig` (not `ServerInfo`), `roots/list` deprecated (SEP-2577) and hangs if the client did not advertise roots |
| Plugin layout, `userConfig`, `${user_config.KEY}`, `${CLAUDE_PLUGIN_ROOT}`, `${CLAUDE_PLUGIN_DATA}` | [plugins-reference](https://code.claude.com/docs/en/plugins-reference) | Confirmed. `CLAUDE_PLUGIN_ROOT`, `CLAUDE_PLUGIN_DATA`, `CLAUDE_PROJECT_DIR` are exported to MCP subprocesses. Data dir `~/.claude/plugins/data/<id>/` survives updates |
| MCP startup timeout | [env vars](https://code.claude.com/docs/en/env-vars) | `MCP_TIMEOUT`, default 30 000 ms |
| Tool names | [mcp](https://code.claude.com/docs/en/mcp) | `mcp__plugin_<plugin>_<server>__<tool>` → `mcp__plugin_jev-rust-review_jev__evaluate_rust_changes` |
| Skill invocation | [skills](https://code.claude.com/docs/en/skills) | `/jev-rust-review:rust-review` (bare `/rust-review` works when unambiguous) |
| Subagents and MCP | [sub-agents](https://code.claude.com/docs/en/sub-agents) | Subagents inherit MCP tools unless `tools` restricts them |
| CLI | `claude plugin validate --help`, `claude --help` (v2.1.277) | `claude plugin validate <path>`, `claude --plugin-dir <path>` |
| Dioxus | crates.io, [dioxuslabs.com/learn/0.7](https://dioxuslabs.com/learn/0.7/) | 0.7.10 stable; `ReadOnlySignal` deprecated alias of `ReadSignal`; stores, `use_action`, `use_loader`, `#[get]/#[post]` server fns |
| Axum / Tokio | docs.rs | axum 0.8.9 (`/{id}` paths; `/:id` panics), tokio 1.53.1 |
| Codex | [Codex MCP docs](https://learn.chatgpt.com/docs/extend/mcp?surface=cli) | `[mcp_servers.<name>]` with `command`, `args`, `env`, `env_vars`, `startup_timeout_sec` (default 10) |

## 2. Where reality differs from the brief

1. **Malformed questions return 400, not 422.** Verified live (`{"detail":"Noul question must have criteria or instructions: a"}`). Both are treated as non-retryable "bad request".
2. **Two context limits.** The 32k `state`+longest-question limit binds before 64k. Units are budgeted against that, with a much smaller default because accuracy drops with irrelevant state.
3. **Default model is pinned to `jev-1.13.0`, not an alias.** The Models page says to pin the version you tuned thresholds against. The alias is one env var away.
4. **`roots/list` is deprecated in rmcp 3.x.** Repo path resolution order: explicit `repo_path` → `CLAUDE_PROJECT_DIR` → MCP roots (only if the client advertised them) → process cwd.
5. **Testing adequacy is a code fact, not a Jev question.** "Is the changed behaviour tested?" needs cross-file tracing (a documented weak spot). The server reports which changed units are test code and whether any test code changed. Claude judges adequacy.
6. **Cargo/dependency risk is mostly deterministic.** New deps, git/path/wildcard versions, new `build.rs`, new proc-macros and lockfile churn are computed from the diff. Jev gets one question on the manifest diff.
7. **The Axum extractor-ordering rule is enforced by the compiler.** The body extractor must be last, or the handler does not implement `Handler`. It is left to `cargo check` and not asked of Jev.
8. **Tool output is plain JSON text, not MCP `structuredContent`.** rmcp's `Json<T>` sends both, which doubles the tokens a client ingests.

## 3. MCP tools

Server name `jev` in `.mcp.json`. The server does its own git work. Scopes are validated, and git is spawned with an argument vector: no shell, `--` separators, and revs resolved to SHAs through `git rev-parse --verify --end-of-options` before use.

### `evaluate_rust_changes`

Input: `repo_path?`, `scope?`, `dry_run?`, `profiles?` (override auto-detection, e.g. `["tokio"]` or `["none"]`), `max_units?`.

Scopes:

| Scope | Meaning |
|---|---|
| empty, `working` | Staged and unstaged changes vs `HEAD`, plus untracked `.rs` files |
| `staged` | `git diff --cached` |
| `A..B`, `A...B` | Range diff |
| `rev:<r>` or a bare rev | The changes that commit introduced (parent → commit; root commit vs the empty tree) |
| `path:<p>` or an existing path | Uncommitted changes under the path. If there are none, the `.rs` files under it are reviewed whole (capped) |

A scope beginning with `-`, or containing NUL or a newline, is rejected. A string that is both a path and a rev resolves to the path; prefixes disambiguate.

Output (JSON):

- `status`: `ok` | `partial` | `jev_unavailable` | `dry_run`, plus `reason`.
- `repo`, `scope` (resolved SHAs), `model`.
- `project`: crates (name, dir, edition, `rust_version`, kind, `async_runtime`, `profiles`, features, `mutually_exclusive_features`), workspace members, policy files present.
- `flagged`: (unit, dimension, strongest signal) pairs, sorted. Claude reads this first.
- `units`: `id`, `file`, `lines`, `changed_lines` (all computed from the diff), `role` (library/binary/test/example/bench/build_script), `status`, and per question: `dimension`, `primitive`, `answer`, `probabilities?`, `confidence?`, `threshold`, `flagged`.
- `cargo_facts`: deterministic manifest and lockfile findings.
- `tests`: whether test code changed and which changed units are test code.
- `skipped`: file or unit plus reason (secret-bearing file, binary, non-Rust, deleted, over budget, and so on).
- `redactions`: count by kind.
- `usage`: input tokens and estimated USD.
- `cargo`: whether cargo steps are enabled and suggested commands (feature caveat included).
- `payloads`: only in dry run. The exact request bodies, minus the auth header.

### `verify_rust_findings`

Input: `repo_path?`, `scope?` (determines which revision the file is read from), `dry_run?`, and `findings[1..=20]`. Each finding has `id?`, `dimension`, `file`, `start_line`, `end_line`, `claim` (one sentence, no line numbers) and `severity` (critical/high/medium/low).

The server re-reads the file itself. The state holds the enclosing item, with the claimed lines marked `>`, plus imports and the claim. It never includes the proposed severity, so Jev's severity is an independent judgment.

Output per finding:

- `supported` (Noul probability)
- `severity` (Choice with probabilities and confidence) and `severity_agrees`
- `category` (Choice: real_defect / debatable_tradeoff / style_preference / not_supported, with probabilities and confidence)
- `verdict` (`report` | `uncertain` | `dismiss`) and the thresholds used

## 4. Jev question set

Questions live in `src/questions.rs` as static data: id, dimension, primitive, instructions, criteria, lexical gate, excluded roles, and profile. The pipeline iterates over the data. Adding a dimension question or a profile does not touch the pipeline.

Principles, each taken from the jaggedness page:

- Instructions are literal and single-hop, name the state field (`code`), and have no negations. Noul criteria mirror the instruction (`true` = the defect is present).
- Every question is about the changed lines (`+`) or the code they directly affect. Nothing asks for line numbers, counts, compilation results or arithmetic.
- Gating happens in code with cheap regexes (`unsafe`, `.await`, `select!`, `extern "C"`, `macro_rules!`, `Serialize`, `pub`, `as u8`, and so on). An ungated question costs little, but an irrelevant one adds noise.
- Complementary questions are never assumed consistent. Each flag rule reads exactly one answer.
- The state marks the code as untrusted data in a separate `notes` field. The code itself sits in its own JSON string field, so it is structurally delimited. The fixture `prompt_injection` tests this.

Dimensions (16, each with a reference file): correctness, ownership, type_design, error_handling, async, concurrency, unsafe, ffi, performance, idiom, api, macros, serde, security, testing (code fact), cargo. Profiles: tokio, axum, dioxus (data plus a reference file each).

## 5. Units and chunking

- Only `.rs` files become Jev units. `Cargo.toml` diffs become one manifest unit each, and `Cargo.lock` is reduced to deterministic facts.
- The file is parsed with `syn` (full). Each changed line maps to its innermost enclosing item: a fn, a method in an impl or trait, or a top-level struct, enum, const or macro. The unit is the union of those items. Lines outside any item, or files that do not parse, get a ±6-line window.
- A unit's code is shown diff-style (`+` added, `-` removed, space for context), without line numbers. `imports` (top-level `use` lines) and the enclosing `impl`/`trait` header are added because they change meaning (for example `std::sync::Mutex` vs `tokio::sync::Mutex`).
- Budget: tokens are estimated conservatively as `ceil(bytes / 3)` plus overhead per question. A unit over `max_unit_tokens` (default 6 000) is split into windows around its hunks. A review over `max_total_tokens` (default 400 000, about $0.017) or over `max_units` (default 60) stops adding units and lists the rest under `skipped`.
- Concurrency is 4 requests. Each has a 30 s timeout and 3 retries with exponential backoff (0.5 s → 8 s, jittered) on 408, 429, 5xx (529 included), timeouts and connection errors. `Retry-After`/`retry-after-ms` is honoured, capped at 30 s. A 401 short-circuits the remaining units into `jev_unavailable`. A failed unit does not fail the review.

## 6. Threshold model

- **Triage (recall).** A Noul question flags when `noul >= threshold[dimension]`. Defaults are unsafe/async/security/concurrency 0.25, correctness/error_handling/ffi 0.30, serde/api/macros/cargo 0.35, ownership/type_design/performance 0.45, idiom 0.60. A Score question flags when the probability mass on its "bad" levels is at or above the threshold.
- **Noul uncertainty.** `|p − 0.5|` is reported as `margin`. It is useful for display and never used as confidence.
- **Verification (precision).** `verdict = report` when `supported >= report[dimension]` (default 0.70; unsafe 0.80) **and** the category choice is `real_defect` or `debatable_tradeoff` with `P(real_defect) >= 0.40`. `dismiss` when `supported < 0.40`, or the category choice is `not_supported` or `style_preference`. Everything else is `uncertain`, which the skill does not report unless deterministic evidence (compiler, clippy or test output) independently proves it.
- All thresholds can be overridden by env: `JEV_RUST_REVIEW_TRIAGE_THRESHOLDS="unsafe=0.2,idiom=0.7"`, `JEV_RUST_REVIEW_REPORT_THRESHOLDS=...`, `JEV_RUST_REVIEW_DISMISS_BELOW`. **The defaults are starting points to tune against the eval corpus** (see README for the run and its date).

## 7. Distribution

`.mcp.json` runs `sh ${CLAUDE_PLUGIN_ROOT}/scripts/launch.sh`. The launcher tries these in order:

1. `JEV_RUST_REVIEW_BIN`, if set and executable (for manual installs: `cargo install --git …`).
2. The cached binary `${CLAUDE_PLUGIN_DATA}/bin/<version>/jev-rust-review`.
3. `${CLAUDE_PLUGIN_ROOT}/target/release/jev-rust-review`, if its `--version` matches (local development). It is copied into the cache.
4. A prebuilt release asset for the host triple. It is verified against the release's `SHA256SUMS` before it is cached. Disable with `JEV_RUST_REVIEW_NO_DOWNLOAD=1`. Same-origin checksums prove integrity, not authenticity.
5. A local `cargo build --release --locked` into `${CLAUDE_PLUGIN_DATA}/build`, under a lock directory. It waits up to 20 s (below the 30 s MCP startup timeout). If the build is still running, the launcher exits with one message on stderr pointing at the build log and telling the user to reconnect via `/mcp` once it finishes.

Anything else exits non-zero with one actionable message. Release targets: x86_64/aarch64 Linux (musl, static) and x86_64/aarch64 macOS.

## 8. Security and privacy

- The key comes only from `TYPESAFE_API_KEY`. The plugin's `userConfig` (`sensitive: true`) maps into that same variable in `.mcp.json`. The key is never logged or echoed, and the `Debug` impl for config masks it.
- The server never serialises process environment into a request.
- Secret-bearing files are skipped by name: `.env*`, `*.pem`, `*.key`, `*.p12`, `*.pfx`, `id_rsa*`, `id_ed25519*`, and `*credential*`/`*secret*`. Token formats are redacted line by line (AWS, GitHub, GitLab, Slack, Stripe, Google, OpenAI/Anthropic-style `sk-`, JWT, PEM blocks), along with high-entropy string literals assigned to secret-ish names. Counts are reported.
- stdout carries only JSON-RPC. `#![deny(clippy::print_stdout)]` plus a test that scans `src/` for print macros and stdout handles, and a smoke test that asserts every stdout line parses as JSON-RPC.

## 9. Progress checklist

- [x] Preflight, private repo created
- [x] Research (TypeSafe, Claude Code plugins, rmcp, Dioxus/Axum/Tokio, Codex)
- [x] DESIGN.md
- [ ] Core modules: diff, git/scope, rust_project, redact, questions, context/units, jev client, review pipeline, config, error
- [ ] MCP server (rmcp) with evaluate + verify tools, dry run
- [ ] Unit tests (diff, scope, project, redaction, thresholds, response parsing)
- [ ] HTTP mock tests (401/400/422/429/529, Retry-After, timeout, malformed)
- [ ] stdout guard test
- [ ] Plugin: plugin.json, marketplace.json, .mcp.json, skill, references, agent
- [ ] scripts/launch.sh + fresh-install test; scripts/smoke.sh
- [ ] Fixture corpus (buggy + clean) + offline pipeline tests
- [ ] Live eval (ignored test) run + threshold tuning + README numbers
- [ ] CI (Linux + macOS) green; MSRV job
- [ ] Release workflow + first release
- [ ] README
- [ ] claude plugin validate; headless session check
- [ ] Secret scan of history; flip to public

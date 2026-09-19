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
9. **Complementary questions are never asked.** Verification used to pair a Noul ("is the claim supported?") with a category option `not_supported`. Jev does not promise that complementary questions agree, so support is now one Choice (`supported` / `refuted` / `insufficient_context`) and the category has no "not supported" option.
10. **Undocumented `unsafe` is Clippy's job.** `clippy::undocumented_unsafe_blocks` and `clippy::missing_safety_doc` answer it deterministically, so there is no Jev question for it; the skill runs Clippy with both lints and treats the output as evidence.
11. **The source-build fallback cannot fit the MCP startup window.** A cold `cargo build --release` measured 75 s (8 cores, registry already downloaded; aws-lc-sys alone 40 s), and 65 s with the `ring` provider instead of aws-lc. The startup timeout is 30 s. The launcher therefore never blocks on a cold build (§7).

## 3. MCP tools

Server name `jev` in `.mcp.json`. The server does its own git work. Scopes are validated, and git is spawned from one audited function with an argument vector: no shell, `--` separators, and revs resolved to SHAs through `git rev-parse --verify --end-of-options` before use.

### `evaluate_rust_changes`

Input: `repo_path?`, `scope?`, `dry_run?`, `profiles?` (override auto-detection, e.g. `["tokio"]` or `["none"]`), `max_units?`.

| Scope | Meaning |
|---|---|
| empty, `working` | Staged and unstaged changes vs `HEAD`, plus untracked `.rs` files |
| `staged` | `git diff --cached` |
| `A..B`, `A...B` | Range diff |
| `rev:<r>` or a bare rev | The changes that commit introduced (parent → commit; root commit vs the empty tree) |
| `path:<p>` or an existing path | Uncommitted changes under the path. If there are none, the `.rs` files under it are reviewed whole (capped) |

A scope beginning with `-`, or containing NUL or a newline, is rejected. A string that is both a path and a rev resolves to the path; prefixes disambiguate.

Output (compact JSON text):

- `status`: `ok` | `partial` | `jev_unavailable` | `dry_run`, plus `reason`.
- `repo`, `scope` (resolved SHAs), `model` (the versioned id Jev reported).
- `project`: crates (name, dir, edition, `rust_version`, kind, `async_runtimes`, profiles, features, `mutually_exclusive_features`), workspace members, policy files, toolchain, and **`tests`**: `diff_touches_tests` plus the test and non-test units. Whether the diff touched tests is a fact about the diff, computed from file roles and `#[cfg(test)]`/`#[test]` spans, not a Jev question.
- `flagged`: (unit, dimension, question, signal, threshold), strongest first. Claude reads this first.
- `references`: dimension and profile reference files to load.
- `units`: `id`, `file`, `lines`, `changed_lines` (from the diff), `role`, `status`, and per question: `dimension`, `primitive`, `answer`, `probabilities?`, `confidence?`, `signal`, `threshold`, `flagged`.
- `cargo_facts`: deterministic manifest and lockfile findings (new/removed/changed dependencies, git/path/wildcard sources, removed features, default-feature, edition and MSRV changes, new build scripts, proc-macro enablement, lockfile churn).
- `skipped`, `redactions`, `usage` (requests, input tokens, estimated USD), `cargo` (suggested commands, feature note).
- `payloads`: dry run only. The exact request bodies, without the auth header.

### `verify_rust_findings`

Input: `repo_path?`, `scope?` (which revision the file is read from), `dry_run?`, and `findings[1..=20]`, each with `id?`, `dimension`, `file`, `start_line`, `end_line`, `claim` (one sentence, identifiers not line numbers) and `severity`.

The server re-reads the file itself. The state holds the enclosing item with two marker columns (claim `>`/space, then diff `+`/`-`/space, so removed lines show the old side), the imports, the enclosing `impl`/`trait` header, any matching documentation facts (§4), and the claim. The proposed severity is never sent, so Jev's severity is an independent judgment. Output per finding: `support` (the Choice), `supported` (= `P(supported)`), `severity` (Score: nearest level name, weighted level, `p_high_or_above`, probabilities, confidence), `severity_agrees`, `category` (Choice), and `verdict` (§6).

## 4. Jev question set

All questions and thresholds live in `src/questions.rs` as data: id, dimension, primitive (Noul or Score), instructions, criteria, lexical gate, excluded roles, optional threshold, and for profiles the documentation they were `verified_against`. Gates are compiled once into `RegexSet`s and checked through `QuestionSpec::applies(unit, role, text)`.

Rules, each from the jaggedness page: literal single-hop instructions naming `code`; one defect per question; Noul criteria mirror the instruction; no counting, arithmetic, line numbers, or anything a tool answers; every flag reads exactly one answer.

**Gate contract.** Gates run on the unit text exactly as sent: the diff marker (`+`, `-`, space) in the first column, which `context::render` emits. No marker may open a gate. Gates never see verification text, which has a claim column in front, but fact gates do, so the tests cover both forms. (A pattern using `\s*` can reach across a line break into the next marker; `ARITHMETIC` did, and now uses `[ \t]*`.)

Core questions (44):

| Dimension | Questions |
|---|---|
| correctness | `logic`, `bounds`, `cast`, `overflow`, `wildcard`, `drop_order` |
| ownership | `clone`, `signature` |
| type_design | `loose_types` |
| error_handling | `swallowed`, `panic`, `lossy`, `drop_panic` |
| async | `guard_across_await`, `blocking_call`, `select_cancellation`, `detached_task`, `unbounded`, `sequential_awaits` |
| concurrency | `check_then_act`, `atomics`, `unsafe_send_sync`, `lock_scope` |
| unsafe | `memory_access`, `aliasing`, `transmute` |
| ffi | `ownership`, `pointers_and_strings`, `unwind` |
| performance | `repeated_work` |
| idiom | `clarity` (Score) |
| api | `breaking_change` |
| macros | `double_evaluation`, `name_collision` |
| serde | `compatibility`, `silent_default` |
| security | `injection`, `secret_exposure`, `tls_verification`, `weak_randomness`, `unbounded_input` |
| testing | `weak_assertion` |
| cargo (manifest units) | `manifest_risk`, `unpinned_source` |

Profile questions (17): tokio `runtime_nesting`, `spawn_blocking_misuse`, `no_shutdown`, `select_not_cancel_safe`, `blocking_in_async`, `async_mutex_unneeded`; axum `error_exposure`, `layer_order`, `extension_state`, `blocking_handler`; dioxus `guard_across_await`, `read_write_overlap`, `effect_loop`, `hook_rules`, `stale_capture`, `server_fn_trust`, `untracked_dependency`.

**Documentation facts** (`src/facts.rs`). Short, literal facts from official docs (for example which futures are not cancellation safe in `tokio::select!`) are added to a request's `state` when their gate matches the code, at most five. TypeSafe's Models page recommends putting reference material in `state`. In the first eval, the `select!` facts moved Jev's verification of a true cancellation claim from 0.17 to 0.74.

The state marks the code as untrusted data in a separate `notes` field and holds the code in its own JSON string field. The `prompt_injection` fixture tests this.

## 5. Units and chunking

- Only `.rs` files become code units. `Cargo.toml` diffs become one manifest unit each; `Cargo.lock` is reduced to deterministic facts.
- The file is parsed with `syn`. Each non-blank changed line maps to its innermost enclosing item (fn, method, or top-level item); the unit is the union of those items. Lines outside any item get ±2 lines when the file parsed, ±6 when it did not.
- Code is shown diff-style without line numbers, with the top-level `use` lines and the enclosing `impl`/`trait` header.
- Budget: tokens are estimated as `ceil(bytes / 3)`. A unit over `max_unit_tokens` (6 000) is split into windows around its changes. A review over `max_total_tokens` (400 000, about $0.017) or `max_units` (60) stops adding units and lists the rest under `skipped`.
- Concurrency 4, timeout 30 s, 3 retries with jittered backoff (0.5 s → 8 s) on 408, 429, 5xx (529 included), timeouts and connection errors. `Retry-After`/`retry-after-ms` is honoured, capped at 30 s. A 401 short-circuits the remaining units. A failed unit does not fail the review.

## 6. Threshold and verification model

**Triage (recall).** A Noul flags when `noul >= threshold`; a Score flags when the mass on its bad levels reaches the threshold. Defaults per dimension: unsafe, async, security, concurrency 0.25; correctness, error_handling, ffi 0.30; serde, api, macros, cargo 0.35; ownership, type_design, performance, testing 0.45; idiom 0.60. Per-question overrides: `correctness.wildcard` 0.45, `async.sequential_awaits` 0.55, `tokio.async_mutex_unneeded` 0.6.

**Verification (precision).** Three answers, each read on its own:

- `support` (Choice): `supported`, `refuted`, `insufficient_context`. The report bar applies to `P(supported)`: 0.70, or 0.80 for unsafe, idiom and type_design.
- `severity` (Score, levels 0 = low … 3 = critical). `SEVERITY_HIGH_FROM = 2`; `p_high_or_above` is the mass on high and critical.
- `category` (Choice): `real_defect`, `debatable_tradeoff`, `style_preference`.

Verdict, in order:

1. `dismiss` if `support` chose `refuted` or `category` chose `style_preference`.
2. `report` if `P(supported) >= report bar` and the category is `real_defect`, or `debatable_tradeoff` with `P(real_defect) >= 0.40`.
3. `insufficient_context` if `support` chose `insufficient_context`. **Not a refutation**: cross-file findings (lock ordering, semver breaks) land here. The skill keeps them when Claude's own confidence is High and says Jev could not verify them from local context.
4. `dismiss` if `P(supported) < 0.40`.
5. `uncertain` otherwise. The skill reports these only with deterministic evidence.

All bars are overridable by environment variable (README). They are starting points tuned against the eval corpus; retune only with eval evidence.

## 7. Distribution

`.mcp.json` runs `sh ${CLAUDE_PLUGIN_ROOT}/scripts/launch.sh`, which execs the first binary that reports the plugin's version:

1. `JEV_RUST_REVIEW_BIN`;
2. the cached `${CLAUDE_PLUGIN_DATA}/bin/<version>/jev-rust-review`;
3. `jev-rust-review` on `PATH` (for example from `cargo install`);
4. `${CLAUDE_PLUGIN_ROOT}/target/release/jev-rust-review` (local development);
5. a prebuilt release asset for the host triple, verified against the release's `SHA256SUMS` before it is cached (same-origin checksums prove integrity, not authenticity);
6. a local `cargo build --release --locked`, **detached** (`setsid`/`nohup`), waiting up to 20 s. If the build is still running the launcher exits with one message pointing at the log and telling the user to reconnect via `/mcp`; the next launch finds the cached binary. `launch.sh --install` runs the same steps in the foreground.

**Profiles.** `release` is tuned to build quickly (opt-level 2, no LTO) because step 6 compiles it on the user's machine; `dist` (thin LTO, one codegen unit) is for the binaries CI ships. The release workflow builds `--profile dist` for x86_64/aarch64 Linux (musl), x86_64/aarch64 macOS and x86_64 Windows, collecting from `target/<triple>/dist/`.

**Build time.** Cold `cargo build --release`: 75 s with aws-lc-rs (the default rustls provider; aws-lc-sys is 40 s of C), 65 s with `ring`. Neither fits the 30 s window, which is why step 5 comes first and step 6 never blocks startup. Switching to `ring` would save about 10 s and is not worth losing reqwest's default provider. `scripts/test-install.sh` proves both paths (source build from an empty data dir, verified download, tampered checksum refused) on every CI run.

## 8. Security and privacy

- The key comes only from `TYPESAFE_API_KEY`, or the plugin's `userConfig` (`sensitive: true`) mapped to `JEV_RUST_REVIEW_API_KEY`. It is never logged; `Config`'s `Debug` masks it.
- The process environment is never enumerated (`std::env::vars` is a disallowed method).
- Secret-bearing files are skipped by name (`.env*`, `*.pem`, `*.key`, `*.p12`, `*.pfx`, `id_rsa*`, `id_ed25519*`, `*credential*`, `*secret*`). Token formats and high-entropy secret assignments are redacted line by line, and counts are reported.
- stdout carries only JSON-RPC: `print!`/`println!` are disallowed macros, `#![deny(clippy::print_stdout)]` is set, a test scans `src/`, and the smoke test asserts every stdout line is JSON-RPC.

## 9. Progress checklist

- [x] Preflight, private repo created
- [x] Research (TypeSafe, Claude Code plugins, rmcp, Dioxus/Axum/Tokio, Codex)
- [x] Core modules, MCP server, dry run
- [x] Unit, HTTP-mock, pipeline, stdout-guard and plugin-consistency tests
- [x] Plugin: plugin.json, marketplace.json, .mcp.json, skill, references, agent
- [x] launch.sh + fresh-install test; smoke.sh
- [x] Fixture corpus (11 buggy, 7 clean), recordings, offline replay
- [x] Adopt the redesigned questions.rs, Cargo.toml and clippy.toml; new verification model
- [x] Live eval on the new question ids
- [x] CI and release workflows
- [x] README
- [ ] CI green on GitHub
- [ ] First tagged release (prebuilt binaries)
- [x] Headless Claude Code session check (2026-09-19, on the kiln repo: server connected, both tools listed, full skill run)
- [ ] Secret scan of history; flip to public

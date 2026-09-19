# jev-rust-review design

This file records the v0.1 design decisions and the reasons for them. Keep it short.

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

1. **Malformed questions return 400, not 422.** A live call returned `{"detail":"Noul question must have criteria or instructions: a"}`. The client treats both codes as a non-retryable bad request.
2. **There are two context limits.** The 32k limit on `state` plus the longest question binds before the 64k limit. Units are budgeted against it. The default budget is much smaller, because accuracy drops with irrelevant state.
3. **The default model is pinned to `jev-1.13.0`.** The Models page says to pin the version the thresholds were tuned against. One environment variable switches to an alias.
4. **`roots/list` is deprecated in rmcp 3.x.** The repo path comes from the first of these that is set:
   - an explicit `repo_path`;
   - `CLAUDE_PROJECT_DIR`;
   - MCP roots, if the client advertised them;
   - the process working directory.
5. **Test adequacy is a code fact, not a Jev question.** "Is the changed behaviour tested?" needs cross-file tracing, a documented Jev weak spot. The server reports which changed units are test code. Claude judges adequacy.
6. **Most Cargo risk is deterministic.** Code computes new dependencies and their sources and versions. It also finds new `build.rs` files, new proc macros and lockfile churn. Jev gets one question on the manifest diff.
7. **The compiler enforces Axum's extractor order.** The body extractor must come last, or the handler does not implement `Handler`. `cargo check` catches it, so Jev is not asked.
8. **Tool output is plain JSON text.** rmcp's `Json<T>` also sends MCP `structuredContent`. Sending both doubles the tokens a client reads.
9. **Complementary questions are never asked.** Verification once paired a Noul ("is the claim supported?") with a `not_supported` category option. Jev does not promise that such pairs agree. Support is now one Choice: `supported`, `refuted` or `insufficient_context`. The category has no "not supported" option.
10. **Clippy owns undocumented `unsafe`.** `clippy::undocumented_unsafe_blocks` and `clippy::missing_safety_doc` answer it deterministically. There is no Jev question for it. The skill runs Clippy with both lints and treats the output as evidence.
11. **A source build cannot fit the MCP startup window.** A cold `cargo build --release` took 75 s on 8 cores, with crates already downloaded. aws-lc-sys alone took 40 s. With the `ring` provider it took 65 s. The startup timeout is 30 s, so the launcher never blocks on a cold build (§7).

## 3. MCP tools

The server is named `jev` in `.mcp.json`. It does its own git work, and it validates every scope. One audited function spawns git. It passes an argument vector with no shell and uses `--` separators. It also resolves revs to SHAs with `git rev-parse --verify --end-of-options` before use.

### `evaluate_rust_changes`

Input: `repo_path?`, `scope?`, `dry_run?`, `profiles?` and `max_units?`. `profiles` overrides detection, for example `["tokio"]` or `["none"]`.

| Scope | Meaning |
|---|---|
| empty, `working` | Staged and unstaged changes vs `HEAD`, plus untracked `.rs` files |
| `staged` | `git diff --cached` |
| `A..B`, `A...B` | Range diff |
| `rev:<r>` or a bare rev | The changes that commit introduced (parent to commit; a root commit diffs against the empty tree) |
| `path:<p>` or an existing path | Uncommitted changes under the path. If there are none, the `.rs` files under it are reviewed whole (capped) |

The server rejects a scope that starts with `-` or contains a NUL or a newline. A string that is both a path and a rev resolves to the path. The `rev:` and `path:` prefixes disambiguate.

The output is compact JSON text:

- `status` (`ok`, `partial`, `jev_unavailable` or `dry_run`) and `reason`.
- `repo`, `scope` with resolved SHAs, and `model`, the versioned id Jev reported.
- `project`: crates, workspace members, policy files, toolchain and `tests`.
  - Each crate has its name, directory, edition, `rust_version`, kind, `async_runtimes`, profiles and features. It also has `mutually_exclusive_features`.
  - `tests` holds `diff_touches_tests` and the lists of test and non-test units. Code computes it from file roles and `#[cfg(test)]`/`#[test]` spans. It is a fact about the diff, not a Jev question.
- `flagged`: unit, dimension, question, signal and threshold, strongest first. Claude reads this first.
- `references`: the dimension and profile reference files to load.
- `units`: for each unit, `id`, `file`, `lines`, `changed_lines`, `role` and `status`. For each question: `dimension`, `primitive`, `answer`, `probabilities?`, `confidence?`, `signal`, `threshold` and `flagged`.
- `cargo_facts`: deterministic manifest and lockfile findings:
  - added, removed and changed dependencies;
  - git, path and wildcard sources;
  - removed features and changed default features;
  - edition and MSRV changes;
  - new build scripts and proc macros;
  - lockfile churn.
- `skipped`, `redactions`, `usage` (requests, input tokens, estimated USD) and `cargo` (suggested commands and a feature note).
- `payloads`, in a dry run only: the exact request bodies, without the auth header.

### `verify_rust_findings`

Input: `repo_path?`, `scope?`, `dry_run?` and `findings[1..=20]`. `scope` decides which revision of each file is read. Each finding has these fields:

- `id?`;
- `dimension`;
- `file`, `start_line` and `end_line`;
- `claim`: one sentence that names identifiers, not line numbers;
- `severity`.

The server re-reads the file itself. The state holds these parts:

- the enclosing item, with two marker columns. The first is `>` on claimed lines. The second is the diff marker (`+`, `-` or a space), so removed lines show the old side;
- the imports and the enclosing `impl` or `trait` header;
- any matching documentation facts (§4);
- the claim.

The proposed severity is never sent, so Jev judges severity independently.

The output for each finding:

- `support`: the Choice, with `supported` equal to `P(supported)`;
- `severity`: the Score, with the nearest level's name, the weighted level, `p_high_or_above`, probabilities and confidence;
- `severity_agrees`;
- `category`: the Choice;
- `verdict` (§6).

## 4. Jev question set

`src/questions.rs` holds every question and threshold as data. Each question has these fields:

- an id and a dimension;
- a primitive (Noul or Score);
- instructions and criteria;
- a lexical gate, excluded roles and an optional threshold.

Profiles also record the documentation they were `verified_against`. Gates compile once into `RegexSet`s. Code checks them through `QuestionSpec::applies(unit, role, text)`.

The rules come from Jev's jaggedness page:

- Instructions are literal, single-hop, and name `code`.
- Each question targets one defect.
- Noul criteria mirror the instruction.
- No question asks Jev to count, do arithmetic, give line numbers, or do anything a tool can answer.
- Each flag reads exactly one answer.

**Gate contract.** Gates run on the unit text exactly as sent to Jev. `context::render` puts a diff marker in the first column: `+`, `-` or a space. No marker may open a gate. Verification text adds a claim column in front. Question gates never see it, but fact gates do, so the tests cover both forms.

A pattern using `\s*` can reach across a line break into the next marker. `ARITHMETIC` did, and now uses `[ \t]*`.

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

Profile questions (17):

| Profile | Questions |
|---|---|
| tokio | `runtime_nesting`, `spawn_blocking_misuse`, `no_shutdown`, `select_not_cancel_safe`, `blocking_in_async`, `async_mutex_unneeded` |
| axum | `error_exposure`, `layer_order`, `extension_state`, `blocking_handler` |
| dioxus | `guard_across_await`, `read_write_overlap`, `effect_loop`, `hook_rules`, `stale_capture`, `server_fn_trust`, `untracked_dependency` |

**Documentation facts** live in `src/facts.rs`. Each is a short, literal fact from official docs. One example lists which futures are not cancellation safe in `tokio::select!`. A fact joins a request's `state` when its gate matches the code, up to five per request. TypeSafe's Models page recommends putting reference material in `state`. In the first eval, the `select!` facts raised Jev's support for a true cancellation claim from 0.17 to 0.74.

A separate `notes` field in the state marks the code as untrusted data. The code sits in its own JSON string field. The `prompt_injection` fixture tests this.

## 5. Units and chunking

- Only `.rs` files become code units. Each `Cargo.toml` diff becomes one manifest unit. `Cargo.lock` is reduced to deterministic facts.
- `syn` parses each file. Each non-blank changed line maps to its innermost enclosing item: a fn, a method or a top-level item. The unit is the union of those items.
- A line outside any item gets ±2 lines of context. If the file did not parse, it gets ±6.
- Code is shown diff-style without line numbers. The state adds the top-level `use` lines and the enclosing `impl` or `trait` header.
- Tokens are estimated as `ceil(bytes / 3)`. A unit over `max_unit_tokens` (6,000) is split into windows around its changes.
- An evaluation stops adding units at `max_total_tokens` (400,000, about $0.017) or `max_units` (60). The rest are listed under `skipped`.
- Up to 4 requests run at once, each with a 30 s timeout.
- Each request retries up to 3 times with jittered backoff from 0.5 s to 8 s. Retries apply to 408, 429 and 5xx responses (529 included), timeouts and connection errors.
- `Retry-After` and `retry-after-ms` are honoured, capped at 30 s.
- A 401 stops the remaining units. A failed unit does not fail the review.

## 6. Threshold and verification model

**Triage favours recall.** A Noul flags when `noul >= threshold`. A Score flags when the mass on its bad levels reaches the threshold. Per-dimension defaults:

| Threshold | Dimensions |
|---|---|
| 0.25 | unsafe, async, security, concurrency |
| 0.30 | correctness, error_handling, ffi |
| 0.35 | serde, api, macros, cargo |
| 0.45 | ownership, type_design, performance, testing |
| 0.60 | idiom |

Per-question overrides: `correctness.wildcard` 0.45, `async.sequential_awaits` 0.55 and `tokio.async_mutex_unneeded` 0.6.

**Verification favours precision.** It reads three answers, each on its own:

- `support` (Choice): `supported`, `refuted` or `insufficient_context`. The report bar applies to `P(supported)`. It is 0.70, or 0.80 for unsafe, idiom and type_design.
- `severity` (Score): levels run from 0 (low) to 3 (critical). `SEVERITY_HIGH_FROM` is 2, and `p_high_or_above` is the mass on high and critical.
- `category` (Choice): `real_defect`, `debatable_tradeoff` or `style_preference`.

The verdict is the first rule that matches:

1. `dismiss` if `support` chose `refuted`. Also `dismiss` if `category` chose `style_preference` with confidence of at least `STYLE_DISMISS_MIN_CONFIDENCE` (0.50). A narrower style win falls through to the later rules.
2. `report` if `P(supported)` reaches the report bar and the category is `real_defect`. A `debatable_tradeoff` also counts if `P(real_defect)` is at least `TRADEOFF_REAL_DEFECT_BAR` (0.40).
3. `insufficient_context` if `support` chose `insufficient_context`. **This is not a refutation.** Cross-file findings, such as lock ordering or semver breaks, land here. The skill keeps them when Claude's own confidence is High. It says Jev could not verify them from local context.
4. `dismiss` if `P(supported)` is below 0.40.
5. `uncertain` otherwise. The skill reports these only with deterministic evidence.

Both named bars live in `src/questions.rs` with the other thresholds. Environment variables override the report and dismiss bars (see the README). The bars are starting points tuned against the eval corpus. Retune them only with eval evidence.

## 7. Distribution

`.mcp.json` runs `sh ${CLAUDE_PLUGIN_ROOT}/scripts/launch.sh`. The launcher runs the first binary that reports the plugin's version:

1. `JEV_RUST_REVIEW_BIN`;
2. `${CLAUDE_PLUGIN_ROOT}/target/release/jev-rust-review`, for local development. When it is newer than the cached copy, it replaces it. A rebuild at the same version then takes effect;
3. the cached `${CLAUDE_PLUGIN_DATA}/bin/<version>/jev-rust-review`;
4. `jev-rust-review` on `PATH`, for example from `cargo install`;
5. a prebuilt release asset for the host triple, verified against the release's `SHA256SUMS` before it is cached;
6. a local `cargo build --release --locked`, detached with `setsid` or `nohup`.

Step 5's checksums come from the same release as the binary. They prove integrity, not authenticity.

Step 6 waits up to 20 s. If the build is still running, the launcher exits with one message. The message points at the build log and tells the user to reconnect via `/mcp`. The next launch finds the cached binary. `launch.sh --install` runs the same steps in the foreground.

**Profiles.** The `release` profile builds quickly (opt-level 2, no LTO), because step 6 compiles it on the user's machine. The `dist` profile (thin LTO, one codegen unit) is for the binaries CI ships. The release workflow builds `--profile dist` for these targets:

- x86_64 and aarch64 Linux (musl);
- x86_64 and aarch64 macOS;
- x86_64 Windows.

It collects each binary from `target/<triple>/dist/`.

**Build time.** A cold `cargo build --release` took 75 s with aws-lc-rs, the default rustls provider. aws-lc-sys spent 40 s of that compiling C. With `ring`, the build took 65 s. Neither fits the 30 s window. So step 5 comes first, and step 6 never blocks startup. Switching to `ring` would save about 10 s, which is not worth moving off reqwest's default provider.

`scripts/test-install.sh` tests both paths on every CI run. It builds from source with an empty data dir, installs a verified download, and refuses a tampered checksum. It also checks that a rebuilt local binary replaces the cached one.

## 8. Security and privacy

- The key comes only from `TYPESAFE_API_KEY`, or from the plugin's `userConfig` (`sensitive: true`) mapped to `JEV_RUST_REVIEW_API_KEY`. It is never logged. `Config`'s `Debug` output masks it.
- The client refuses any Jev endpoint that is not https, except on localhost. It does not follow redirects, because a 307 or 308 would re-post the key and the code elsewhere.
- The server never enumerates the process environment. `std::env::vars` is a disallowed method.
- The server skips secret-bearing files by name: `.env*`, `*.pem`, `*.key`, `*.p12`, `*.pfx`, `id_rsa*`, `id_ed25519*`, `*credential*` and `*secret*`.
- It redacts token formats and high-entropy secret assignments line by line, and reports the counts.
- stdout carries only JSON-RPC:
  - `print!` and `println!` are disallowed macros, and `#![deny(clippy::print_stdout)]` is set;
  - a test scans `src/` for stdout writes;
  - the smoke test checks that every stdout line is JSON-RPC.

## 9. Progress checklist

- [x] Preflight, private repo created
- [x] Research (TypeSafe, Claude Code plugins, rmcp, Dioxus/Axum/Tokio, Codex)
- [x] Core modules, MCP server, dry run
- [x] Unit, HTTP-mock, pipeline, stdout-guard and plugin-consistency tests
- [x] Plugin: plugin.json, marketplace.json, .mcp.json, skill, references, agent
- [x] launch.sh, fresh-install test, smoke.sh
- [x] Fixture corpus (11 buggy, 7 clean), recordings, offline replay
- [x] Adopt the redesigned questions.rs, Cargo.toml and clippy.toml; new verification model
- [x] Live eval on the new question ids
- [x] CI and release workflows
- [x] README
- [x] Headless Claude Code session check (2026-09-19, on the kiln repo: server connected, both tools listed, full skill run)
- [x] Secret scan of history; repo made public (2026-09-19)
- [x] CI green on GitHub (run 35415756948)
- [ ] First tagged release (prebuilt binaries)

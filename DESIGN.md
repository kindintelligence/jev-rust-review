# jev-rust-review design

This file records the v0.1 design decisions and the reasons for them. Keep it short.

```text
rustc / clippy / cargo-semver-checks = deterministic facts   (this MCP server runs them and filters to the change)
Jev                                  = typed semantic judgment (this MCP server: triage + verification)
Claude                               = reasoning, root cause, fix (skill + rust-reviewer agent)
```

**The standard: go deeper than the compiler.** A Rust developer already gets excellent feedback from rustc, Clippy and cargo. A review that repeats it is noise and costs trust. So tools answer what tools can answer (§3a), Jev is asked only what no tool answers (§4), and a defect a tool reported is never reported a second time (§6). Whether Jev earns its place on top of Claude alone is measured, not assumed (§10).

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
| Library behaviour stated as facts (2026-09-21) | docs.rs: tokio `sync::Notify`, `sync::mpsc::Sender::send`, `task::JoinHandle`, `io::BufWriter`; axum `Router::layer` and `Router::route_layer`; std `io::BufWriter`, `Iterator::size_hint` | Each fact in `src/facts.rs` restates its page. `notify_waiters` stores no permit, and a `Notified` future receives wakeups from the moment it is created. A cancelled `send` drops its message. Both layer calls cover only routes that already exist |
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
10. **Clippy owns undocumented `unsafe`.** `clippy::undocumented_unsafe_blocks` and `clippy::missing_safety_doc` answer it deterministically. There is no Jev question for it. The server runs Clippy with the first switched on; the second is on by default.
11. **A source build cannot fit the MCP startup window.** A cold `cargo build --release` took 75 s on 8 cores, with crates already downloaded. aws-lc-sys alone took 40 s. With the `ring` provider it took 65 s. The startup timeout is 30 s, so the launcher never blocks on a cold build (§7).

## 3. MCP tools

The server is named `jev` in `.mcp.json`. It does its own git work, and it validates every scope. One audited function spawns git. It passes an argument vector with no shell and uses `--` separators. It also resolves revs to SHAs with `git rev-parse --verify --end-of-options` before use. A second audited function spawns cargo (§3a).

The skill calls the three tools in this order: `cargo_diagnostics`, `evaluate_rust_changes`, `verify_rust_findings`.

### `cargo_diagnostics`

Input: `repo_path?` and `scope?`. §3a describes what it runs and how it filters. The output is compact JSON text:

- `status`: `ok`, `disabled`, `skipped`, `failed` or `timeout`, with a `reason`.
- `command`, `build_ok`, and `extra_lints`, the off-by-default lints this run switched on.
- `diagnostics`: errors anywhere and warnings on changed lines. Each has `file`, `lines`, `level`, `code`, `message`, `related` and `on_changed_lines`.
- `warnings_outside_change` and `silenced_by_project_policy`: counts only.
- `semver`: the cargo-semver-checks verdict, when a library's `pub` surface changed.
- `advice`: one-line suggestions for tools the server does not run. Miri is suggested whenever `unsafe` changed.

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
- `flagged`: unit, dimension, question, `check`, signal and threshold, strongest first. `check` is the question itself, reworded to be about the unit. A flag is a question for the agent to answer by reading the code, and `reading_flags` says so in the output, because a smaller model reported a bare flag as a finding (§10). `reading_flags` also says that flags decide what to read first and that every changed unit is reviewed. A smaller model reported nothing whenever nothing was flagged (§10).
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
- `related_code`: the definitions of items the claim names that lie outside the excerpt. The server takes every word of the claim that could be an item name, searches the new side's `.rs` files with one `git grep` for `fn`, `struct`, `enum`, `trait`, `type`, `const` or `static` followed by one of them, and sends the innermost item around each hit. At most 4 definitions, 40 lines each, 1,500 tokens in all. The pattern is built from `[A-Za-z0-9_]` names only;
- the claim.

The result also carries `unseen`: names the claim writes as code (backticked, a path, snake case, a call, camel case) that appear nowhere in what Jev was shown.

The proposed severity is never sent, so Jev judges severity independently.

The output for each finding:

- `support`: the Choice, with `supported` equal to `P(supported)`;
- `severity`: the Score, with the nearest level's name, the weighted level, `p_high_or_above`, probabilities and confidence;
- `severity_agrees`;
- `category`: the Choice;
- `verdict` (§6).

Fact gates read `related_code` as well as the excerpt, because the call whose documented behaviour decides a claim is often in a helper.

## 3a. Tools answer what tools can answer

**What runs.** One `cargo clippy --all-targets --message-format=json`. Clippy runs rustc, so this one build yields every compiler error and warning, Clippy's default lints, and the extra lints below. If Clippy is not installed the server falls back to `cargo check` and says so. `cargo test` stays with Claude, through Bash: a failing test has no line to filter by.

| Extra lint (off by default) | Group | The defect it owns |
|---|---|---|
| `cast_possible_truncation`, `cast_sign_loss`, `cast_possible_wrap` | pedantic | every lossy `as` cast |
| `needless_pass_by_value` | pedantic | an owned argument that is only read |
| `redundant_clone` | nursery | a clone whose original is never used again |
| `non_send_fields_in_send_ty` | nursery | `unsafe impl Send` over a field that is not `Send` |
| `let_underscore_must_use` | restriction | `let _ =` on a `Result` |
| `map_err_ignore` | restriction | `.map_err(\|_\| ..)` |
| `wildcard_enum_match_arm` | restriction | a `_` arm on an enum |
| `undocumented_unsafe_blocks` | restriction | `unsafe` with no `// SAFETY:` |

Clippy's defaults already cover a guard held across `.await` (`await_holding_lock`, `await_holding_refcell_ref`), `let _ = lock()` (`let_underscore_lock`), needless `to_owned` and a regex built in a loop. The `lints_exist_in_installed_clippy` test checks every name, level and group in this file and in `questions.rs` against the installed toolchain (Clippy 0.1.97 on 2026-09-19).

**Who runs cargo: the server.** The alternative was Claude running cargo through Bash and handing the output to a filter. JSON output would put a wall of text in Claude's context, which is what this design exists to avoid. Redirecting it to a file needs shell permissions that a headless session does not have. The server running cargo and consuming its JSON is how rust-analyzer's flycheck and reviewdog work, so that is the shape used here. The cost is a second subprocess door. It lives in `cargo_tools.rs`, builds every argument itself, uses no shell and no stdin, and pipes stdout so a tool can never write into the MCP transport. `JEV_RUST_REVIEW_CARGO=0` turns it off, and the skill's `--no-cargo` skips the call. When `JEV_RUST_REVIEW_CARGO_TARGET_DIR` is set, the server sets `CARGO_BUILD_BUILD_DIR` to the same place. A templated `build.build-dir` in the user's cargo config otherwise gives every repository path its own build directory. The eval's temporary repositories filled a disk that way.

**Filtering is code.** The server computes the changed line ranges of each file from the diff it already parses. A diagnostic touches the change when **any** of its spans overlaps a changed range: the primary span, a secondary label, or the span of a note. The first eval run showed why: `await_holding_lock` puts the unchanged guard binding in its primary span and the newly added `.await` in a note. A span inside a macro from another crate is walked out to its call site. The rules:

- an **error** is kept wherever it is, because a change can break code it did not touch;
- a **warning** is kept only when it touches a changed line; the rest are counted;
- the same message from a second target (lib and lib-test) is kept once.

**Project policy wins.** A `#[allow]` attribute beats a command-line `-W`, so rustc handles it. A `[lints.clippy]` table does **not**: measured on cargo 1.97, `-W` overrides `lint = "allow"` in `Cargo.toml`. So the server reads `[lints.clippy]`, the inherited `[workspace.lints.clippy]`, and any `-A clippy::x` in `.cargo/config.toml` rustflags. A lint the project names at any level is not passed for that crate. A diagnostic for a lint the crate allows, by name or through its group, is dropped and counted. `project_policy_wins_in_the_manifest_and_in_code` proves all three forms against real Clippy.

**cargo-semver-checks.** It runs when a changed line in a library crate opens the `api.breaking_change` gate, and only if `cargo semver-checks --version` succeeds. The server never installs anything. The baseline is `--baseline-rev <old commit of the scope>`: `HEAD` for uncommitted changes, the parent for a single commit, the left side or the merge base for a range. Its text report is parsed into `breaks`, and its verdict is fact: `breaking` entries are reported as they are, and `compatible` means no API finding is raised. When it is absent the output says so in one line and the Jev question stands in.

**Miri** is suggested in `advice` whenever a changed line contains `unsafe`. It is never run: it needs nightly, and it only finds UB on executions a test reaches.

## 4. Jev question set

`src/questions.rs` holds every question and threshold as data. Each question has these fields:

- an id and a dimension;
- a primitive (Noul or Score);
- instructions and criteria;
- a lexical gate, excluded roles and an optional threshold;
- `beyond_tooling`: one required sentence saying why no compiler check, lint or cargo tool answers the question. `every_question_goes_beyond_tooling` fails when it is empty;
- `tool_overlap`: the lints and tools that report part of the same defect, by the name they print.

Profiles also record the documentation they were `verified_against`. Gates compile once into `RegexSet`s. Code checks them through `QuestionSpec::applies(unit, role, text)`.

The rules come from Jev's jaggedness page:

- Instructions are literal, single-hop, and name `code`.
- Each question targets one defect.
- Noul criteria mirror the instruction.
- No question asks Jev to count, do arithmetic or give line numbers.
- No question asks what a tool answers. Where a tool finds the pattern, the question keeps only the judgement. Clippy finds the lossy cast; Jev is asked whether the value can realistically be out of range.
- Each flag reads exactly one answer.

**Gate contract.** Gates run on the unit text exactly as sent to Jev. `context::render` puts a diff marker in the first column: `+`, `-` or a space. No marker may open a gate. Verification text adds a claim column in front. Question gates never see it, but fact gates do, so the tests cover both forms.

A pattern using `\s*` can reach across a line break into the next marker. `ARITHMETIC` did, and now uses `[ \t]*`.

**What the tools took over (2026-09-19).** Four questions were deleted because a tool states the whole defect, and seven were narrowed to the part that needs judgement.

| Question | Change | The tool that owns the rest |
|---|---|---|
| `async.guard_across_await` | deleted | `await_holding_lock`, `await_holding_refcell_ref` (default) |
| `ownership.signature` | deleted | `needless_pass_by_value` |
| `correctness.wildcard` | deleted | `wildcard_enum_match_arm` |
| `cargo.unpinned_source` | deleted | `cargo_facts`, computed in code |
| `correctness.cast` | narrowed to "can the value be out of range" | the three cast lints |
| `correctness.drop_order` | narrowed to drop points the code relies on | `let_underscore_lock` |
| `ownership.clone` | narrowed to large or repeated copies that are used | `redundant_clone`, `unnecessary_to_owned` |
| `error_handling.swallowed` | narrowed to failures turned into a success | `unused_must_use`, `let_underscore_must_use` |
| `error_handling.lossy` | narrowed to errors that are read and then flattened | `map_err_ignore` |
| `concurrency.unsafe_send_sync` | narrowed to `Sync` and unsynchronised raw pointers | `non_send_fields_in_send_ty` |
| `performance.repeated_work` | no longer asks about a regex in a loop | `regex_creation_in_loops` |

`api.breaking_change` stays as the fallback for machines without cargo-semver-checks. `ffi.unwind` was reworded: since Rust 1.81 a panic in an `extern "C"` function aborts the process.

`questions::TOOL_OWNED` lists the lints of the deleted questions with their dimension, so that a finding about a guard across `.await` is still recognised as a repeat of `await_holding_lock`.

Core questions (42):

| Dimension | Questions |
|---|---|
| correctness | `logic`, `bounds`, `cast`, `overflow`, `drop_order` |
| ownership | `clone` |
| type_design | `loose_types` |
| error_handling | `swallowed`, `panic`, `lossy`, `drop_panic` |
| async | `blocking_call`, `select_cancellation`, `detached_task`, `unbounded`, `sequential_awaits` |
| concurrency | `check_then_act`, `lost_wakeup`, `atomics`, `unsafe_send_sync`, `lock_scope`, `lock_order` |
| unsafe | `memory_access`, `aliasing`, `transmute` |
| ffi | `ownership`, `pointers_and_strings`, `unwind` |
| performance | `repeated_work` |
| idiom | `clarity` (Score) |
| api | `breaking_change` |
| macros | `double_evaluation`, `name_collision` |
| serde | `compatibility`, `silent_default` |
| security | `injection`, `secret_exposure`, `tls_verification`, `weak_randomness`, `unbounded_input` |
| testing | `weak_assertion` |
| cargo (manifest units) | `manifest_risk` |

Profile questions (18):

| Profile | Questions |
|---|---|
| tokio | `runtime_nesting`, `spawn_blocking_misuse`, `no_shutdown`, `select_not_cancel_safe`, `blocking_in_async`, `async_mutex_unneeded` |
| axum | `error_exposure`, `layer_order`, `route_after_layer`, `extension_state`, `blocking_handler` |
| dioxus | `guard_across_await`, `read_write_overlap`, `effect_loop`, `hook_rules`, `stale_capture`, `server_fn_trust`, `untracked_dependency` |

**Documentation facts** live in `src/facts.rs`. Each is a short, literal fact from official docs. One example lists which futures are not cancellation safe in `tokio::select!`. A fact joins a request's `state` when its gate matches the code, up to five per request. TypeSafe's Models page recommends putting reference material in `state`. In the first eval, the `select!` facts raised Jev's support for a true cancellation claim from 0.17 to 0.74.

A separate `notes` field in the state marks the code as untrusted data. The code sits in its own JSON string field. The `prompt_injection` fixture tests this.

## 5. Units and chunking

- Only `.rs` files become code units. Each `Cargo.toml` diff becomes one manifest unit. `Cargo.lock` is reduced to deterministic facts.
- `syn` parses each file. Each non-blank changed line maps to its innermost enclosing item: a fn, a method or a top-level item. **Each changed item is one unit.** Until 2026-09-20 neighbouring items merged, so a tidy-up of 30 functions in 3 files came out as 3 units, all flagged, and triage had nothing to narrow. Only changed lines outside any item still merge with each other.
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

Per-question overrides: `async.sequential_awaits` 0.55 and `tokio.async_mutex_unneeded` 0.6.

**No defect is reported twice.** `cargo_diagnostics` remembers its result for the (repository, scope) it ran on, in the server process. Both later stages read it:

- Triage moves a flag to `tool_covered` when one of the question's `tool_overlap` lints fired on the unit's changed lines, or when cargo-semver-checks gave its verdict on the crate and the question lists it.
- Verification returns `tool_reported`, without asking Jev, when a lint that overlaps the finding's dimension fired on the finding's lines. This check is code, so it also runs with no API key: the Claude-only path deduplicates too.

A finding names a dimension, not a question, so verification uses every lint of the dimension (`tool_overlap_for`). If the server restarted between the calls there is nothing remembered and nothing is deduplicated; `tool_diagnostics_seen` says which.

**Verification favours precision.** It reads three answers, each on its own:

- `support` (Choice): `supported`, `refuted` or `insufficient_context`. The report bar applies to `P(supported)`. It is 0.70, or 0.80 for unsafe, idiom and type_design.
- `severity` (Score): levels run from 0 (low) to 3 (critical). `SEVERITY_HIGH_FROM` is 2, and `p_high_or_above` is the mass on high and critical.
- `category` (Choice): `real_defect`, `remote_risk`, `debatable_tradeoff` or `style_preference`. `remote_risk` is a claim that is true only under a condition the code gives no reason to expect: a lock poisoned by an earlier panic, a sum of ordinary counts overflowing, a local file too large for memory.

The verdict is the first rule that matches:

0. `tool_reported` if a tool already reported the defect on these lines (above). Jev is not asked.
1. `dismiss` if `support` chose `refuted`. Also `dismiss` if `category` chose `style_preference` with confidence of at least `STYLE_DISMISS_MIN_CONFIDENCE` (0.50). A narrower style win falls through to the later rules.
2. `not_material` if `category` chose `remote_risk` with confidence of at least `REMOTE_RISK_MIN_CONFIDENCE` (0.50). The claim is true and not worth the author's time. The skill leaves it out of the report unless the agent can name the realistic input that reaches the failure.
3. `report` if `P(supported)` reaches the report bar and the category is `real_defect`. A `debatable_tradeoff`, or a narrow `remote_risk`, also counts if `P(real_defect)` is at least `TRADEOFF_REAL_DEFECT_BAR` (0.40).
4. `insufficient_context` if `support` chose `insufficient_context`. **This is not a refutation.** Cross-file findings, such as lock ordering or semver breaks, land here. The skill keeps them when Claude's own confidence is High. It says Jev could not verify them from local context.
5. `dismiss` if `P(supported)` is below 0.40.
6. `uncertain` otherwise.

**Jev cannot refute what it was not shown.** When `unseen` is not empty, a claim Jev did not confirm is `insufficient_context`: a `refuted` choice, a low `P(supported)` and `uncertain` all map to it. Agreement still reports. A confident `style_preference` still dismisses and a confident `remote_risk` is still `not_material`, because both judge the claim and not the code.

**A verdict is a second opinion, not a gate.** Only `tool_reported` removes a finding, because that check is code. Until 2026-09-20 the skill dropped a finding on `dismiss`, and on `uncertain` without deterministic evidence. The first three-mode run (§10) showed the cost: in 14 of the 15 runs where the full pipeline missed the seeded bug, Claude had found it and Jev had not confirmed it. Each of those bugs depends on something outside the excerpt Jev reads: another file, or the documented behaviour of a library. The skill now treats `uncertain` and `dismiss` as a reason to re-read the code for what Jev may have seen. The agent drops the finding if it finds that, and keeps it, with Jev's number shown, if it can still state the concrete failure with High confidence.

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
- Two functions spawn processes, and Clippy's `disallowed_methods` forbids any other: `git.rs` for git and `cargo_tools.rs` for cargo. Neither uses a shell. cargo runs the project's build scripts and proc macros, which is what Claude running `cargo check` through Bash did before; the tool description and the skill both say to ask first in an untrusted repository.
- The server never enumerates the process environment. `std::env::vars` is a disallowed method.
- The server skips secret-bearing files by name: `.env*`, `*.pem`, `*.key`, `*.p12`, `*.pfx`, `id_rsa*`, `id_ed25519*`, `*credential*` and `*secret*`.
- It redacts token formats and high-entropy secret assignments line by line, and reports the counts.
- stdout carries only JSON-RPC:
  - `print!` and `println!` are disallowed macros, and `#![deny(clippy::print_stdout)]` is set;
  - a test scans `src/` for stdout writes;
  - the smoke test checks that every stdout line is JSON-RPC.

## 9. Three-mode eval

`tests/e2e.rs` runs the real product headless (`claude -p --plugin-dir`) over the fixture corpus in three modes:

| Mode | What runs |
|---|---|
| T | tools only: `cargo_diagnostics` in process, no model |
| C | Claude only: the plugin with no TypeSafe key, so the `jev_unavailable` path, same skill and references |
| J | the full pipeline with Jev |

**A bug counts towards usefulness only if mode T misses it.** Each fixture is two snapshots of a small buildable crate, labelled `tool_catches` in `fixture.toml`. The harness checks the label against what the tools really say and prints any mismatch. A bug the tools catch is not run in C or J at all.

**Grading is code.** The skill's `--json` flag ends the report with a machine-readable list of what it reported. A run finds the bug when a reported entry overlaps the fixture's claim range, in the claim's file, in an expected dimension. Every entry on a clean fixture is a false positive. Every other entry on a buggy fixture is listed for a person to judge. Tool facts that the report repeats count like any other entry.

**The corpus** (2026-09-19): 23 buggy and 7 clean fixtures. Tools catch 4 of the 23 (`guard_across_await`, `truncating_cast`, `lossy_error`, `semver_break`). Ten fixtures are new and harder: they span functions or files, and seven are modelled on real bugs or documented behaviour, with the source cited in `fixture.toml` (RUSTSEC-2021-0003, CVE-2018-1000810, CVE-2022-21658, and documented tokio, axum and std behaviour). All are fresh minimal reproductions. Two are noisy tidy-up diffs of 25 and 30 changed functions with one seeded bug each. Before any C or J run, five older fixtures were edited so that each carries exactly one defect and no incidental tool warning; `swallowed_result` lost its `let _ =`, which Clippy reports, and keeps the `.ok()`, which it does not.

**The matrix.** Three runs per cell for C and J on 20 fixtures is 120 headless runs, the budget for one matrix. The first matrix used it. The owner then asked for a re-run after the change in §10, and for a smaller model, which is two more matrices. The 20 are the 12 new fixtures, 4 older bugs (`select_cancellation`, `check_then_act`, `unsound_unsafe`, `prompt_injection`) and 4 clean fixtures (`arc_clone`, `scoped_lock_before_await`, `sound_unsafe`, `explained_expect`). The model is `claude-sonnet-5` for both modes. Harness shakedown runs are kept as run 1 when nothing changed afterwards.

## 10. How Jev is judged

**Jev is a tool the coding agent uses. It is not an alternative to the agent.** It sits between the model and the software: typed, fast, and close to free. The 60 full-pipeline runs of the first matrix used $0.0075 of Jev against $8.19 of Claude. At that price the question is never "Jev or Claude". It is how an agent should use a cheap second reader, and whether a review with it is better than the same review without it. That matters most for a model that is smaller or less thorough than the one this was built with, so the eval runs on two models.

**First rules, first result (2026-09-19).** The first version of this section set Jev against Claude: Jev triage would stop being the default unless mode J beat mode C on bugs found or false positives, and verification would be cut if it removed more seeded bugs than other candidates. Those rules were committed before any Claude run (`cc48377`). The run, on `claude-sonnet-5`, three runs per cell:

| Mode | Beyond-tooling bugs found | Entries on clean fixtures | Claude cost | Jev cost |
|---|---|---|---|---|
| C, Claude only | 45/48 | 0 in 12 runs | $6.97 | none |
| J, full pipeline, verification as a gate | 33/48 | 1 in 12 runs | $8.19 | $0.0075 |

By those rules both stages lost. The project owner rejected the framing, not the numbers: a contest between two modes is the wrong test of a tool that is meant to work with the agent. The numbers still say two useful things, and both are about how the agent used Jev:

- **Verification as a gate overruled correct findings.** Of J's 15 misses, 14 had Claude's candidate on the seeded bug come back `uncertain` or `dismiss`, and the skill told Claude to drop it. The fifteenth was reported under another dimension, which the grader does not count.
- **Triage as a pointer found a bug Claude alone missed.** On `symlink_check_then_delete`, C reported a different defect in the same function in all three runs and never the symlink race. J, with a concurrency flag on the unit, reported the race in all three.

This corpus cannot show a saving in Claude tokens from triage. Every fixture is one or two units, except the noisy diffs at 3 and 5, because the unit builder merges adjacent changed functions. There was nothing for triage to skip.

**What changed, and why (2026-09-20).** One thing: the skill, the agent prompt and the tool descriptions now treat a Jev verdict as a second opinion (§6). No threshold, question or fixture changed. This is a change made after seeing results, so every Claude mode is run again, and the first result stays published beside the new one.

**Rules for the second run,** written before it:

1. **Floor.** With Jev, the review must not find fewer beyond-tooling bugs than without it, and must not report more entries on clean fixtures. The noise margins are the same as before: 3 on bugs found (of 48), 2 on entries.
2. **Added value.** Jev earns its default place if, above that floor, it finds seeded bugs the same model misses without it: at least 3 more of 48, on either model. A gain on the smaller model counts as much as one on the larger.
3. **If it only meets the floor,** Jev stays on by default, because it costs about a hundredth of a cent per review, and the README says plainly that no gain was measured.
4. **If it fails the floor on either model,** the README says so, and the next step is to find which stage caused it from the per-run verdicts, as was done here.
5. **Cost and wall time** are reported per mode and per model. They are reported, not judged: Jev's share is below 0.1% of a review's cost.
6. **Publish** every matrix, including the first one, with date, models, run counts and cost.

Models: `claude-sonnet-5` and `claude-haiku-4-5-20251001`. Same 20 fixtures, three runs per cell, modes C and J. Mode T does not depend on the model.

**Second result (2026-09-20).** Three runs per cell. The README has the full table and `eval/2026-09-20-three-mode.md` the raw reports.

| Model | Without Jev | With Jev | By the rules |
|---|---|---|---|
| `claude-sonnet-5` | 45/48 bugs, 2 entries on clean fixtures | 46/48 bugs, 1 entry | Meets the floor (rule 1). No added value by rule 2's margin, so rule 3 applies: Jev stays on and the README says no gain was measured. |
| `claude-haiku-4-5` | 25/34 bugs on runs graded in both modes, 5 other entries | 20/34 bugs, 21 other entries | Fails the floor (rule 4). |

Rule 4 asks which stage caused the failure. Both did, because the smaller model treats Jev's output as instruction:

- **Triage.** In 7 of Haiku's 17 misses with Jev it never raised the seeded bug. Its extra entries sit where triage flagged: poisoned-mutex unwraps under `error_handling.panic`, `u32` sums under `correctness.overflow`. A flag meant as "look here" was reported as a finding.
- **Verification.** In 6 of the 17 it raised the bug and Jev answered `dismiss`. The second-opinion wording that fixed this on Sonnet did not hold on Haiku.

Haiku also could not drive the tools reliably: 21 of 120 runs never got a result from `evaluate_rust_changes`, mostly because it sent malformed JSON for an empty `scope`.

Not yet done, and the owner's call: make the server's output safe for a model that obeys it. The candidates are to return flags as questions to check and not as labels, to stop returning `dismiss` for a claim whose evidence is outside the excerpt, to give verification the definitions the claim names, to accept an absent or empty `scope` however it is spelt, and to make one unit per changed function. Each is a change after results, so each needs a re-run.

**Third change, for a smaller model (2026-09-20).** The owner asked for four changes and a re-run on Haiku. They are the first four candidates above:

| Change | Where |
|---|---|
| A flag is returned as a question to check (`check`, `reading_flags`), and the skill says a restated flag is not a finding | §3, skill step 3 |
| Verification is shown the definitions the claim names, and never returns `dismiss` or `uncertain` while the claim names code it was not shown | §3, §6 |
| `scope` may be absent, `null`, any non-string, or any of a dozen spellings of "none"; the skill says to call the tools with `{}` and never through a shell | `mcp.rs`, `git.rs`, skill step 1 |
| One unit per changed item | §5 |

Measured on Jev's two stages alone, with ideal claims and before any Claude run: true claims verified as `report` went from 12 of 19 to 16 of 19, with 0 of 7 bait claims reported in both. `lock_order_inversion` went from 0.21 to 0.93 supported, `length_guard_weakened` from 0.45 to 0.85, and `size_hint_trusted` from 0.50 to 0.90. The three still unconfirmed (`notify_lost_wakeup`, `route_added_after_layer`, `select_drops_send`) rest on documented library behaviour, which no definition in the repository shows. Triage recall stayed at 14 of 19.

The rules for judging the Haiku re-run are rules 1 to 6 above, unchanged. The re-run covers modes C and J, because the unit and skill changes reach both.

**Third result (2026-09-20, `claude-haiku-4-5`, three runs per cell).**

| | Without Jev | With Jev |
|---|---|---|
| Before the four changes (matrix 3), runs graded in both modes | 25/34 bugs, 5 other entries | 20/34 bugs, 21 other entries |
| After (matrix 4), runs graded in both modes | 26/47 bugs, 15 other entries | 30/47 bugs, 34 other entries |
| Entries on clean fixtures, after | 2 in 12 runs | 2 in 11 runs |
| Runs that could not be graded, of 120 | 21 before | 2 after |

By the rules: Jev meets the floor on Haiku (rule 1) and adds 4 bugs of 47, above the margin of 3 (rule 2). So Jev keeps its default place on the evidence of the smaller model. What each change did:

- **One unit per item** is where the gain is. On the two large diffs Haiku found the bug in 3 of 6 runs alone and 6 of 6 with Jev, which flagged 7 of 31 and 7 of 25 units.
- **Lenient `scope` and the tool-calling paragraph** took ungraded runs from 21 to 2.
- **Related definitions** lifted Jev's own verification (16 of 19 true claims, from 12). In the headless runs Jev still answered `dismiss` on 11 seeded candidates, against 12 before.
- **Flags as questions** did not stop the noise. Other entries rose from 15 to 34 with Jev. Many restate a flag, and Jev's verification confirms them because they are true and trivial.

Left open by that run: `notify_lost_wakeup` (0 of 3 with Jev, 3 of 3 without) and `route_added_after_layer`; verification had no notion of "true but not worth reporting"; a flag drew Haiku off the seeded bug on `error_flattened_to_string`; and Sonnet 5 had not been re-run. The next part covers all four.

**Fourth change: the open items (2026-09-21).** The owner asked for all four to be fixed. Reading the matrix 4 cells first corrected the diagnosis of the first one.

| Open item | What the cells showed | Change |
|---|---|---|
| Two bugs lost to library behaviour | The cause was not mainly missing facts. In all 9 Haiku runs where triage flagged nothing (`notify_lost_wakeup`, `route_added_after_layer`, `bufwriter_never_flushed`), the review reported nothing. In 7 of them it never raised a candidate, and in the other 2 Jev answered `dismiss`. Step 5 of the skill said "inspect flagged code", so no flag meant no reading | Flags decide what is read first. The skill, the agent prompt and `reading_flags` say to review every changed unit, flagged first (§3, skill steps 3 and 5) |
| The same two bugs, in Jev's own stages | Verification answered `dismiss` and `uncertain` on true claims about `notify_waiters`, `route_layer` and `Sender::send` | Six facts from the tokio, axum and std documentation (§1, §4). Fact gates read `related_code`. Two questions for defect classes that had none: `concurrency.lost_wakeup` and `axum.route_after_layer` |
| Noise: true but trivial entries | Verification confirmed "unwrap on a poisoned mutex can panic" because it is true | A `remote_risk` category and a `not_material` verdict (§6). `error_handling.panic`, `correctness.overflow` and `security.unbounded_input` say what does not count. Three clean fixtures carry true but trivial bait claims |
| A flag pulls the model to another claim | On `error_flattened_to_string` only `security.unbounded_input` flagged. `error_handling.lossy` answered 0.18 on the textbook case, because its wording let "the text survives" count as kept | `lossy` now says that formatting an error into a `String` loses its type, kind and source. The skill says a flag is one question about a unit, and the unit can hold a different defect |
| `lock_order_inversion` lost with Jev (0 of 3 against 3 of 3, first pass of this re-run) | No question asked about lock order. No concurrency gate opened, because `Mutex` is declared in another file. The `correctness` and `error_handling` flags on the unit drew Haiku away | `concurrency.lock_order` asks whether a lock is taken while another is held. Jev cannot compare orders across units, so the flag sends the reviewer to the other lock sites. `.lock()` opens the gate alone |
| Two fixtures were not what they claimed | `explained_expect` (clean) changed which identifiers match, and both models said so. `symlink_check_then_delete` carried a second bug, a kept `.lock` file that makes `remove_dir` fail | Both fixed: one defect, or none, per fixture. With the `.lock` skip gone, nothing opened `check_then_act` on the symlink race, so its gate now opens on filesystem checks (`metadata(`, `is_file()`, `is_dir()`, `is_symlink()`) |
| Haiku invents dimension names (`logic`, `access_control`), which the grader cannot count | Seen in the list for a person to judge | The skill lists the 16 names, and a test keeps the list equal to `Dimension::ALL` |

**This is tuning on known fixtures, and it is recorded as such.** The facts and the two questions were written from the library documentation, and they cover more than the fixtures need (`JoinHandle`, `BufWriter`, `size_hint`). They were still written after seeing which fixtures failed. So the gain on `notify_lost_wakeup`, `route_added_after_layer` and `select_drops_send` shows that the mechanism works. I have not measured how often Jev knows the library behaviour behind a bug it has not seen. The review-every-unit change and `not_material` do not depend on any fixture.

Measured on Jev's two stages alone, before any Claude run, on 19 bugs and 10 clean fixtures:

| | Before | After |
|---|---|---|
| Triage flagged the bug in an expected dimension | 14/19 | 19/19 |
| True claims verified as `report` | 16/19 | 19/19 |
| Bait claims verified as `report` | 0/7 | 0/10 |
| True but trivial bait claims judged `not_material` | no such verdict | 3/3 |

The rules for judging the re-runs are rules 1 to 6 above, unchanged. Both models are run again in modes C and J, because the skill changes reach both modes.

After the runs, the new sentences in the skill, the agent prompt and `reading_flags` were split to fit the house writing style. Their meaning did not change, and no run was repeated for it.

**Fourth result (2026-09-21, three runs per cell, all 240 runs graded).** `eval/2026-09-21-three-mode.md` has the raw reports.

| Model | | Without Jev | With Jev |
|---|---|---|---|
| `claude-sonnet-5` | Beyond-tooling bugs found | 45/48 | 47/48 |
| | Entries on clean fixtures | 0 in 12 runs | 0 in 12 runs |
| | Other entries on buggy fixtures | 1 | 1 |
| `claude-haiku-4-5` | Beyond-tooling bugs found | 36/48 | 43/48 |
| | Entries on clean fixtures | 1 in 12 runs | 1 in 12 runs |
| | Other entries on buggy fixtures | 5 | 4 |

By the rules:

- **Sonnet 5** meets the floor (rule 1). It adds 2 bugs of 48, below the margin of 3, so rule 3 applies: Jev stays on, and no gain beyond noise was measured. Both bugs are `symlink_check_then_delete`, 0 of 3 without Jev and 2 of 3 with it, as in matrices 1 and 2.
- **Haiku 4.5** meets the floor and adds 7 bugs of 48 (rule 2). With Jev no fixture is below its result without Jev except `bufwriter_never_flushed`, 0 of 3 against 1 of 3. In one of those three runs Haiku raised the bug, Jev answered `report`, and Haiku still wrote "no material issues".

What each change did:

- **Review every unit.** The three fixtures with no flag in matrix 4 went from 0 of 9 with Jev to 6 of 9. The skill change reaches mode C too, and Haiku without Jev rose from 26 of 47 to 36 of 48. The listed dimension names are part of that: no entry in either matrix was filed under an invented name that the grader could not count, except one `safety`.
- **`not_material`.** Haiku's other entries with Jev fell from 34 to 4. Verification answered `not_material` on 20 candidates away from the seeded bug's lines and on 5 that overlap them. In all 5 runs the candidate was a second, trivial claim on the same lines, such as a poisoned lock. The seeded bug's own claim came back `report`. No seeded bug was called `not_material` on either model.
- **Facts and the new questions.** Triage flagged the seeded bug's lines in 48 of 48 runs on both models, from 39 of 48 on Haiku. On the seeded bug, Jev answered `dismiss` twice on Haiku and never on Sonnet, from 11 times in matrix 4.
- **`concurrency.lock_order`** was added after the first pass of this re-run. In that pass Haiku found `lock_order_inversion` in 0 of 3 runs with Jev and 3 of 3 without. With the question it found it in 3 of 3. The 24 cells per model that the question or the two corrected fixtures touch were run again; the raw report says which.

Cost per review: Haiku $0.081 without Jev and $0.090 with it, 54 s both ways. Sonnet $0.118 and $0.135, 30 s and 29 s. Jev's share is $0.0002.

Still open after this run: nothing from the list above. One thing the run showed and did not settle: `bufwriter_never_flushed` on Haiku, where the model dropped a finding Jev had confirmed.

## 11. Progress checklist

- [x] Preflight, private repo created
- [x] Research (TypeSafe, Claude Code plugins, rmcp, Dioxus/Axum/Tokio, Codex)
- [x] Core modules, MCP server, dry run
- [x] Unit, HTTP-mock, pipeline, stdout-guard and plugin-consistency tests
- [x] Plugin: plugin.json, marketplace.json, .mcp.json, skill, references, agent
- [x] launch.sh, fresh-install test, smoke.sh
- [x] Fixture corpus (11 buggy, 7 clean), recordings, offline replay
- [x] Tools first: `cargo_diagnostics`, extended Clippy set, changed-line filter, project lint policy, cargo-semver-checks, Miri advice
- [x] `beyond_tooling` and `tool_overlap` on every question; 4 questions deleted, 7 narrowed; deduplication in triage and verification
- [x] Fixtures as buildable crates; 10 harder fixtures and 2 noisy diffs; three-mode eval harness; decision rules (§10)
- [x] Three-mode eval: three matrices on two models, published in the README
- [x] Haiku re-run after the four changes: Jev now adds 4 bugs of 47 on the smaller model (§10, third result)
- [x] First four changes for a smaller model: flags as questions, related definitions and `unseen`, lenient `scope`, one unit per item (§10)
- [x] Adopt the redesigned questions.rs, Cargo.toml and clippy.toml; new verification model
- [x] Live eval on the new question ids
- [x] CI and release workflows
- [x] README
- [x] Headless Claude Code session check (2026-09-19, on the kiln repo: server connected, both tools listed, full skill run)
- [x] Secret scan of history; repo made public (2026-09-19)
- [x] CI green on GitHub (run 35415756948; after the fourth change, run 35539152798 on `7737839`)
- [x] Fourth change and re-run on both models (2026-09-21): Haiku 43/48 with Jev against 36/48, Sonnet 47/48 against 45/48 (§10, fourth result)
- [ ] First tagged release (prebuilt binaries)

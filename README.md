# jev-rust-review

Rust-aware code review for Claude Code (and other MCP clients). [TypeSafe](https://typesafe.ai)'s Jev model does fast, cheap triage and independent verification. Claude does the reasoning, root-cause analysis and fixes.

> **In active development.** This is an early release (v0.1). Interfaces, question ids, thresholds and output formats may change between versions, and prebuilt binaries are not published yet: the first launch builds from source (see [Install](#install)).
>
> Independent project. It is not affiliated with or endorsed by TypeSafe or Anthropic.

```text
rustc / cargo / clippy  = deterministic facts        (Claude runs them)
Jev                     = typed semantic judgment     (this MCP server: triage + verification)
Claude                  = reasoning, root cause, fix  (the rust-review skill + rust-reviewer agent)
```

```text
git diff ──> Rust context collector ──> Jev triage (one fan-out request per unit)
                                              │
                                   flagged (unit, dimension) pairs
                                              v
                         Claude inspects the real code (+ cargo, clippy, tests)
                                              │
                                     candidate findings
                                              v
                      Jev verification: supported / refuted / insufficient_context
                                              v
                          short, high-precision, actionable review
```

## Why Jev

Jev answers typed questions (yes/no probabilities, choices, ordered scores) about a piece of state. It answers quickly, answers many questions in one request, and costs very little: $0.042 per million input tokens. It never writes prose. That makes it a good fit for two narrow jobs:

- **Triage (recall-oriented).** For each changed unit, 44 core questions plus framework-profile questions, one defect each. Examples: "is a `std::sync` guard alive at an `.await`?", "does this `as` cast truncate?". Lexical gates decide which questions apply. A low bar flags where Claude should look. A miss is expensive and a false flag is cheap.
- **Verification (precision-oriented).** Before anything reaches you, Jev re-reads the code and answers:
  - does the code contain the claimed defect: `supported`, `refuted`, or `insufficient_context`;
  - how severe it is, on an ordered score;
  - whether it is a real defect or a matter of taste.

  A high bar decides what is reported. Here a false positive is the expensive error.

Jev's documented weak spots shaped the question design (literal reading, counting, indirection, large irrelevant state, prompt injection, no guarantee that complementary questions agree). See [DESIGN.md](DESIGN.md).

## Requirements

- Claude Code 2.1 or later on Linux or macOS. Windows works through Git Bash but is not tested in CI.
- A TypeSafe API key from [console.typesafe.ai](https://console.typesafe.ai/). Without a key the review still runs, as a Claude-only review, under a clear banner.
- `git`. `curl` or `wget` and `sha256sum` or `shasum` for the prebuilt download. `cargo` only if no prebuilt binary exists for your platform.

## Install

```bash
export TYPESAFE_API_KEY=...        # or set it later in the plugin's settings (stored in your keychain)
claude plugin marketplace add kindintelligence/jev-rust-review
claude plugin install jev-rust-review@jev-rust-review
```

**First run.** The MCP server is a compiled Rust binary. `scripts/launch.sh` tries these in order:

1. a cached binary in the plugin's data directory;
2. a `jev-rust-review` on your `PATH`;
3. a prebuilt release binary, checked against the release's `SHA256SUMS` before use;
4. as a last resort, a `cargo build --release` in the background.

A cold source build takes 65–75 s, longer than Claude Code's 30 s MCP startup timeout. If it gets that far, the server shows as failed with a message pointing at the build log. Run `/mcp` and reconnect `jev` when the build finishes, or run the build in the foreground:

```bash
sh ~/.claude/plugins/cache/*/jev-rust-review/*/scripts/launch.sh --install
```

**Manual alternative:**

```bash
cargo install --locked --git https://github.com/kindintelligence/jev-rust-review --tag v0.1.0
```

The launcher finds the binary on `PATH`. You can also point `JEV_RUST_REVIEW_BIN` at a binary.

**Confirm the server is connected.** Run `/mcp` in Claude Code; `plugin:jev-rust-review:jev` should be connected with two tools. From a script:

```bash
claude -p "list your MCP tools" --output-format stream-json --verbose | head -1   # init event lists mcp_servers
```

**Local development.** Build first, then load the checkout for a session. The launcher uses `target/release` when its version matches.

```bash
cargo build --release
claude --plugin-dir "$(pwd)"
```

## Use

```text
/jev-rust-review:rust-review                  # uncommitted changes (incl. untracked .rs files)
/jev-rust-review:rust-review staged
/jev-rust-review:rust-review main...HEAD      # a branch against its merge base
/jev-rust-review:rust-review rev:abc1234      # what one commit introduced
/jev-rust-review:rust-review src/cache.rs     # changes under a path, or the whole file if unchanged
/jev-rust-review:rust-review --dry-run        # show exactly what would be sent; send nothing
/jev-rust-review:rust-review --no-cargo       # skip cargo check/clippy/test (they run build scripts)
```

`/rust-review` also works when no other skill has that name.

A finding looks like this:

```text
Critical — src/cache.rs:81-92
A std::sync::MutexGuard is held across `.await` in `refresh()`. ...
Fix: copy the needed value out, drop the guard, then await.
Confidence: High (guard binding at :83 is live at the await on :88)
Jev: claim supported 0.93 · severity "critical" (P(high or above) 0.88, confidence 0.81)
```

Every number in the report is Jev's, and labelled as such. Claude states its own confidence in words. When Jev answers `insufficient_context`, the finding depended on code outside the excerpt (lock ordering, callers of a changed API). Claude keeps it only if its own confidence is High, and says Jev could not verify it.

## MCP tools

| Tool | Input | Output |
|---|---|---|
| `evaluate_rust_changes` | `repo_path?`, `scope?`, `dry_run?`, `profiles?`, `max_units?` | Status and reason. Units with file and line ranges computed from the diff. Every Jev answer with probabilities, confidence, threshold and `flagged`. The strongest flags first. Project facts: edition, MSRV, runtime, profiles, crate kind, and whether the diff touches tests. Deterministic Cargo facts, skipped files, redaction counts, token usage and cost. In dry run, the exact request bodies. |
| `verify_rust_findings` | `findings[1..=20]` (`dimension`, `file`, `start_line`, `end_line`, `claim`, `severity`), `repo_path?`, `scope?`, `dry_run?` | Per finding: the `support` choice and `P(supported)`, a severity score (level name, `p_high_or_above`, confidence), a category choice, and a `verdict`: `report`, `insufficient_context`, `uncertain` or `dismiss`. |

The server reads the repository itself: it takes a path and a scope, not a pasted diff. Scopes are validated, and git runs with an argument vector and `--` separators.

## Review dimensions

The 16 dimensions are correctness, ownership, type design, error handling, async, concurrency, unsafe, FFI, performance, idiom, API/semver, macros, serde, security, testing, and Cargo. The full question list is in [DESIGN.md §4](DESIGN.md) and [`src/questions.rs`](src/questions.rs).

Each dimension has a reference file under `skills/rust-review/references/dimensions/`. It says what to look for, **what not to flag**, and what evidence makes a finding. Claude loads a reference only when its dimension is flagged.

Deliberately **not** flagged:
- `unwrap()` in tests or on a just-checked invariant;
- `expect` with an explanation;
- `Arc::clone`, and data moved into a task;
- ordinary `for` loops;
- a sound `unsafe` with a correct `SAFETY` comment;
- style trivia, unless project policy asks for it.

Undocumented `unsafe` is left to Clippy (`undocumented_unsafe_blocks`, `missing_safety_doc`), which the skill runs.

## Framework detection

Profiles activate only when the crate depends on the framework. Each profile is data: detection crates, extra questions, a reference file, and the documentation it was checked against.

| Profile | Checked against | Covers |
|---|---|---|
| Tokio | tokio 1.x docs | runtime nesting, `spawn_blocking`, shutdown, `select!` cancel safety, `blocking_*` in async, sync vs async mutex |
| Axum | axum 0.8 docs | error exposure, middleware order, `Extension` vs `State`, blocking handlers |
| Dioxus | Dioxus 0.7 docs (0.7.10) | signal guards across `.await`, read/write overlap, effect loops, hook rules, stale captures, server-function trust, `use_server_future` tracking |

The async runtime is detected from `Cargo.toml`; Tokio is never assumed.

## Configuration

All settings are environment variables. The defaults are sane.

| Variable | Default | Meaning |
|---|---|---|
| `TYPESAFE_API_KEY` | — | Jev API key. Plugin `userConfig` can supply it instead. |
| `JEV_RUST_REVIEW_MODEL` | `jev-1.13.0` | Pinned to the version the thresholds were tuned against. `jev-latest` also works. |
| `JEV_RUST_REVIEW_TRIAGE_THRESHOLDS` | per dimension | For example `unsafe=0.2,async.sequential_awaits=0.65`. Keys are dimension names or question ids. |
| `JEV_RUST_REVIEW_REPORT_THRESHOLDS` | 0.70 (0.80 unsafe, idiom, type_design) | Bar on `P(supported)` for `report`. |
| `JEV_RUST_REVIEW_DISMISS_BELOW` | 0.40 | Below this `P(supported)`, a claim is dismissed (unless Jev said it lacks context). |
| `JEV_RUST_REVIEW_MAX_UNIT_TOKENS` | 6000 | Larger units are split around their changes. |
| `JEV_RUST_REVIEW_MAX_TOTAL_TOKENS` | 400000 | Per evaluation. Units beyond it are reported as skipped. |
| `JEV_RUST_REVIEW_MAX_UNITS` | 60 | Per evaluation. |
| `JEV_RUST_REVIEW_CONCURRENCY` | 4 | Concurrent Jev requests. |
| `JEV_RUST_REVIEW_TIMEOUT_SECS` | 30 | Per request. |
| `JEV_RUST_REVIEW_MAX_RETRIES` | 3 | On 408, 429, 5xx, timeouts, and connection errors. `Retry-After` is honoured. |
| `JEV_RUST_REVIEW_PROFILES` | `auto` | `auto`, `none`, or a list such as `tokio,axum`. |
| `JEV_RUST_REVIEW_CARGO` | `1` | `0` stops the skill from running cargo. |
| `JEV_RUST_REVIEW_DRY_RUN` | `0` | `1` makes every call a dry run. |
| `JEV_RUST_REVIEW_LOG` | `warn` | Log filter. Logs go to stderr only. |
| `JEV_RUST_REVIEW_BIN` | — | Launcher: use this binary. |
| `JEV_RUST_REVIEW_NO_DOWNLOAD` | `0` | Launcher: never download a prebuilt binary. |
| `JEV_RUST_REVIEW_BUILD_WAIT` | 20 | Launcher: seconds to wait for a source build before giving up on this start. |

## Privacy and data flow

- **Reviewed code is sent to TypeSafe.** The server sends the changed code and its enclosing items to `https://api.typesafe.ai/v1/systemone`, straight from your machine. There is no proxy and no telemetry. See TypeSafe's [privacy policy](https://typesafe.ai/legal/privacy-policy) and [data processing agreement](https://typesafe.ai/legal/data-processing). TypeSafe states it does not train on requests. The only other network access is the optional, checksum-verified binary download from this repository's GitHub releases.
- **Dry run** (`--dry-run`, or `dry_run: true`) returns the exact request bodies and sends nothing.
- **Secret files are never sent.** Files such as `.env*`, `*.pem`, `*.key`, `id_rsa*`, and credential or secret files are skipped even when they are in the diff. Token-shaped strings (AWS, GitHub, GitLab, Slack, Stripe, Google, `sk-`, JWT, PEM) and high-entropy secret assignments are redacted, and the counts are reported.
- **The API key stays local.** It is never logged or echoed, and it is never written to source, config or fixtures.

## Cost

In the live eval, 40 Jev requests (18 fixtures, triage plus verification) used **43,477 input tokens, $0.0018**, about 1,100 tokens per request. A typical review of a 10-unit diff with a handful of candidate findings costs well under a tenth of a cent.

## Eval results

The corpus lives under `fixtures/`: 11 seeded-bug diffs and 7 clean diffs full of bait. It includes a guard held across `.await`, a `select!` cancellation bug, a truncating cast, check-then-act, unsound `unsafe`, a semver break, an untagged-serde ambiguity, and a prompt-injection comment.

Live runs used `jev-1.13.0` on 2026-09-19:

| | Old question set | Current question set |
|---|---|---|
| Triage recall (buggy fixture flagged in an expected dimension) | 11/11 | 11/11 |
| Clean fixtures with any triage flag | 3/7 | 3/7 |
| Clean fixtures flagged in the bait's own dimension | 0/7 | 1/7 |
| True claims verified as `report` | 10/11 | 11/11 |
| Bait claims verified as `report` (false positives) | 0/7 | 0/7 |
| Range of `supported` on true claims / maximum on bait claims | 0.74–0.96 / 0.31 | 0.86–1.00 / 0.11 |

Triage flags on clean code are cheap by design. They send Claude to look, and verification then dismissed every bait claim. The one bait-dimension flag is `async.sequential_awaits` (0.63 against a 0.55 bar) on a send loop into a bounded channel.

The corpus is small, and the numbers show that the pipeline works on these cases; they are not a general accuracy claim. CI replays the recorded answers offline (`recorded_answers_meet_targets`), so a question or threshold change that regresses these numbers fails the build.

```bash
cargo test --test eval                                        # offline replay
TYPESAFE_API_KEY=... cargo test --test eval live_eval -- --ignored --nocapture
JEV_EVAL_RECORD=1 TYPESAFE_API_KEY=... cargo test --test eval live_eval -- --ignored   # re-record
```

## Codex and other MCP clients

The server is a plain stdio MCP server. For Codex, install the binary and add it to `~/.codex/config.toml`:

```bash
cargo install --locked --git https://github.com/kindintelligence/jev-rust-review --tag v0.1.0
```

```toml
[mcp_servers.jev-rust-review]
command = "jev-rust-review"
env_vars = ["TYPESAFE_API_KEY"]
startup_timeout_sec = 20
```

Codex does not set `CLAUDE_PROJECT_DIR`. The server then uses MCP roots if the client offers them, otherwise its working directory, so pass `repo_path` explicitly if reviews target the wrong directory. The skill and agent are Claude Code features. In other clients, call `evaluate_rust_changes` and then `verify_rust_findings` yourself. The server's `instructions` describe that flow. The Codex path is documented from Codex's MCP docs but was not tested here.

## Limitations

- Jev sees one unit at a time. Findings that span files come back as `insufficient_context` and rest on Claude's judgment.
- Question gates are lexical, so an unusual spelling of a pattern can skip a question.
- Recall and false-flag numbers come from an 18-fixture corpus.
- A source-build fallback cannot finish inside the MCP startup window. Prebuilt binaries are the intended path.
- Windows is untested in CI.
- Thresholds are starting points and were tuned on the corpus only.

## Contributing

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && bash scripts/smoke.sh
```

- **Add a fixture:** create `fixtures/{buggy,clean}/<name>/` with `before.rs`, `after.rs`, and `fixture.toml`. The TOML holds the file path, `expected_dimensions` for buggy fixtures, extra `deps`, and a `[claim]` located by `start`/`end` substrings. Then re-record with a key.
- **Add a question or dimension:** edit [`src/questions.rs`](src/questions.rs) (data only). Its tests enforce wording rules, unique ids, compiling gates, and "no diff marker opens a gate". Add or update the dimension's reference file.
- **Add a framework profile:** add a `Profile` entry (detection crates, questions, `verified_against`) and `skills/rust-review/references/frameworks/<name>.md`. The pipeline does not change.

Licence: MIT.

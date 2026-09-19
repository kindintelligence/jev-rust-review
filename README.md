# jev-rust-review

A Claude Code plugin for Rust code review. [TypeSafe](https://typesafe.ai)'s Jev model triages the diff and checks each candidate finding. Claude reads the code, finds the root cause and proposes the fix.

> **In active development.** This is v0.1. Interfaces, question ids, thresholds and output formats may change between versions. Prebuilt binaries are not published yet, so the first launch builds from source (see [Install](#install)).
>
> This is an independent project. TypeSafe and Anthropic do not endorse it and are not affiliated with it.

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

Jev answers typed questions about a piece of state: yes/no probabilities, choices and ordered scores. It never writes prose. One request can carry many questions, and input costs $0.042 per million tokens. That suits two narrow jobs.

- **Triage finds where to look.** Each changed unit gets up to 44 core questions plus framework questions. Each question targets one defect. Examples: "is a `std::sync` guard alive at an `.await`?" and "does this `as` cast truncate?". Lexical gates decide which questions apply. The bar to flag is low, because a missed bug costs more than a wasted look.
- **Verification decides what you see.** Before a finding reaches you, Jev re-reads the code and answers three questions:
  - does the code contain the claimed defect (`supported`, `refuted` or `insufficient_context`);
  - how severe is it, on an ordered score;
  - is it a real defect or a matter of taste.

  The bar to report is high, because a false positive costs more than a missed nitpick.

The question design works around Jev's documented weak spots. These include literal reading, counting, indirection, prompt injection and large irrelevant state. Jev also does not guarantee that complementary questions agree. [DESIGN.md](DESIGN.md) has the detail.

## Requirements

- Claude Code 2.1 or later on Linux or macOS. Windows works through Git Bash, but CI does not test it.
- A TypeSafe API key from [console.typesafe.ai](https://console.typesafe.ai/). Without a key, the review still runs as a Claude-only review under a banner.
- `git`. The prebuilt download also needs `curl` or `wget`, and `sha256sum` or `shasum`.
- `cargo`, until prebuilt binaries are published.

## Install

```bash
export TYPESAFE_API_KEY=...        # or set it later in the plugin's settings (stored in your keychain)
claude plugin marketplace add kindintelligence/jev-rust-review
claude plugin install jev-rust-review@jev-rust-review
```

**First run.** The MCP server is a compiled Rust binary. `scripts/launch.sh` tries these in order:

1. a cached binary in the plugin's data directory;
2. a `jev-rust-review` on your `PATH`;
3. a prebuilt release binary, checked against the release's `SHA256SUMS` before use (once releases exist);
4. a `cargo build --release` in the background.

A cold source build takes 65 to 75 s on an 8-core machine. Claude Code's MCP startup timeout is 30 s. So on the first start the server shows as failed, with a message pointing at the build log. Run `/mcp` and reconnect `jev` when the build finishes. You can also run the build in the foreground:

```bash
sh ~/.claude/plugins/cache/*/jev-rust-review/*/scripts/launch.sh --install
```

**Manual install:**

```bash
cargo install --locked --git https://github.com/kindintelligence/jev-rust-review --tag v0.1.0
```

The launcher finds the binary on `PATH`. You can also point `JEV_RUST_REVIEW_BIN` at a binary.

**Check the connection.** Run `/mcp` in Claude Code. `plugin:jev-rust-review:jev` should show as connected with two tools. From a script:

```bash
claude -p "list your MCP tools" --output-format stream-json --verbose | head -1   # init event lists mcp_servers
```

**Local development.** Build first, then load the checkout for a session. The launcher checks `target/release` before its cache. It refreshes the cached copy whenever you rebuild.

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
High · async · src/relay.rs:9-14
`forward(&tx, event)` is raced against `heartbeat.tick()` in `tokio::select!`.
`forward` awaits `tx.send(event)`. When the tick wins while the channel is
full, the send future is dropped with the event inside it: the event is lost.
Why no tool sees it: cancellation safety is documented in prose, not in types.
Fix: reserve first (`tx.reserve().await`), then send on the permit.
Confidence: High (`forward` in src/sink.rs:9 awaits `Sender::send`)
Jev: claim supported 0.91 · severity "high" (P(high or above) 0.84, confidence 0.78)
```

Every number in a report comes from Jev and says so. Claude states its own confidence in words. Sometimes Jev answers `insufficient_context`. That means the finding depends on code outside the excerpt, such as lock ordering or API callers. Claude keeps such a finding only when its own confidence is High, and says Jev could not verify it.

## MCP tools

| Tool | Input | Output |
|---|---|---|
| `evaluate_rust_changes` | `repo_path?`, `scope?`, `dry_run?`, `profiles?`, `max_units?` | Status and reason. Units, with line ranges computed from the diff. Every Jev answer with its probabilities, confidence, threshold and `flagged`. The strongest flags, listed first. Project facts: edition, MSRV, runtime, profiles, crate kind, and whether the diff touches tests. Cargo facts, skipped files, redaction counts, token usage and cost. In a dry run, the exact request bodies. |
| `verify_rust_findings` | `findings[1..=20]` (`dimension`, `file`, `start_line`, `end_line`, `claim`, `severity`), `repo_path?`, `scope?`, `dry_run?` | For each finding: the `support` choice and `P(supported)`, a severity score (level name, `p_high_or_above`, confidence), a category choice, and a `verdict` (`report`, `insufficient_context`, `uncertain` or `dismiss`). |

The server reads the repository itself. It takes a path and a scope, not a pasted diff. It validates scopes, and runs git with an argument vector and `--` separators.

## Review dimensions

There are 16 dimensions:

| Group | Dimensions |
|---|---|
| Behaviour | correctness, error handling, async, concurrency |
| Memory and interop | ownership, unsafe, FFI |
| Design | type design, API/semver, macros, serde, idiom |
| Other | performance, security, testing, Cargo |

[DESIGN.md §4](DESIGN.md) and [`src/questions.rs`](src/questions.rs) list every question.

Each dimension has a reference file under `skills/rust-review/references/dimensions/`. It says what to look for, **what not to flag**, and what evidence makes a finding. Claude loads a reference only when its dimension is flagged.

The reviewer deliberately does **not** flag:

- `unwrap()` in tests, or on an invariant checked just before;
- `expect` with an explanation;
- `Arc::clone`, or data moved into a task;
- ordinary `for` loops;
- a sound `unsafe` with a correct `SAFETY` comment;
- style points, unless project policy asks for them.

Clippy handles undocumented `unsafe` (`undocumented_unsafe_blocks` and `missing_safety_doc`). The skill runs it.

## Framework detection

A profile activates only when the crate depends on its framework. Each profile is data: detection crates, extra questions, a reference file, and the documentation it was checked against.

| Profile | Checked against | Covers |
|---|---|---|
| Tokio | tokio 1.x docs | runtime nesting, `spawn_blocking`, shutdown, `select!` cancel safety, `blocking_*` in async, sync vs async mutex |
| Axum | axum 0.8 docs | error exposure, middleware order, `Extension` vs `State`, blocking handlers |
| Dioxus | Dioxus 0.7 docs (0.7.10) | signal guards across `.await`, read/write overlap, effect loops, hook rules, stale captures, server-function trust, `use_server_future` tracking |

The server detects the async runtime from `Cargo.toml`. It never assumes Tokio.

## Configuration

Every setting is an environment variable.

| Variable | Default | Meaning |
|---|---|---|
| `TYPESAFE_API_KEY` | none | Jev API key. The plugin's `userConfig` can supply it instead. |
| `JEV_RUST_REVIEW_API_URL` | `https://api.typesafe.ai` | Jev endpoint. Must be https. Plain http is accepted only for localhost, for testing. |
| `JEV_RUST_REVIEW_MODEL` | `jev-1.13.0` | Pinned to the version the thresholds were tuned against. `jev-latest` also works. |
| `JEV_RUST_REVIEW_TRIAGE_THRESHOLDS` | per dimension | For example `unsafe=0.2,async.sequential_awaits=0.65`. Keys are dimension names or question ids. |
| `JEV_RUST_REVIEW_REPORT_THRESHOLDS` | 0.70 (0.80 for unsafe, idiom, type_design) | Bar on `P(supported)` for `report`. |
| `JEV_RUST_REVIEW_DISMISS_BELOW` | 0.40 | A claim below this `P(supported)` is dismissed, unless Jev said it lacked context. |
| `JEV_RUST_REVIEW_MAX_UNIT_TOKENS` | 6000 | Larger units are split around their changes. |
| `JEV_RUST_REVIEW_MAX_TOTAL_TOKENS` | 400000 | Per evaluation. Units beyond it are reported as skipped. |
| `JEV_RUST_REVIEW_MAX_UNITS` | 60 | Per evaluation. |
| `JEV_RUST_REVIEW_CONCURRENCY` | 4 | Concurrent Jev requests. |
| `JEV_RUST_REVIEW_TIMEOUT_SECS` | 30 | Per request. |
| `JEV_RUST_REVIEW_MAX_RETRIES` | 3 | Applies to 408, 429 and 5xx responses, timeouts, and connection errors. `Retry-After` is honoured. |
| `JEV_RUST_REVIEW_PROFILES` | `auto` | `auto`, `none`, or a list such as `tokio,axum`. |
| `JEV_RUST_REVIEW_CARGO` | `1` | `0` stops the skill from running cargo. |
| `JEV_RUST_REVIEW_DRY_RUN` | `0` | `1` makes every call a dry run. |
| `JEV_RUST_REVIEW_LOG` | `warn` | Log filter. Logs go to stderr only. |
| `JEV_RUST_REVIEW_BIN` | none | Launcher: use this binary. |
| `JEV_RUST_REVIEW_NO_DOWNLOAD` | `0` | Launcher: never download a prebuilt binary. |
| `JEV_RUST_REVIEW_BUILD_WAIT` | 20 | Launcher: seconds to wait for a source build before this start gives up. |

## Privacy and data flow

- **Your code goes to TypeSafe.** The server sends the changed code and its enclosing items to `https://api.typesafe.ai/v1/systemone`. Requests go straight from your machine, with no proxy and no telemetry. TypeSafe states it does not train on requests. See its [privacy policy](https://typesafe.ai/legal/privacy-policy) and [data processing agreement](https://typesafe.ai/legal/data-processing).
- **The only other download is the binary.** The launcher can fetch a checksum-verified binary from this repository's GitHub releases.
- **A dry run sends nothing.** `--dry-run` or `dry_run: true` returns the exact request bodies instead.
- **Secret files are never sent.** The server skips them even when the diff changes them. That covers `.env*`, `*.pem`, `*.key`, `id_rsa*`, and credential or secret files.
- **Secrets inside code are redacted.** Token-shaped strings are replaced, and the report counts each kind. That covers AWS, GitHub, GitLab, Slack, Stripe, Google and `sk-` keys, plus JWTs and PEM blocks. It also covers high-entropy strings assigned to secret-like names.
- **The API key stays local.** The server never logs or echoes it. It never appears in source, config or fixtures.

## Cost

The live eval made 40 Jev requests over 18 fixtures and used **43,477 input tokens ($0.0018)**. That is about 1,100 tokens per request. A real review of one changed file in another repository used 2 requests and 7,040 tokens (about $0.0003).

## Eval results

The corpus under `fixtures/` has 11 diffs with a seeded bug and 7 clean diffs full of bait. Examples of seeded bugs:

- a guard held across `.await`;
- a `select!` cancellation bug;
- a truncating cast;
- check-then-act;
- unsound `unsafe`;
- a semver break;
- an ambiguous untagged serde enum;
- a comment that tries to inject instructions.

Live runs used `jev-1.13.0` on 2026-09-19:

| | Old question set | Current question set |
|---|---|---|
| Triage recall (buggy fixture flagged in an expected dimension) | 11/11 | 11/11 |
| Clean fixtures with any triage flag | 3/7 | 3/7 |
| Clean fixtures flagged in the bait's own dimension | 0/7 | 1/7 |
| True claims verified as `report` | 10/11 | 11/11 |
| Bait claims verified as `report` (false positives) | 0/7 | 0/7 |
| `supported` range on true claims; maximum on bait claims | 0.74 to 0.96; 0.31 | 0.86 to 1.00; 0.11 |

Triage flags on clean code are cheap by design: they send Claude to look. Verification then dismissed every bait claim. The one bait-dimension flag was `async.sequential_awaits` at 0.63 against a 0.55 bar. It fired on a send loop into a bounded channel.

The corpus is small. These numbers show the pipeline works on these cases, not general accuracy. CI replays the recorded answers offline (`recorded_answers_meet_targets`). A question or threshold change that lowers these numbers fails the build.

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

Codex does not set `CLAUDE_PROJECT_DIR`. The server then uses MCP roots if the client offers them, and otherwise its working directory. If reviews target the wrong directory, pass `repo_path` explicitly.

The skill and agent are Claude Code features. In other clients, call `evaluate_rust_changes` and then `verify_rust_findings` yourself. The server's `instructions` describe that flow.

I have not tested the Codex setup. It follows Codex's MCP documentation.

## Limitations

- Jev sees one unit at a time. Findings that span files come back as `insufficient_context` and rest on Claude's judgment.
- Question gates are lexical. An unusual spelling of a pattern can skip a question.
- The recall and false-flag numbers come from an 18-fixture corpus.
- A source build cannot finish inside the MCP startup window. Prebuilt binaries are the intended path.
- CI does not test Windows.
- The thresholds are starting points, tuned on the corpus only.

## Contributing

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && bash scripts/smoke.sh
```

- **Add a fixture.** Create `fixtures/{buggy,clean}/<name>/` with `before.rs`, `after.rs` and `fixture.toml`. The TOML holds the file path, extra `deps`, and a `[claim]` located by `start` and `end` substrings. Buggy fixtures also list `expected_dimensions`. Then re-record with a key.
- **Add a question or dimension.** Edit the data in [`src/questions.rs`](src/questions.rs). Its tests check wording rules and unique ids. They also check that gates compile and that no diff marker opens a gate. Add or update the dimension's reference file.
- **Add a framework profile.** Add a `Profile` entry with detection crates, questions and `verified_against`. Add `skills/rust-review/references/frameworks/<name>.md`. The pipeline does not change.

Licence: MIT.

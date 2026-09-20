# jev-rust-review

A Claude Code plugin for Rust code review that goes deeper than the compiler. rustc, Clippy and cargo-semver-checks report what tools can find, filtered to the lines you changed. [TypeSafe](https://typesafe.ai)'s Jev model and Claude spend their effort on what no tool reports: a `select!` branch that loses data when it is cancelled, a check and an act under two separate locks, a length guard removed from a caller in another file.

> **In active development.** This is v0.1. Interfaces, question ids, thresholds and output formats may change between versions. Prebuilt binaries are not published yet, so the first launch builds from source (see [Install](#install)).
>
> This is an independent project. TypeSafe and Anthropic do not endorse it and are not affiliated with it.

```text
rustc / clippy / cargo-semver-checks = deterministic facts    (this MCP server runs them, filtered to the change)
Jev                                  = typed semantic judgment (this MCP server: triage + verification)
Claude                               = reasoning, root cause, fix (the rust-review skill + rust-reviewer agent)
```

```text
git diff ──> cargo_diagnostics: clippy + semver-checks, changed lines only ──> tool facts, reported once
    │
    └──────> Rust context collector ──> Jev triage (one fan-out request per unit)
                                              │
                          flagged (unit, dimension) pairs, minus what a tool reported
                                              v
                 Claude reviews every changed unit, flagged first (+ tests)
                                              │
                                     candidate findings
                                              v
              verification: tool_reported / supported / refuted / insufficient_context
                                              v
                          short, high-precision, actionable review
```

## Tools first

A Rust developer already gets excellent feedback from rustc, Clippy and cargo. A review that repeats it is noise. So the server runs the tools itself and treats their output as fact:

- **One Clippy run, filtered in code.** `cargo clippy --message-format=json` gives every compiler diagnostic, Clippy's defaults, and ten off-by-default lints. They cover lossy casts, needless ownership, redundant clones, ignored must-use values, discarded errors, non-`Send` fields, wildcard enum arms and undocumented `unsafe`. The server keeps errors anywhere and warnings on changed lines. Claude never reads raw compiler output.
- **Your lint policy wins.** If the project allows a lint, with `#[allow]` or in `[lints.clippy]`, the review stays quiet about it.
- **cargo-semver-checks answers API questions.** It runs when a library's `pub` surface changed and it is installed. The plugin never installs anything. When it is absent, the report says so in one line.
- **Miri is suggested, never run,** when `unsafe` code changed.
- **Nothing is reported twice.** A defect a tool reported on the same lines never comes back as a Jev flag or a Claude finding. The check is code, and it also runs without a TypeSafe key.

Every Jev question carries a sentence that says why no compiler check, lint or cargo tool answers it. A test fails if one is empty.

## Why Jev

Jev answers typed questions about a piece of state: yes/no probabilities, choices and ordered scores. It never writes prose. One request can carry many questions, and input costs $0.042 per million tokens. That suits two narrow jobs.

- **Triage finds where to look first.** Each changed unit gets up to 42 core questions plus framework questions. Each question targets one defect that no tool reports. Examples: "can a `select!` branch that loses the race lose data?" and "is this check separated from the action it guards?". Where a tool finds the pattern, the question keeps only the judgement: Clippy finds the lossy cast, and Jev is asked whether the value can be out of range. Lexical gates decide which questions apply. The bar to flag is low, because a missed bug costs more than a wasted look. A flag sets the order of reading and not its limits: a unit with no flag is still reviewed.
- **Verification decides what you see.** Before a finding reaches you, Jev re-reads the code and answers three questions:
  - does the code contain the claimed defect (`supported`, `refuted` or `insufficient_context`);
  - how severe is it, on an ordered score;
  - is it a real defect, a matter of taste, or a risk too remote to be worth your time, such as a lock poisoned by an earlier panic.

  The bar to report is high, because a false positive costs more than a missed nitpick.

The question design works around Jev's documented weak spots. These include literal reading, counting, indirection, prompt injection and large irrelevant state. Jev also does not guarantee that complementary questions agree. [DESIGN.md](DESIGN.md) has the detail.

## Requirements

- Claude Code 2.1 or later on Linux or macOS. Windows works through Git Bash, but CI does not test it.
- A TypeSafe API key from [console.typesafe.ai](https://console.typesafe.ai/). Without a key, the review still runs as a Claude-only review under a banner.
- `git`. The prebuilt download also needs `curl` or `wget`, and `sha256sum` or `shasum`.
- `cargo` with Clippy. Without Clippy the tool facts come from `cargo check` alone.
- Optional: [`cargo-semver-checks`](https://github.com/obi1kenobi/cargo-semver-checks), for library API changes.

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

**Check the connection.** Run `/mcp` in Claude Code. `plugin:jev-rust-review:jev` should show as connected with three tools. From a script:

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
/jev-rust-review:rust-review --no-cargo       # skip clippy, semver-checks and tests (cargo runs build scripts)
/jev-rust-review:rust-review --json           # end the report with a machine-readable list of what it reported
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
| `cargo_diagnostics` | `repo_path?`, `scope?` | Compiler errors anywhere and warnings on changed lines, each with file, lines, lint code and message. The extra lints that ran. Counts of warnings outside the change and of diagnostics the project's lint policy silenced. The cargo-semver-checks verdict when a library's `pub` surface changed. A Miri suggestion when `unsafe` changed. |
| `evaluate_rust_changes` | `repo_path?`, `scope?`, `dry_run?`, `profiles?`, `max_units?` | Status and reason. Units, with line ranges computed from the diff. Every Jev answer with its probabilities, confidence, threshold and `flagged`. The strongest flags, listed first, and `tool_covered`: flags dropped because a tool already reported the defect. Project facts: edition, MSRV, runtime, profiles, crate kind, and whether the diff touches tests. Cargo facts, skipped files, redaction counts, token usage and cost. In a dry run, the exact request bodies. |
| `verify_rust_findings` | `findings[1..=20]` (`dimension`, `file`, `start_line`, `end_line`, `claim`, `severity`), `repo_path?`, `scope?`, `dry_run?` | For each finding: the `support` choice and `P(supported)`, a severity score (level name, `p_high_or_above`, confidence), a category choice, and a `verdict`: `report`, `insufficient_context`, `uncertain`, `dismiss`, `not_material` when the claim is true but needs a condition the code gives no reason to expect, or `tool_reported` when a tool already reported the defect on those lines. |

The server reads the repository itself. It takes a path and a scope, not a pasted diff. It validates scopes, and runs git and cargo with argument vectors, never a shell. The skill calls the tools in the order of the table.

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

It also leaves these to the tools, which report them on changed lines:

- a guard held across `.await` (`await_holding_lock`, on by default);
- lossy `as` casts, `let _ =` on a `Result`, `.map_err(|_| ..)`, wildcard enum arms;
- an owned argument that is only read, and a clone whose original is never used again;
- undocumented `unsafe` (`undocumented_unsafe_blocks` and `missing_safety_doc`);
- a removed or changed `pub` item, when cargo-semver-checks is installed.

## Framework detection

A profile activates only when the crate depends on its framework. Each profile is data: detection crates, extra questions, a reference file, and the documentation it was checked against.

| Profile | Checked against | Covers |
|---|---|---|
| Tokio | tokio 1.x docs | runtime nesting, `spawn_blocking`, shutdown, `select!` cancel safety, `blocking_*` in async, sync vs async mutex |
| Axum | axum 0.8 docs | error exposure, middleware order, `Extension` vs `State`, blocking handlers |
| Dioxus | Dioxus 0.7 docs (0.7.10) | signal guards across `.await` (Clippy does not know these guards), read/write overlap, effect loops, hook rules, stale captures, server-function trust, `use_server_future` tracking |

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
| `JEV_RUST_REVIEW_CARGO` | `1` | `0` stops the server and the skill from running cargo. |
| `JEV_RUST_REVIEW_SEMVER_CHECKS` | `1` | `0` skips cargo-semver-checks even when it is installed. |
| `JEV_RUST_REVIEW_CARGO_TIMEOUT_SECS` | 600 | Upper bound on one cargo run. |
| `JEV_RUST_REVIEW_CARGO_TARGET_DIR` | none | A target directory for the server's own cargo runs, so they never wait on your build's lock. It costs one extra full build. |
| `JEV_RUST_REVIEW_DRY_RUN` | `0` | `1` makes every call a dry run. |
| `JEV_RUST_REVIEW_LOG` | `warn` | Log filter. Logs go to stderr only. |
| `JEV_RUST_REVIEW_BIN` | none | Launcher: use this binary. |
| `JEV_RUST_REVIEW_NO_DOWNLOAD` | `0` | Launcher: never download a prebuilt binary. |
| `JEV_RUST_REVIEW_BUILD_WAIT` | 20 | Launcher: seconds to wait for a source build before this start gives up. |

## Privacy and data flow

- **Your code goes to TypeSafe.** The server sends the changed code and its enclosing items to `https://api.typesafe.ai/v1/systemone`. Requests go straight from your machine, with no proxy and no telemetry. TypeSafe states it does not train on requests. See its [privacy policy](https://typesafe.ai/legal/privacy-policy) and [data processing agreement](https://typesafe.ai/legal/data-processing).
- **cargo runs on your machine.** `cargo_diagnostics` runs `cargo clippy`, which executes the project's build scripts and proc macros, as any build does. In a repository you do not trust, use `--no-cargo`.
- **The only other download is the binary.** The launcher can fetch a checksum-verified binary from this repository's GitHub releases.
- **A dry run sends nothing.** `--dry-run` or `dry_run: true` returns the exact request bodies instead.
- **Secret files are never sent.** The server skips them even when the diff changes them. That covers `.env*`, `*.pem`, `*.key`, `id_rsa*`, and credential or secret files.
- **Secrets inside code are redacted.** Token-shaped strings are replaced, and the report counts each kind. That covers AWS, GitHub, GitLab, Slack, Stripe, Google and `sk-` keys, plus JWTs and PEM blocks. It also covers high-entropy strings assigned to secret-like names.
- **The API key stays local.** The server never logs or echoes it. It never appears in source, config or fixtures.

## Cost and speed

Jev is close to free. Sixty full reviews used 308,839 Jev input tokens, which is **$0.0130**, against $8.08 of Claude (`claude-sonnet-5`). A review with Jev took 29 s on average and one without took 30 s. The offline Jev-stage eval, with one request per changed function, made 130 requests for 128,790 tokens ($0.0054).

## Eval results

Jev is a tool the coding agent uses. It is not an alternative to the agent. So the eval compares the same agent with and without it, against a baseline of the tools alone:

| Mode | What runs |
|---|---|
| T | tools only: Clippy with the extra lints, and cargo-semver-checks. No model |
| C | the plugin run headless (`claude -p --plugin-dir`) with no Jev |
| J | the same, with Jev triage and verification |

**A bug counts only if the tools miss it.** The corpus under `fixtures/` has 23 diffs with one seeded bug each and 10 clean diffs full of bait. Three of the clean ones carry a claim that is true but trivial, such as "this `unwrap` panics if the mutex is poisoned". Every fixture is a small crate that builds. The tools catch 4 of the 23 (a guard across `.await`, a truncating cast, `.map_err(|_| ..)` and a semver break), so those take no part in C or J. Of the other 19, ten are harder cases that span functions or files. Seven of those are modelled on real bugs or documented behaviour, cited in each `fixture.toml`: RUSTSEC-2021-0003, CVE-2018-1000810, CVE-2022-21658, and tokio, axum and std documentation. Two are tidy-up diffs of 25 and 30 changed functions with one seeded bug.

**Grading is code.** A run finds the bug when an entry in its report overlaps the seeded lines, in the right file, in an expected dimension. Every entry on a clean fixture counts against it. C and J ran on 16 of the 19 bugs and 4 of the 10 clean fixtures, three runs per cell, to keep one matrix at 120 headless runs.

Results, 2026-09-19 to 2026-09-21, Jev `jev-1.13.0`, Claude Code 2.1.278:

| Matrix | Mode | Graded runs | Beyond-tooling bugs found | Entries on clean fixtures | Other entries on buggy fixtures | Claude cost | Jev cost | Wall time per run |
|---|---|---|---|---|---|---|---|---|
| all | T | 30 | 0 of 19 (4 of 23 seeded bugs) | 0 in 7 | 4 | none | none | about 4 s |
| 1. `claude-sonnet-5`, Jev verdict as a gate | C | 60 | 45/48 | 0 in 12 | 9 | $6.97 | none | 33 s |
| | J | 60 | 33/48 | 1 in 12 | 7 | $8.19 | $0.0075 | 39 s |
| 2. `claude-sonnet-5`, Jev verdict as a second opinion | C | 60 | 45/48 | 2 in 12 | 5 | $6.89 | none | 33 s |
| | J | 60 | **46/48** | 1 in 12 | 6 | $7.82 | $0.0072 | 37 s |
| 3. `claude-haiku-4-5`, Jev verdict as a second opinion | C | 50 | 29/41 | 0 in 9 | 5 | $5.16 | none | 54 s |
| | J | 49 | 21/38 | 3 in 11 | 21 | $5.50 | $0.0071 | 59 s |
| 4. `claude-haiku-4-5`, after the four changes for a smaller model | C | 59 | 26/47 | 2 in 12 | 15 | $4.85 | none | 52 s |
| | J | 59 | **31/48** | 2 in 11 | 34 | $5.17 | $0.0141 | 54 s |
| 5. `claude-sonnet-5`, after the fourth change | C | 60 | 45/48 | 0 in 12 | 1 | $7.08 | none | 30 s |
| | J | 60 | **47/48** | 0 in 12 | 1 | $8.08 | $0.0130 | 29 s |
| 6. `claude-haiku-4-5`, after the fourth change | C | 60 | 36/48 | 1 in 12 | 5 | $4.86 | none | 54 s |
| | J | 60 | **43/48** | 1 in 12 | 4 | $5.40 | $0.0145 | 54 s |

What this shows:

- **Latest result (matrices 5 and 6): Jev helps the smaller model and does not hurt the larger one.** Haiku 4.5 found 43 of 48 bugs with Jev and 36 of 48 without, with the same single entry on clean fixtures and 4 other entries against 5. Sonnet 5 found 47 of 48 with Jev and 45 of 48 without, which is inside the run-to-run noise, with no entry on a clean fixture either way. All 240 runs could be graded.
- **Matrix 4 had been misread.** It lost `notify_lost_wakeup` and `route_added_after_layer` with Jev, and the first explanation was library behaviour that Jev does not know. The cells said something simpler: in all 9 runs where triage flagged nothing, Haiku reported nothing, because the skill said to inspect flagged code. Flags now set the order of reading and not its limits. Those three fixtures went from 0 of 9 with Jev to 6 of 9.
- **The noise is gone.** Verification has a `not_material` verdict for a claim that is true only under a condition the code gives no reason to expect. Haiku's other entries with Jev fell from 34 to 4. No seeded bug was called `not_material`.
- **The fourth change is partly tuning on known fixtures.** Six library facts and three questions (`concurrency.lost_wakeup`, `concurrency.lock_order`, `axum.route_after_layer`) were written from documentation, after seeing which fixtures failed. They show the mechanism works. They do not show how often Jev will know the behaviour behind a bug it has not seen. DESIGN.md section 10 lists every change and what prompted it.
- **What is left on Haiku.** `bufwriter_never_flushed` is 0 of 3 with Jev and 1 of 3 without. In one run Haiku raised the bug, Jev confirmed it, and Haiku still reported nothing.

The earlier matrices, kept for the record:

- **Matrix 1 found a design fault.** The skill dropped a finding when Jev answered `uncertain` or `dismiss`. In 14 of the 15 runs where J missed the bug, Claude had found it and was overruled. Those bugs depend on code outside the excerpt Jev reads. The skill now treats a verdict as a second opinion: Claude re-reads the code, and keeps a finding it can still demonstrate. That is the only change between matrix 1 and matrix 2.
- **On Sonnet 5 in matrix 2, Jev met the floor and added little.** 46 of 48 against 45 of 48 is inside the run-to-run noise. The one clear gain is `symlink_check_then_delete`: without Jev, Claude reported a different bug in that function in all 6 runs and never the symlink race. With Jev's concurrency flag it reported the race in 5 of 6.
- **On Haiku 4.5, Jev first helped after four changes (matrix 4).** Counting runs graded in both modes, Haiku found 26 of 47 bugs alone and 30 of 47 with Jev. The gain is on the two large diffs: 3 of 6 alone and 6 of 6 with Jev, which flagged 7 of their 31 and 7 of their 25 units. It also found `check_then_act` in 3 of 3 runs against 0 of 3, and `select_cancellation` in 3 of 3 against 1 of 3. Ungraded runs fell from 21 of 120 to 2 of 120. The four changes: a flag is returned as a question to check; verification is shown the definitions a claim names and never answers `dismiss` about code it was not shown; `scope` may be absent or malformed in any way the server can see; and each changed function is its own unit.
- **In matrix 4 Jev still cost Haiku three bugs and added noise.** With Jev it found `notify_lost_wakeup` in 0 of 3 runs against 3 of 3, and `route_added_after_layer` in 0 of 3 against 1 of 3. Both rest on documented library behaviour that Jev does not know. On `error_flattened_to_string` a `security.unbounded_input` flag drew it to a different claim in all 3 runs. Other entries on buggy fixtures rose from 15 to 34. Many are a flag restated, such as "`expect` on a poisoned mutex can panic", which the skill told it not to report and which Jev's verification confirmed as true. Matrix 6 fixed all three.
- **Before those changes, Jev made the Haiku review worse (matrix 3).** Counting only runs graded in both modes, Haiku found 25 of 34 bugs alone and 20 of 34 with Jev. It also reported more noise: 21 other entries against 5. Of its 17 misses with Jev, in 7 it never raised the seeded bug at all. In 6 it raised it and Jev answered `dismiss`. In the other 4 Jev agreed and the run still did not count; I have not read those four. The extra noise follows the triage flags. In the three runs I checked, Haiku reported "unwrap on a poisoned mutex" where `error_handling.panic` had flagged, and "u32 sum can overflow" where `correctness.overflow` had. A flag says where to look, and the smaller model reports it as a finding. Jev did help Haiku on three fixtures: `check_then_act`, `select_cancellation` and the symlink race.
- **In matrix 3 Haiku also struggled with the tools themselves.** 21 of its 120 runs could not be graded, against none of 240 on Sonnet. It sent the MCP tools malformed JSON when the scope was empty, or tried to call them through the shell.
- **The grader is strict.** A bug reported under another dimension does not count. In matrices 3 and 4 Haiku filed several concurrency bugs under `async`, and some under names that do not exist, such as `logic`. The skill now lists the dimension names.
- **Triage narrows a large diff now.** In matrices 1 to 3 the server merged adjacent changed functions into one unit, so the two large diffs were 3 and 5 units and Jev flagged 63 of 96 units overall. With one unit per changed function they are 31 and 25 units, and in matrix 4 Jev flagged 86 of 261.

[`eval/2026-09-21-three-mode.md`](eval/2026-09-21-three-mode.md) (matrices 5 and 6) and [`eval/2026-09-20-three-mode.md`](eval/2026-09-20-three-mode.md) (matrices 1 to 4) have the per-fixture tables, Jev's verdicts on every candidate, the ungraded runs, and **the list of entries that need a person's judgement**. Two fixtures were corrected before matrices 5 and 6. `symlink_check_then_delete` had a second bug I did not plan, and `explained_expect`, a clean fixture, changed which identifiers match. Both models had reported both.

The corpus is small and synthetic. These numbers say how the pipeline behaves on these cases, not how accurate it is in general.

**Jev's two stages on their own** (ideal claims, no Claude), on the 19 beyond-tooling bugs and 10 clean fixtures (2026-09-21):

| | |
|---|---|
| Triage flagged the buggy fixture in an expected dimension | 19/19 (14/19 before the fourth change) |
| Clean fixtures with any triage flag | 5/10 |
| True claims verified as `report` | 19/19. It was 12/19 before verification was shown the definitions a claim names, and 16/19 before the library facts |
| Bait claims verified as `report` | 0/10 |
| True but trivial bait claims judged `not_material` | 3/3 |

These fixtures are the ones the questions and facts were written against, so read the 19/19 as "nothing known is broken", not as accuracy. CI replays the recorded answers offline (`recorded_answers_meet_targets`), so a question or threshold change that lowers these numbers fails the build.

```bash
cargo test --test eval                                        # offline replay of Jev's two stages
JEV_EVAL_RECORD=1 TYPESAFE_API_KEY=... cargo test --test eval live_eval -- --ignored   # re-record
cargo build --release
E2E_TAG=mine E2E_MODEL=claude-sonnet-5 E2E_RUNS=3 cargo test --test e2e -- --ignored --nocapture   # three-mode eval; spends Claude tokens
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

The skill and agent are Claude Code features. In other clients, call `cargo_diagnostics`, then `evaluate_rust_changes`, then `verify_rust_findings` yourself. The server's `instructions` describe that flow.

I have not tested the Codex setup. It follows Codex's MCP documentation.

## Limitations

- Jev sees one excerpt at a time. A finding that depends on another file can come back as `uncertain` or `dismiss` and not as `insufficient_context`. The skill treats every verdict as a second opinion for that reason.
- Jev does not know library documentation beyond the 15 facts in `src/facts.rs`, which cover tokio, axum, serde, Dioxus and std. A claim that rests on any other documented behaviour can come back `dismiss`.
- **A smaller model follows Jev closely.** That is why the wording of the server's output matters as much as its numbers (see [Eval results](#eval-results)).
- Question gates are lexical. An unusual spelling of a pattern can skip a question.
- The numbers come from a 33-fixture synthetic corpus and two models, and part of the question set was written against that corpus.
- A source build cannot finish inside the MCP startup window. Prebuilt binaries are the intended path.
- CI does not test Windows.
- The thresholds are starting points, tuned on the first 18 fixtures. They were not changed for the three-mode eval.

## Contributing

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && bash scripts/smoke.sh
```

- **Add a fixture.** Create `fixtures/{buggy,clean}/<name>/` with `before/` and `after/`, two snapshots of a small crate that builds, and a `fixture.toml`. The TOML holds the claim's file, a `[claim]` located by `start` and `end` substrings, and `tool_catches`: whether Clippy or cargo-semver-checks reports the bug. Buggy fixtures also list `expected_dimensions`. A fixture modelled on a real bug cites it in `source` and is a fresh minimal reproduction, never copied code. Give each fixture one defect and no incidental tool warning. Then re-record with a key.
- **Add a question or dimension.** Edit the data in [`src/questions.rs`](src/questions.rs). First check that no lint answers it: `clippy-driver -W help` lists them. A question needs a `beyond_tooling` sentence, and `tool_overlap` lists the lints that come close. The tests check wording rules, unique ids, that every named lint exists in the installed Clippy, that gates compile, and that no diff marker opens a gate. Add or update the dimension's reference file.
- **Add a framework profile.** Add a `Profile` entry with detection crates, questions and `verified_against`. Add `skills/rust-review/references/frameworks/<name>.md`. The pipeline does not change.

Licence: MIT.

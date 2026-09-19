---
name: rust-review
description: Rust code review that goes deeper than the compiler. Reviews uncommitted changes, staged changes, a commit, a range such as main...HEAD, or a path. The tools report what tools can find, filtered to the changed lines; TypeSafe Jev triages the rest and verifies candidate findings; you inspect the flagged code and report only verified, material defects that no tool reported. Use when asked to review Rust code or Rust changes.
argument-hint: "[scope: staged | <rev> | <a>..<b> | <a>...<b> | <path>] [--no-cargo] [--dry-run] [--json]"
allowed-tools: Read, Grep, Glob, Bash(cargo test:*), Bash(cargo nextest:*), Bash(git diff:*), Bash(git show:*), Bash(git log:*), mcp__plugin_jev-rust-review_jev__cargo_diagnostics, mcp__plugin_jev-rust-review_jev__evaluate_rust_changes, mcp__plugin_jev-rust-review_jev__verify_rust_findings
---

# Rust review

Arguments: `$ARGUMENTS`

A Rust developer already gets excellent feedback from rustc, Clippy and cargo. Repeating it is noise. This review reports what those tools found on the changed lines once, as fact, and spends everything else on defects no tool can see.

The review uses three layers. Each does the job it is good at:

- **Tools** (rustc, Clippy, cargo-semver-checks) give deterministic facts. The `cargo_diagnostics` tool runs them and filters the output to the change in code. You never read raw compiler output.
- **Jev** (the `jev` MCP server) gives typed, cheap judgements on questions no tool answers. It triages where to look, then checks your candidate findings against the code. It never writes prose. Its numbers are routing signals, not truth.
- **You** do the reasoning. Read the real code, find root causes and propose fixes. Decide what is worth the user's time.

Prefer precision over coverage. "No material issues found" is a good outcome. Say it plainly, along with what you checked.

## 1. Parse the arguments

- Everything that is not a flag is the **scope**. Empty means uncommitted changes, including untracked `.rs` files. `staged`, a commit, `a..b`, `a...b`, and a path are also accepted. Prefix with `rev:` or `path:` to disambiguate.
- `--dry-run`: call the evaluation tool with `dry_run: true`. Summarise which files, units and questions *would* be sent and roughly how many tokens they would cost, then stop. Nothing leaves the machine.
- `--no-cargo`: skip step 2 and the tests in step 4.
- `--json`: end the report with the machine-readable block described in step 7.

## 2. Collect the tool facts

Skip this step if `--no-cargo` was given. Otherwise call `cargo_diagnostics` with the `scope`, **before** anything else. Triage and verification use its result to avoid repeating it.

`cargo` runs build scripts and proc macros from the project and its dependencies. If the repository is untrusted, ask before calling the tool.

Read these fields:

- `diagnostics`: compiler errors anywhere, and warnings on changed lines. Each is a fact. Report every one in the "From the tools" section of the report, one line each, with its lint or error code. Do not re-derive them, and do not write a finding of your own for the same defect.
- `semver`: the verdict of cargo-semver-checks when a library's `pub` surface changed.
  - `breaking`: report each entry of `breaks` as a fact.
  - `compatible`: the public API did not break. Do not raise API findings.
  - `absent`: say in one line that cargo-semver-checks is not installed, so the API change was checked by judgement only. Never install it.
- `advice`: pass each line on as it is. When `unsafe` changed it suggests Miri. Do not run Miri.
- `silenced_by_project_policy`: the project allows these lints. Stay quiet about them.
- `status` other than `ok`: say why in one line and continue without tool facts.

A tool diagnostic can still deserve more than its one line. A lossy cast that Clippy reports is usually harmless; one that truncates an attacker-controlled length is a bug. When you can show a concrete failure, say so **in that diagnostic's entry** and raise its severity there. It stays one entry.

## 3. Triage with Jev

Call `evaluate_rust_changes` with `scope` (and `dry_run` if asked). Read these fields in this order:

- `status`:
  - `ok` or `partial`: continue. For `partial`, mention `reason`.
  - `jev_unavailable`: continue **without Jev**. Treat every unit in `units` as flagged. Put this banner at the top of the report, verbatim except for the reason: `> Jev triage and verification were skipped: <reason>. This is a Claude-only review.` Still run step 6: it removes duplicates of tool diagnostics without Jev.
  - `dry_run`: report as described in step 1, then stop.
- `flagged`: (unit, dimension) pairs, strongest first. **A flag says where to look. It is not a finding.**
- `tool_covered`: flags on lines where a tool already reported the same defect. They never become findings. Use them only as a hint that the tool's diagnostic there deserves the closer look described in step 2.
- `references`: the reference files to load, relative to this skill directory (`${CLAUDE_SKILL_DIR}`). Load **only** those, with Read, before judging code in that dimension.
- `project`: edition, MSRV (`rust_version`), crate kind, `async_runtimes`, and profiles. Never assume Tokio: use only what `async_runtimes` says.
- `project.tests`: whether the diff touches test code (`diff_touches_tests`), and which changed units are and are not test code. The tool computes this from the diff. Jev does not judge it. If behaviour changed and no tests did, consider that under the testing dimension.
- `cargo_facts`: deterministic dependency, feature, build-script and lockfile changes. Report risky ones directly. They need no Jev verification.
- `skipped` and `redactions`: mention any skipped units or redactions in one line at the end of the report.

## 4. Run the tests

Skip this step if `cargo.enabled` is false or `--no-cargo` was given. Otherwise run the commands in `cargo.commands` with Bash, from the repository root. Respect `cargo.note` about features. A failing test in changed code is a finding with deterministic evidence.

## 5. Inspect flagged code

Inspect a handful of flagged units yourself. For many units, or several files, dispatch the `rust-reviewer` agent: `jev-rust-review:rust-reviewer`. Give it:

- each unit's file, `lines`, `changed_lines`, flagged dimensions and questions, and the Jev signal;
- the loaded reference file paths;
- the tool diagnostics from step 2;
- the project facts.

Read the real code with enough surrounding context. The defects worth your time are the ones that build clean: they often depend on code the diff did not touch. Open the callers, the other lock sites, and the function on the other side of the channel. Follow the reference files. They say what **not** to flag and what evidence turns a suspicion into a finding.

A candidate finding needs:

- a defect that **no tool reported**. If `cargo_diagnostics` already has it on those lines, it is not a candidate;
- a concrete failure: the input, interleaving, or call that breaks;
- the exact file and line range, taken from the code you read;
- **one** defect, stated in **one** sentence that names identifiers, not line numbers. Put everything the claim relies on inside the line range you give. A defect may depend on code elsewhere (lock ordering across functions, callers of a changed `pub` API). If so, say so in your evidence. Jev will likely answer `insufficient_context`;
- a severity: critical, high, medium, or low;
- your own confidence in words, High or Medium, with the evidence. Drop Low-confidence ideas.

## 6. Verify candidates

Call `verify_rust_findings` with all candidates (at most 20) and the same `scope`. Do this even when Jev is unavailable. The server re-reads the code itself. For each result:

- `verdict: tool_reported`: a tool already reported this defect on these lines, and `tool` names it. Drop the finding. The tool's entry stands, and you may add your failure scenario to it.
- `verdict: report`: keep it.
- `verdict: insufficient_context`: Jev could not judge the claim from the local code (its `support` answer was `insufficient_context`). This is **not a refutation**. If your own confidence is High, keep the finding. Say in the report that Jev could not verify it from local context. If your confidence is Medium, move it to "considered and dismissed".
- `verdict: uncertain`: keep it **only** if deterministic evidence proves it independently. Examples are a failing test or a reproduction you ran. Otherwise move it to "considered and dismissed".
- `verdict: dismiss`: Jev chose `refuted`, the claim is a style preference, or `supported` is below the dismiss bar. Drop it. At most, list it under "considered and dismissed".
- `verdict: not_verified` with the tool's `status: jev_unavailable`: Jev did not see it. Keep it only on High confidence.
- `status: invalid` or `error`: fix the input (line range, file, severity) and retry once. Otherwise treat the finding as unverified and apply the same rule as `uncertain`.

You may quote these numbers:

- `supported`: Jev's probability that the claim is supported;
- `severity.name`;
- `severity.p_high_or_above`: Jev's probability mass on high or critical;
- `severity.confidence`;
- the `category` choice.

## 7. Write the report

Lead with a one-line verdict, for example "2 issues worth fixing before merge" or "No material issues found". Then list findings, most severe first. Cap the list at the findings that matter. Each finding's first line is `<Severity> · <dimension> · <file>:<start>-<end>`.

```text
High · async · src/relay.rs:9-14
`forward(&tx, event)` is raced against `heartbeat.tick()` in `tokio::select!`.
`forward` awaits `tx.send(event)`. When the tick wins while the channel is
full, the send future is dropped with the event inside it: the event is lost.
Why no tool sees it: cancellation safety is documented in prose, not in types.
The code builds and lints clean.
Fix: reserve first (`tx.reserve().await`), then send on the permit, or move
the send out of the `select!`.
Confidence: High (`forward` in src/sink.rs:9 awaits `Sender::send`, which
drops the message when cancelled)
Jev: claim supported 0.91 · severity "high" (P(high or above) 0.84, confidence 0.78)
```

A kept `insufficient_context` finding uses this Jev line instead:

```text
Jev: could not verify from local context (insufficient_context 0.74); kept on
Claude's High confidence
```

After the findings, list the tool facts under "From the tools", one line each:

```text
From the tools (changed lines only)
- src/frame.rs:12 clippy::cast_possible_truncation: casting `usize` to `u16` may truncate
- cargo-semver-checks function_missing: `demo::parse` was removed
```

Rules:

- **One entry per defect.** A defect a tool reported appears under "From the tools" and nowhere else. If it is a real bug, say so in that entry.
- **Numbers.** The only percentages or probabilities in the report are Jev's, labelled as Jev's. Use `supported`, the severity name, `p_high_or_above` and confidence, and the `support` choice from `verify_rust_findings`. State your own certainty in words (High or Medium), with its evidence. Never invent a number.
- **Severity.** If Jev's severity differs from yours, show both and explain which you used.
- **No style trivia.** Skip style points unless project policy asks for them (CLAUDE.md, `clippy.toml`, lint configuration).
- **Dismissed candidates.** Put anything considered but dismissed in a short collapsed `<details>` block. Title it "Considered and dismissed". Give one line each, with the Jev number that dismissed it.
- **Closing line.** End with one line on what was checked. Include scope, units, dimensions asked, the tools that ran, and Jev tokens and estimated cost from `usage`. Add anything skipped or redacted.

With `--json`, end the report with one fenced `json` block and nothing after it. List every entry the report presents as a problem: your findings with `"source": "review"`, and tool facts with `"source": "tool"`. Leave out dismissed candidates.

```json
{"findings": [{"severity": "high", "dimension": "async", "file": "src/relay.rs", "start_line": 9, "end_line": 14, "title": "select! drops the send future and loses the event", "source": "review"}]}
```

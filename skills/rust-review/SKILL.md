---
name: rust-review
description: Rust-specific code review of uncommitted changes, staged changes, a commit, a range such as main...HEAD, or a path. TypeSafe Jev triages the diff and verifies candidate findings; you inspect the flagged code, run cargo, and report only verified, material defects. Use when asked to review Rust code or Rust changes.
argument-hint: "[scope: staged | <rev> | <a>..<b> | <a>...<b> | <path>] [--no-cargo] [--dry-run]"
allowed-tools: Read, Grep, Glob, Bash(cargo check:*), Bash(cargo clippy:*), Bash(cargo test:*), Bash(cargo nextest:*), Bash(git diff:*), Bash(git show:*), Bash(git log:*), mcp__plugin_jev-rust-review_jev__evaluate_rust_changes, mcp__plugin_jev-rust-review_jev__verify_rust_findings
---

# Rust review

Arguments: `$ARGUMENTS`

The review uses three layers. Each does the job it is good at:

- **cargo** (rustc, clippy, tests) gives deterministic facts. Treat its output as evidence.
- **Jev** (the `jev` MCP server) gives typed, cheap judgements. It triages where to look, then checks your candidate findings against the code. It never writes prose. Its numbers are routing signals, not truth.
- **You** do the reasoning. Read the real code, find root causes and propose fixes. Decide what is worth the user's time.

Prefer precision over coverage. "No material issues found" is a good outcome. Say it plainly, along with what you checked.

## 1. Parse the arguments

- Everything that is not a flag is the **scope**. Empty means uncommitted changes, including untracked `.rs` files. `staged`, a commit, `a..b`, `a...b`, and a path are also accepted. Prefix with `rev:` or `path:` to disambiguate.
- `--dry-run`: call the evaluation tool with `dry_run: true`. Summarise which files, units and questions *would* be sent and roughly how many tokens they would cost, then stop. Nothing leaves the machine.
- `--no-cargo`: skip step 3.

## 2. Triage with Jev

Call `evaluate_rust_changes` with `scope` (and `dry_run` if asked). Read these fields in this order:

- `status`:
  - `ok` or `partial`: continue. For `partial`, mention `reason`.
  - `jev_unavailable`: continue **without Jev**. Treat every unit in `units` as flagged, and skip step 5. Put this banner at the top of the report, verbatim except for the reason: `> Jev triage and verification were skipped: <reason>. This is a Claude-only review.`
  - `dry_run`: report as described in step 1, then stop.
- `flagged`: (unit, dimension) pairs, strongest first. **A flag says where to look. It is not a finding.**
- `references`: the reference files to load, relative to this skill directory (`${CLAUDE_SKILL_DIR}`). Load **only** those, with Read, before judging code in that dimension.
- `project`: edition, MSRV (`rust_version`), crate kind, `async_runtimes`, and profiles. Never assume Tokio: use only what `async_runtimes` says.
- `project.tests`: whether the diff touches test code (`diff_touches_tests`), and which changed units are and are not test code. The tool computes this from the diff. Jev does not judge it. If behaviour changed and no tests did, consider that under the testing dimension.
- `cargo_facts`: deterministic dependency, feature, build-script and lockfile changes. Report risky ones directly. They need no Jev verification.
- `skipped` and `redactions`: mention any skipped units or redactions in one line at the end of the report.

## 3. Collect cargo evidence

Skip this step if `cargo.enabled` is false or `--no-cargo` was given. Otherwise run the commands in `cargo.commands` with Bash, from the repository root. Respect `cargo.note` about features.

`cargo` runs build scripts and proc macros from the project and its dependencies. If the repository is untrusted, ask before running it.

Keep only diagnostics that touch changed files. A compiler error or failing test in changed code is a finding with deterministic evidence.

The clippy command enables `clippy::undocumented_unsafe_blocks` and `clippy::missing_safety_doc`. Undocumented `unsafe` is Clippy's job, not Jev's. Report those warnings on changed code directly, as low severity with the lint as evidence. The exception is when you can also show the code is unsound. Then it is an unsafe finding in its own right, verified in step 5.

## 4. Inspect flagged code

Inspect a handful of flagged units yourself. For many units, or several files, dispatch the `rust-reviewer` agent: `jev-rust-review:rust-reviewer`. Give it:

- each unit's file, `lines`, `changed_lines`, flagged dimensions and questions, and the Jev signal;
- the loaded reference file paths;
- the relevant cargo diagnostics;
- the project facts.

Read the real code with enough surrounding context. Follow the reference files. They say what **not** to flag and what evidence turns a suspicion into a finding.

A candidate finding needs:

- a concrete failure: the input, interleaving, or call that breaks;
- the exact file and line range, taken from the code you read;
- **one** defect, stated in **one** sentence that names identifiers, not line numbers. Put everything the claim relies on inside the line range you give. A defect may depend on code elsewhere (lock ordering across functions, callers of a changed `pub` API). If so, say so in your evidence. Jev will likely answer `insufficient_context`;
- a severity: critical, high, medium, or low;
- your own confidence in words, High or Medium, with the evidence. Drop Low-confidence ideas.

## 5. Verify candidates with Jev

Call `verify_rust_findings` with all candidates (at most 20) and the same `scope`. The server re-reads the code itself. For each result:

- `verdict: report`: keep it.
- `verdict: insufficient_context`: Jev could not judge the claim from the local code (its `support` answer was `insufficient_context`). This is **not a refutation**. If your own confidence is High, keep the finding. Say in the report that Jev could not verify it from local context. If your confidence is Medium, move it to "considered and dismissed".
- `verdict: uncertain`: keep it **only** if deterministic evidence proves it independently. Examples are a compiler error, a clippy lint, a failing test, or a reproduction you ran. Otherwise move it to "considered and dismissed".
- `verdict: dismiss`: Jev chose `refuted`, the claim is a style preference, or `supported` is below the dismiss bar. Drop it. At most, list it under "considered and dismissed".
- `status: invalid` or `error`: fix the input (line range, file, severity) and retry once. Otherwise treat the finding as unverified and apply the same rule as `uncertain`.

You may quote these numbers:

- `supported`: Jev's probability that the claim is supported;
- `severity.name`;
- `severity.p_high_or_above`: Jev's probability mass on high or critical;
- `severity.confidence`;
- the `category` choice.

## 6. Write the report

Lead with a one-line verdict, for example "2 issues worth fixing before merge" or "No material issues found". Then list findings, most severe first. Cap the list at the findings that matter.

```text
Critical: src/cache.rs:81-92
A std::sync::MutexGuard is held across `.await` in `refresh()`. Other tasks on
the same worker block on the lock, and `refresh()` re-enters `get()`, which
takes the same lock: a deadlock path.
Why it matters in Rust: std guards are not released at `.await`; the future
holds the lock for as long as it is suspended.
Fix: copy the needed value out, drop the guard, then await.
Confidence: High (guard binding at :83 is live at the await on :88)
Jev: claim supported 0.93 · severity "critical" (P(high or above) 0.88, confidence 0.81)
```

A kept `insufficient_context` finding uses this Jev line instead:

```text
Jev: could not verify from local context (insufficient_context 0.74); kept on
Claude's High confidence
```

Rules:

- **Numbers.** The only percentages or probabilities in the report are Jev's, labelled as Jev's. Use `supported`, the severity name, `p_high_or_above` and confidence, and the `support` choice from `verify_rust_findings`. State your own certainty in words (High or Medium), with its evidence. Never invent a number.
- **Severity.** If Jev's severity differs from yours, show both and explain which you used.
- **No style trivia.** Skip style points unless project policy asks for them (CLAUDE.md, `clippy.toml`, lint configuration).
- **Dismissed candidates.** Put anything considered but dismissed in a short collapsed `<details>` block. Title it "Considered and dismissed". Give one line each, with the Jev number that dismissed it.
- **Closing line.** End with one line on what was checked. Include scope, units, dimensions asked, cargo commands run, and Jev tokens and estimated cost from `usage`. Add anything skipped or redacted.

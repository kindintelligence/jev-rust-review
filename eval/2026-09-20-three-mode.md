# Three-mode eval, raw reports

These are the reports `tests/e2e.rs` wrote, unedited apart from the headings. The README has the summary and DESIGN.md sections 9 and 10 have the method and the rules.

- **T** is tools only, **C** is Claude with the plugin and no Jev, **J** is the full pipeline.
- "Runs" counts graded cells. A cell is ungraded when the session never got a result from `evaluate_rust_changes`, or the report had no JSON block. Those cells are listed at the end of each report.
- Token counts are the CLI's `modelUsage` summed over every turn. Almost all of the input is cache reads, so the sum tracks the number of turns. Use the cost column to compare modes.
- Mode T ran once per matrix and gives the same result in all four.
- "Findings for a person to judge" are entries that did not match the seeded bug's lines and dimension. Some are real: in matrices 1 to 4 `symlink_check_then_delete` had a second, unplanned bug (a kept `.lock` file makes `remove_dir` fail). The fixture was corrected on 2026-09-21. Some are the seeded bug filed under another dimension, which the grader does not count.


## Matrix 1: claude-sonnet-5, Jev verdict used as a gate (2026-09-19, commit cc48377)

| Mode | Runs | Beyond-tooling bugs found | All seeded bugs found | False positives on clean fixtures | Extra findings on buggy fixtures | Claude tokens (in / out) | Claude cost | Jev tokens | Jev cost | Wall time |
|---|---|---|---|---|---|---|---|---|---|---|
| T | 30 | 0/19 | 4/23 | 0 in 7 runs | 4 | 0 / 0 | $0.00 | 0 | $0.0000 | 2 min |
| C | 60 | 45/48 | 45/48 | 0 in 12 runs | 9 | 10644052 / 94600 | $6.97 | 0 | $0.0000 | 33 min |
| J | 60 | 33/48 | 33/48 | 1 in 12 runs | 7 | 12303289 / 124688 | $8.19 | 179084 | $0.0075 | 39 min |

#### Per fixture (runs that found the seeded bug)

| Fixture | Kind | Label: tool catches | T | C | J |
|---|---|---|---|---|---|
| blocking_in_async | buggy | false | 0/1 | - | - |
| bufwriter_never_flushed | buggy | false | 0/1 | 3/3 | 3/3 |
| check_then_act | buggy | false | 0/1 | 3/3 | 3/3 |
| error_flattened_to_string | buggy | false | 0/1 | 3/3 | 2/3 |
| guard_across_await | buggy | true | 1/1 | - | - |
| length_guard_weakened | buggy | false | 0/1 | 3/3 | 2/3 |
| lock_order_inversion | buggy | false | 0/1 | 3/3 | 2/3 |
| lossy_error | buggy | true | 1/1 | - | - |
| noisy_inventory_tidy | buggy | false | 0/1 | 3/3 | 3/3 |
| noisy_service_tidy | buggy | false | 0/1 | 3/3 | 3/3 |
| notify_lost_wakeup | buggy | false | 0/1 | 3/3 | 0/3 |
| prompt_injection | buggy | false | 0/1 | 3/3 | 3/3 |
| repeat_capacity_overflow | buggy | false | 0/1 | 3/3 | 3/3 |
| route_added_after_layer | buggy | false | 0/1 | 3/3 | 0/3 |
| select_cancellation | buggy | false | 0/1 | 3/3 | 3/3 |
| select_drops_send | buggy | false | 0/1 | 3/3 | 0/3 |
| semver_break | buggy | true | 1/1 | - | - |
| size_hint_trusted | buggy | false | 0/1 | 3/3 | 0/3 |
| swallowed_result | buggy | false | 0/1 | - | - |
| symlink_check_then_delete | buggy | false | 0/1 | 0/3 | 3/3 |
| truncating_cast | buggy | true | 1/1 | - | - |
| unsound_unsafe | buggy | false | 0/1 | 3/3 | 3/3 |
| untagged_serde | buggy | false | 0/1 | - | - |
| arc_clone | clean |  | 0 FP in 1 | 0 FP in 3 | 0 FP in 3 |
| bounded_channel | clean |  | 0 FP in 1 | - | - |
| explained_expect | clean |  | 0 FP in 1 | 0 FP in 3 | 0 FP in 3 |
| plain_for_loop | clean |  | 0 FP in 1 | - | - |
| scoped_lock_before_await | clean |  | 0 FP in 1 | 0 FP in 3 | 0 FP in 3 |
| sound_unsafe | clean |  | 0 FP in 1 | 0 FP in 3 | 1 FP in 3 |
| test_unwrap | clean |  | 0 FP in 1 | - | - |

#### Jev's two stages, judged apart (mode J)

- Triage flagged the seeded bug's lines in 39/48 buggy runs. It flagged 63 of 96 units overall.
- Verification of candidates on the seeded bug: dismiss 5, insufficient_context 4, report 36, uncertain 11.
- Verification of every other candidate: dismiss 1, uncertain 3.

#### Findings for a person to judge

- C lock_order_inversion run 1 (buggy): `src/report.rs:12-15` [correctness / review] reconcile's held == 0 check always holds and never compares balances to the ledger
- C symlink_check_then_delete run 1 (buggy): `src/purge.rs:12-18` [error_handling / review] remove_dir on a subdirectory that still holds a kept .lock file fails and aborts the purge
- C symlink_check_then_delete run 2 (buggy): `src/purge.rs:13-18` [error_handling / review] remove_dir fails on a subdirectory containing a preserved .lock file, aborting purge midway
- C symlink_check_then_delete run 3 (buggy): `src/purge.rs:15-18` [error_handling / review] remove_dir fails on subdirectory that still holds a skipped .lock file, aborting purge midway
- C unsound_unsafe run 1 (buggy): `src/table.rs:10-13` [api / review] lookup return type changed from Option<u32> to u32, breaking callers
- C unsound_unsafe run 1 (buggy): `src/table.rs:12-12` [clippy / tool] clippy::undocumented_unsafe_blocks: unsafe block missing a safety comment
- C unsound_unsafe run 2 (buggy): `src/table.rs:11-11` [api / review] lookup return type changed from Option<u32> to u32 (breaking, missed by semver-checks)
- C unsound_unsafe run 2 (buggy): `src/table.rs:12-12` [clippy::undocumented_unsafe_blocks / tool] unsafe block missing a safety comment
- C unsound_unsafe run 3 (buggy): `src/table.rs:11-13` [api / review] Public Table::lookup return type changed from Option<u32> to u32 (breaking)
- J error_flattened_to_string run 1 (buggy): `src/counter.rs:10-14` [security / review] read_to_string reads an unbounded file to parse a u64
- J error_flattened_to_string run 3 (buggy): `src/counter.rs:10-14` [security / review] read_to_string reads the whole file with no size limit
- J lock_order_inversion run 2 (buggy): `src/report.rs:10-11` [correctness / review] reconcile locks ledger then accounts, opposite to transfer: deadlock
- J lock_order_inversion run 2 (buggy): `src/report.rs:8-15` [correctness / review] reconcile never compares ledger amounts to balances
- J symlink_check_then_delete run 1 (buggy): `src/purge.rs:15-18` [correctness / review] Recursive purge keeps .lock files, so remove_dir on the non-empty subdirectory fails and aborts the purge
- J symlink_check_then_delete run 3 (buggy): `src/purge.rs:12-18` [correctness / review] remove_dir fails on a subdirectory that still holds a kept .lock file, aborting the purge
- J unsound_unsafe run 1 (buggy): `src/table.rs:11-11` [api / review] Table::lookup return type changed from Option<u32> to u32 (breaking)
- J sound_unsafe run 1 (clean): `src/sum.rs:5-8` [unsafe / tool] unsafe changed; Miri suggested but not run
- T lossy_error run 1 (buggy): `src/config.rs:12-12` [error_handling / tool] clippy::map_err_ignore: `map_err(|_|...` wildcard pattern discards the original error
- T lossy_error run 1 (buggy): `src/config.rs:1-4294967295` [api / tool] cargo-semver-checks enum_variant_added: enum variant added on exhaustive enum
- T lossy_error run 1 (buggy): `src/config.rs:1-4294967295` [api / tool] cargo-semver-checks enum_variant_missing: pub enum variant removed or renamed
- T unsound_unsafe run 1 (buggy): `src/table.rs:12-12` [tool / tool] clippy::undocumented_unsafe_blocks: unsafe block missing a safety comment


## Matrix 2: claude-sonnet-5, Jev verdict as a second opinion (2026-09-20, commit 123b571)

| Mode | Runs | Beyond-tooling bugs found | All seeded bugs found | False positives on clean fixtures | Extra findings on buggy fixtures | Claude tokens (in / out) | Claude cost | Jev tokens | Jev cost | Wall time |
|---|---|---|---|---|---|---|---|---|---|---|
| T | 30 | 0/19 | 4/23 | 0 in 7 runs | 4 | 0 / 0 | $0.00 | 0 | $0.0000 | 2 min |
| C | 60 | 45/48 | 45/48 | 2 in 12 runs | 5 | 9811196 / 95493 | $6.89 | 0 | $0.0000 | 33 min |
| J | 60 | 46/48 | 46/48 | 1 in 12 runs | 6 | 10720253 / 114030 | $7.82 | 171715 | $0.0072 | 37 min |

#### Per fixture (runs that found the seeded bug)

| Fixture | Kind | Label: tool catches | T | C | J |
|---|---|---|---|---|---|
| blocking_in_async | buggy | false | 0/1 | - | - |
| bufwriter_never_flushed | buggy | false | 0/1 | 3/3 | 3/3 |
| check_then_act | buggy | false | 0/1 | 3/3 | 3/3 |
| error_flattened_to_string | buggy | false | 0/1 | 3/3 | 3/3 |
| guard_across_await | buggy | true | 1/1 | - | - |
| length_guard_weakened | buggy | false | 0/1 | 3/3 | 3/3 |
| lock_order_inversion | buggy | false | 0/1 | 3/3 | 2/3 |
| lossy_error | buggy | true | 1/1 | - | - |
| noisy_inventory_tidy | buggy | false | 0/1 | 3/3 | 3/3 |
| noisy_service_tidy | buggy | false | 0/1 | 3/3 | 3/3 |
| notify_lost_wakeup | buggy | false | 0/1 | 3/3 | 3/3 |
| prompt_injection | buggy | false | 0/1 | 3/3 | 3/3 |
| repeat_capacity_overflow | buggy | false | 0/1 | 3/3 | 3/3 |
| route_added_after_layer | buggy | false | 0/1 | 3/3 | 3/3 |
| select_cancellation | buggy | false | 0/1 | 3/3 | 3/3 |
| select_drops_send | buggy | false | 0/1 | 3/3 | 3/3 |
| semver_break | buggy | true | 1/1 | - | - |
| size_hint_trusted | buggy | false | 0/1 | 3/3 | 3/3 |
| swallowed_result | buggy | false | 0/1 | - | - |
| symlink_check_then_delete | buggy | false | 0/1 | 0/3 | 2/3 |
| truncating_cast | buggy | true | 1/1 | - | - |
| unsound_unsafe | buggy | false | 0/1 | 3/3 | 3/3 |
| untagged_serde | buggy | false | 0/1 | - | - |
| arc_clone | clean |  | 0 FP in 1 | 0 FP in 3 | 0 FP in 3 |
| bounded_channel | clean |  | 0 FP in 1 | - | - |
| explained_expect | clean |  | 0 FP in 1 | 1 FP in 3 | 0 FP in 3 |
| plain_for_loop | clean |  | 0 FP in 1 | - | - |
| scoped_lock_before_await | clean |  | 0 FP in 1 | 0 FP in 3 | 0 FP in 3 |
| sound_unsafe | clean |  | 0 FP in 1 | 1 FP in 3 | 1 FP in 3 |
| test_unwrap | clean |  | 0 FP in 1 | - | - |

#### Jev's two stages, judged apart (mode J)

- Triage flagged the seeded bug's lines in 39/48 buggy runs. It flagged 63 of 96 units overall.
- Verification of candidates on the seeded bug: dismiss 5, insufficient_context 3, report 35, uncertain 9.
- Verification of every other candidate: insufficient_context 1.

#### Findings for a person to judge

- C symlink_check_then_delete run 1 (buggy): `src/purge.rs:14-18` [error_handling / review] Recursive purge keeps nested .lock files, so remove_dir fails and aborts the purge
- C symlink_check_then_delete run 2 (buggy): `src/purge.rs:15-18` [error_handling / review] purge fails with DirectoryNotEmpty when a subdirectory contains a .lock file
- C symlink_check_then_delete run 3 (buggy): `src/purge.rs:15-18` [error_handling / review] remove_dir fails on a subdirectory that still holds a kept .lock file, aborting purge
- C unsound_unsafe run 2 (buggy): `src/table.rs:11-13` [api / review] lookup return type changed from Option<u32> to u32, breaking callers
- C unsound_unsafe run 3 (buggy): `src/table.rs:11-13` [api / review] lookup return type changed from Option<u32> to u32 (breaking)
- C explained_expect run 2 (clean): `src/ident.rs:9-11` [api / review] is_ident now rejects leading-digit inputs it previously accepted
- C sound_unsafe run 2 (clean): `src/sum.rs:2-9` [correctness / review] wrapping_add silently wraps on u64 overflow where Iterator::sum panicked in debug builds
- J error_flattened_to_string run 1 (buggy): `src/counter.rs:10-14` [security / review] read_to_string reads the whole file with no size limit
- J error_flattened_to_string run 3 (buggy): `src/counter.rs:10-14` [security / review] load_counter reads the whole file with no size limit
- J lock_order_inversion run 1 (buggy): `src/report.rs:9-12` [correctness / review] reconcile locks ledger then accounts, the reverse of transfer, so the two can deadlock
- J symlink_check_then_delete run 1 (buggy): `src/purge.rs:13-19` [correctness / review] purge fails with DirectoryNotEmpty when a subdirectory holds a .lock file
- J symlink_check_then_delete run 2 (buggy): `src/purge.rs:7-18` [correctness / review] remove_dir fails with DirectoryNotEmpty when a subdirectory contains a kept .lock file, aborting the purge
- J symlink_check_then_delete run 3 (buggy): `src/purge.rs:7-19` [correctness / review] remove_dir fails on subdirectory kept because it contains a .lock file, aborting purge midway
- J sound_unsafe run 1 (clean): `src/sum.rs:1-10` [unsafe / tool] unsafe changed; Miri suggested but not run
- T lossy_error run 1 (buggy): `src/config.rs:12-12` [error_handling / tool] clippy::map_err_ignore: `map_err(|_|...` wildcard pattern discards the original error
- T lossy_error run 1 (buggy): `src/config.rs:1-4294967295` [api / tool] cargo-semver-checks enum_variant_added: enum variant added on exhaustive enum
- T lossy_error run 1 (buggy): `src/config.rs:1-4294967295` [api / tool] cargo-semver-checks enum_variant_missing: pub enum variant removed or renamed
- T unsound_unsafe run 1 (buggy): `src/table.rs:12-12` [tool / tool] clippy::undocumented_unsafe_blocks: unsafe block missing a safety comment


## Matrix 3: claude-haiku-4-5-20251001, Jev verdict as a second opinion (2026-09-20, commit 123b571)

| Mode | Runs | Beyond-tooling bugs found | All seeded bugs found | False positives on clean fixtures | Extra findings on buggy fixtures | Claude tokens (in / out) | Claude cost | Jev tokens | Jev cost | Wall time |
|---|---|---|---|---|---|---|---|---|---|---|
| T | 30 | 0/19 | 4/23 | 0 in 7 runs | 4 | 0 / 0 | $0.00 | 0 | $0.0000 | 2 min |
| C | 50 | 29/41 | 29/41 | 0 in 9 runs | 5 | 15476279 / 184993 | $4.29 | 0 | $0.0000 | 46 min |
| J | 49 | 21/38 | 21/38 | 3 in 11 runs | 21 | 15276031 / 205426 | $4.56 | 167492 | $0.0070 | 51 min |

#### Per fixture (runs that found the seeded bug)

| Fixture | Kind | Label: tool catches | T | C | J |
|---|---|---|---|---|---|
| blocking_in_async | buggy | false | 0/1 | - | - |
| bufwriter_never_flushed | buggy | false | 0/1 | 1/2 | 0/3 |
| check_then_act | buggy | false | 0/1 | 1/2 | 3/3 |
| error_flattened_to_string | buggy | false | 0/1 | 2/3 | 0/3 |
| guard_across_await | buggy | true | 1/1 | - | - |
| length_guard_weakened | buggy | false | 0/1 | 2/3 | 1/1 |
| lock_order_inversion | buggy | false | 0/1 | 0/1 | 0/1 |
| lossy_error | buggy | true | 1/1 | - | - |
| noisy_inventory_tidy | buggy | false | 0/1 | 3/3 | 3/3 |
| noisy_service_tidy | buggy | false | 0/1 | 0/2 | 1/2 |
| notify_lost_wakeup | buggy | false | 0/1 | 3/3 | 0/3 |
| prompt_injection | buggy | false | 0/1 | 3/3 | 1/1 |
| repeat_capacity_overflow | buggy | false | 0/1 | 3/3 | 2/2 |
| route_added_after_layer | buggy | false | 0/1 | 2/2 | 0/2 |
| select_cancellation | buggy | false | 0/1 | 1/3 | 2/2 |
| select_drops_send | buggy | false | 0/1 | 3/3 | 3/3 |
| semver_break | buggy | true | 1/1 | - | - |
| size_hint_trusted | buggy | false | 0/1 | 2/2 | 1/3 |
| swallowed_result | buggy | false | 0/1 | - | - |
| symlink_check_then_delete | buggy | false | 0/1 | 0/3 | 1/3 |
| truncating_cast | buggy | true | 1/1 | - | - |
| unsound_unsafe | buggy | false | 0/1 | 3/3 | 3/3 |
| untagged_serde | buggy | false | 0/1 | - | - |
| arc_clone | clean |  | 0 FP in 1 | 0 FP in 3 | 0 FP in 2 |
| bounded_channel | clean |  | 0 FP in 1 | - | - |
| explained_expect | clean |  | 0 FP in 1 | 0 FP in 2 | 0 FP in 3 |
| plain_for_loop | clean |  | 0 FP in 1 | - | - |
| scoped_lock_before_await | clean |  | 0 FP in 1 | 0 FP in 2 | 3 FP in 3 |
| sound_unsafe | clean |  | 0 FP in 1 | 0 FP in 2 | 0 FP in 3 |
| test_unwrap | clean |  | 0 FP in 1 | - | - |

#### Jev's two stages, judged apart (mode J)

- Triage flagged the seeded bug's lines in 30/38 buggy runs. It flagged 49 of 77 units overall.
- Verification of candidates on the seeded bug: dismiss 12, insufficient_context 2, report 26, uncertain 8.
- Verification of every other candidate: dismiss 4, insufficient_context 1, report 13, uncertain 6.

#### Findings for a person to judge

- C check_then_act run 2 (buggy): `src/registry.rs:10-14` [async / review] TOCTOU race condition allows duplicate registrations
- C lock_order_inversion run 3 (buggy): `src/report.rs:9-16` [async / review] Lock ordering mismatch creates potential deadlock with transfer method
- C noisy_service_tidy run 2 (buggy): `src/sessions.rs:55-63` [async / review] open_session checks the limit before acquiring the lock, violating the at-most-limit invariant
- C noisy_service_tidy run 3 (buggy): `src/sessions.rs:55-62` [async / review] The limit check is performed outside the lock and races with concurrent inserts, allowing more than `limit` sessions to be open.
- C symlink_check_then_delete run 2 (buggy): `src/purge.rs:13-17` [correctness / review] fs::remove_dir fails when subdirectory contains only .lock files
- J check_then_act run 1 (buggy): `src/registry.rs:10-14` [error_handling / review] unwrap on poisoned mutex panics instead of returning error to caller
- J check_then_act run 2 (buggy): `src/registry.rs:10-14` [error_handling / review] Unwrap on poisoned mutex will panic, violating the Result<(), String> contract; should handle mutex poison errors
- J check_then_act run 3 (buggy): `src/registry.rs:9-16` [error_handling / review] Mutex.lock().unwrap() panics on poisoning, making registry unusable
- J error_flattened_to_string run 1 (buggy): `src/counter.rs:10-14` [security / review] read_to_string(path) reads entire file without size limit, enabling DoS
- J error_flattened_to_string run 2 (buggy): `src/counter.rs:10-14` [security / review] load_counter reads entire file without size limits, enabling memory exhaustion
- J error_flattened_to_string run 3 (buggy): `src/counter.rs:10-14` [security / review] load_counter reads file contents unbounded, allowing DoS if path is attacker-controlled
- J lock_order_inversion run 3 (buggy): `src/report.rs:9-16` [correctness / review] Lock acquisition order inverted from transfer(), causing potential deadlock
- J lock_order_inversion run 3 (buggy): `src/report.rs:9-16` [correctness / review] Reconciliation does not verify ledger entries sum to current balances as docstring claims
- J lock_order_inversion run 3 (buggy): `src/report.rs:14-14` [correctness / review] Sum of i64 account balances can overflow, giving incorrect reconciliation result
- J noisy_inventory_tidy run 2 (buggy): `src/stock.rs:4-5` [correctness / review] total_pallet() sums u32 into u32, wrapping on overflow
- J noisy_inventory_tidy run 2 (buggy): `src/orders.rs:4-5` [correctness / review] total_order() sums u32 into u32, wrapping on overflow
- J noisy_inventory_tidy run 2 (buggy): `src/pricing.rs:4-5` [correctness / review] total_tier() sums u32 into u32, wrapping on overflow
- J noisy_inventory_tidy run 2 (buggy): `src/orders.rs:35-36` [correctness / review] total_manifest() sums u32 into u32, wrapping on overflow
- J noisy_inventory_tidy run 2 (buggy): `src/pricing.rs:35-36` [correctness / review] total_invoice() sums u32 into u32, wrapping on overflow
- J noisy_inventory_tidy run 2 (buggy): `src/stock.rs:40-41` [correctness / review] total_batch() sums u32 into u32, wrapping on overflow
- J noisy_service_tidy run 1 (buggy): `src/sessions.rs:54-63` [error_handling / review] open_session will panic if mutex is poisoned by concurrent panic
- J noisy_service_tidy run 1 (buggy): `src/jobs.rs:3-6` [correctness / review] total_job silently wraps on overflow instead of panicking
- J noisy_service_tidy run 1 (buggy): `src/metrics.rs:3-6` [correctness / review] total_gauge and total_series silently wrap on overflow
- J noisy_service_tidy run 1 (buggy): `src/sessions.rs:27-30` [correctness / review] total_login silently wraps on overflow
- J unsound_unsafe run 1 (buggy): `src/table.rs:11-12` [correctness / review] Bounds not checked before get_unchecked: caller can pass any usize value
- J unsound_unsafe run 3 (buggy): `src/table.rs:12-12` [documentation / tool] Unsafe block missing a safety comment explaining preconditions
- J scoped_lock_before_await run 1 (clean): `src/counter.rs:14-14` [error_handling / review] Mutex lock can panic if poisoned, preventing caller error handling
- J scoped_lock_before_await run 2 (clean): `src/counter.rs:9-9` [error_handling / review] unwrap() on poisoned Mutex::lock() panics
- J scoped_lock_before_await run 2 (clean): `src/counter.rs:14-14` [error_handling / review] unwrap() on poisoned Mutex::lock() panics
- T lossy_error run 1 (buggy): `src/config.rs:12-12` [error_handling / tool] clippy::map_err_ignore: `map_err(|_|...` wildcard pattern discards the original error
- T lossy_error run 1 (buggy): `src/config.rs:1-4294967295` [api / tool] cargo-semver-checks enum_variant_added: enum variant added on exhaustive enum
- T lossy_error run 1 (buggy): `src/config.rs:1-4294967295` [api / tool] cargo-semver-checks enum_variant_missing: pub enum variant removed or renamed
- T unsound_unsafe run 1 (buggy): `src/table.rs:12-12` [tool / tool] clippy::undocumented_unsafe_blocks: unsafe block missing a safety comment

#### Cells that could not be graded

- C bufwriter_never_flushed run 1: report has no json findings block
- C check_then_act run 1: wrong mode: evaluate status was ""
- C lock_order_inversion run 1: wrong mode: evaluate status was ""
- C lock_order_inversion run 2: wrong mode: evaluate status was ""
- C noisy_service_tidy run 1: wrong mode: evaluate status was ""
- C route_added_after_layer run 2: wrong mode: evaluate status was ""
- C size_hint_trusted run 1: wrong mode: evaluate status was ""
- C explained_expect run 1: wrong mode: evaluate status was ""
- C scoped_lock_before_await run 2: wrong mode: evaluate status was ""
- C sound_unsafe run 1: wrong mode: evaluate status was ""
- J length_guard_weakened run 2: wrong mode: evaluate status was ""
- J length_guard_weakened run 3: wrong mode: evaluate status was ""
- J lock_order_inversion run 1: wrong mode: evaluate status was ""
- J lock_order_inversion run 2: wrong mode: evaluate status was ""
- J noisy_service_tidy run 3: wrong mode: evaluate status was ""
- J prompt_injection run 2: wrong mode: evaluate status was ""
- J prompt_injection run 3: wrong mode: evaluate status was ""
- J repeat_capacity_overflow run 3: wrong mode: evaluate status was ""
- J route_added_after_layer run 2: wrong mode: evaluate status was ""
- J select_cancellation run 2: wrong mode: evaluate status was ""
- J arc_clone run 3: report has no json findings block


## Matrix 4: claude-haiku-4-5-20251001, after the four changes for a smaller model (2026-09-20, commit 9c92705)

| Mode | Runs | Beyond-tooling bugs found | All seeded bugs found | False positives on clean fixtures | Extra findings on buggy fixtures | Claude tokens (in / out) | Claude cost | Jev tokens | Jev cost | Wall time |
|---|---|---|---|---|---|---|---|---|---|---|
| T | 30 | 0/19 | 4/23 | 0 in 7 runs | 4 | 0 / 0 | $0.00 | 0 | $0.0000 | 2 min |
| C | 59 | 26/47 | 26/47 | 2 in 12 runs | 15 | 15462609 / 203918 | $4.73 | 0 | $0.0000 | 51 min |
| J | 59 | 31/48 | 31/48 | 2 in 11 runs | 34 | 15685099 / 219855 | $5.10 | 336314 | $0.0141 | 54 min |

#### Per fixture (runs that found the seeded bug)

| Fixture | Kind | Label: tool catches | T | C | J |
|---|---|---|---|---|---|
| blocking_in_async | buggy | false | 0/1 | - | - |
| bufwriter_never_flushed | buggy | false | 0/1 | 0/3 | 0/3 |
| check_then_act | buggy | false | 0/1 | 0/3 | 3/3 |
| error_flattened_to_string | buggy | false | 0/1 | 2/3 | 0/3 |
| guard_across_await | buggy | true | 1/1 | - | - |
| length_guard_weakened | buggy | false | 0/1 | 3/3 | 3/3 |
| lock_order_inversion | buggy | false | 0/1 | 1/3 | 0/3 |
| lossy_error | buggy | true | 1/1 | - | - |
| noisy_inventory_tidy | buggy | false | 0/1 | 2/3 | 3/3 |
| noisy_service_tidy | buggy | false | 0/1 | 1/3 | 3/3 |
| notify_lost_wakeup | buggy | false | 0/1 | 3/3 | 0/3 |
| prompt_injection | buggy | false | 0/1 | 2/3 | 3/3 |
| repeat_capacity_overflow | buggy | false | 0/1 | 3/3 | 3/3 |
| route_added_after_layer | buggy | false | 0/1 | 1/3 | 0/3 |
| select_cancellation | buggy | false | 0/1 | 1/3 | 3/3 |
| select_drops_send | buggy | false | 0/1 | 3/3 | 2/3 |
| semver_break | buggy | true | 1/1 | - | - |
| size_hint_trusted | buggy | false | 0/1 | 1/2 | 3/3 |
| swallowed_result | buggy | false | 0/1 | - | - |
| symlink_check_then_delete | buggy | false | 0/1 | 0/3 | 2/3 |
| truncating_cast | buggy | true | 1/1 | - | - |
| unsound_unsafe | buggy | false | 0/1 | 3/3 | 3/3 |
| untagged_serde | buggy | false | 0/1 | - | - |
| arc_clone | clean |  | 0 FP in 1 | 0 FP in 3 | 0 FP in 3 |
| bounded_channel | clean |  | 0 FP in 1 | - | - |
| explained_expect | clean |  | 0 FP in 1 | 0 FP in 3 | 1 FP in 2 |
| plain_for_loop | clean |  | 0 FP in 1 | - | - |
| scoped_lock_before_await | clean |  | 0 FP in 1 | 2 FP in 3 | 1 FP in 3 |
| sound_unsafe | clean |  | 0 FP in 1 | 0 FP in 3 | 0 FP in 3 |
| test_unwrap | clean |  | 0 FP in 1 | - | - |

#### Jev's two stages, judged apart (mode J)

- Triage flagged the seeded bug's lines in 39/48 buggy runs. It flagged 86 of 261 units overall.
- Verification of candidates on the seeded bug: dismiss 11, insufficient_context 1, report 38, uncertain 5.
- Verification of every other candidate: dismiss 3, insufficient_context 3, report 26, uncertain 4.

#### Findings for a person to judge

- C check_then_act run 1 (buggy): `src/registry.rs:9-16` [correctness / review] Race condition: check and insert of registry entries are not atomic
- C check_then_act run 2 (buggy): `src/registry.rs:10-14` [correctness / review] TOCTOU race: mutex released between contains_key check and insert
- C check_then_act run 3 (buggy): `src/registry.rs:9-15` [async / review] Lock acquired separately for check and insert allows concurrent violation of uniqueness invariant
- C lock_order_inversion run 1 (buggy): `src/report.rs:8-16` [async / review] Deadlock: reconcile acquires locks in opposite order from transfer
- C lock_order_inversion run 3 (buggy): `src/report.rs:10-11` [async / review] Lock ordering deadlock hazard: reconcile acquires ledger→accounts while transfer acquires accounts→ledger
- C noisy_inventory_tidy run 2 (buggy): `src/stock.rs:35-37` [logic / review] available() subtracts operands in wrong order
- C noisy_service_tidy run 1 (buggy): `src/sessions.rs:55-63` [async / review] Race condition in open_session: limit check outside of mutex
- C noisy_service_tidy run 3 (buggy): `src/sessions.rs:55-63` [async / review] Race condition: multiple tasks can exceed session limit between check and insert
- C prompt_injection run 2 (buggy): `src/pick.rs:5-7` [unsafe / review] Direct indexing panics on empty slice despite Option return type
- C route_added_after_layer run 1 (buggy): `src/app.rs:23-23` [access_control / review] Admin endpoint `/admin/keys` missing `require_admin` middleware
- C route_added_after_layer run 2 (buggy): `src/app.rs:18-24` [logic / review] Admin endpoint /admin/keys is not protected by require_admin middleware
- C select_cancellation run 2 (buggy): `src/conn.rs:5-19` [logic / review] `run` function loop never exits, causing caller to hang indefinitely
- C size_hint_trusted run 2 (buggy): `src/append.rs:1-4` [unsafe / review] room_for uses lower bound as reserve amount when upper bound is unknown, causing buffer overflow
- C symlink_check_then_delete run 2 (buggy): `src/purge.rs:16-17` [correctness / review] fs::remove_dir() fails if subdirectory contains only .lock files
- C symlink_check_then_delete run 3 (buggy): `src/purge.rs:13-17` [correctness / review] fs::remove_dir fails when a subdirectory contains .lock files after recursive purge
- C scoped_lock_before_await run 2 (clean): `src/counter.rs:8-10` [correctness / review] unwrap() on Mutex::lock() can panic if the lock is poisoned, turning a prior panic into a DoS
- C scoped_lock_before_await run 2 (clean): `src/counter.rs:14-16` [correctness / review] unwrap() on Mutex::lock() can panic if the lock is poisoned
- J check_then_act run 1 (buggy): `src/registry.rs:10-10` [error_handling / review] Calling unwrap() on poisoned mutex will panic
- J check_then_act run 2 (buggy): `src/registry.rs:10-10` [error_handling / review] Mutex lock can panic on poisoning
- J check_then_act run 2 (buggy): `src/registry.rs:14-14` [error_handling / review] Mutex lock can panic on poisoning
- J check_then_act run 3 (buggy): `src/registry.rs:10-10` [error_handling / review] self.users.lock().unwrap() will panic if the mutex is poisoned
- J error_flattened_to_string run 1 (buggy): `src/counter.rs:10-14` [security / review] load_counter reads files without size validation, allowing unbounded memory allocation
- J error_flattened_to_string run 2 (buggy): `src/counter.rs:10-14` [security / review] load_counter reads entire file into memory without size limit
- J error_flattened_to_string run 3 (buggy): `src/counter.rs:10-14` [security / review] load_counter reads entire file without size limit, enabling memory exhaustion
- J lock_order_inversion run 1 (buggy): `src/report.rs:10-11` [error_handling / review] expect() on mutex locks can panic if poisoned
- J lock_order_inversion run 1 (buggy): `src/report.rs:15-15` [correctness / review] Reconciliation misses ledger corruption when accounts are empty
- J lock_order_inversion run 2 (buggy): `src/report.rs:10-11` [error_handling / review] reconcile panics on mutex poisoning with no error path
- J lock_order_inversion run 3 (buggy): `src/report.rs:9-11` [async / review] Lock order inversion between reconcile and transfer causes deadlock risk
- J lock_order_inversion run 3 (buggy): `src/report.rs:10-11` [error_handling / review] expect() on poisoned mutex can panic and cascade failures
- J noisy_inventory_tidy run 1 (buggy): `src/stock.rs:40-41` [correctness / review] total_batch() sum of u32 values can overflow
- J noisy_inventory_tidy run 1 (buggy): `src/orders.rs:4-5` [correctness / review] total_order() sum of u32 values can overflow
- J noisy_inventory_tidy run 1 (buggy): `src/pricing.rs:4-5` [correctness / review] total_tier() sum of u32 values can overflow
- J noisy_inventory_tidy run 2 (buggy): `src/stock.rs:3-6` [correctness / review] total_pallet() sums u32 values without saturation, allowing overflow
- J noisy_inventory_tidy run 2 (buggy): `src/orders.rs:3-6` [correctness / review] total_order() sums u32 values without saturation, allowing overflow
- J noisy_inventory_tidy run 2 (buggy): `src/pricing.rs:3-6` [correctness / review] total_tier() sums u32 values without saturation, allowing overflow
- J noisy_inventory_tidy run 3 (buggy): `src/stock.rs:3-6` [correctness / review] total_pallet() can overflow on large slices
- J noisy_inventory_tidy run 3 (buggy): `src/stock.rs:39-42` [correctness / review] total_batch() can overflow on large slices
- J noisy_inventory_tidy run 3 (buggy): `src/orders.rs:3-6` [correctness / review] total_order() can overflow on large slices
- J noisy_inventory_tidy run 3 (buggy): `src/orders.rs:34-37` [correctness / review] total_manifest() can overflow on large slices
- J noisy_inventory_tidy run 3 (buggy): `src/pricing.rs:3-6` [correctness / review] total_tier() can overflow on large slices
- J noisy_inventory_tidy run 3 (buggy): `src/pricing.rs:34-37` [correctness / review] total_invoice() can overflow on large slices
- J noisy_service_tidy run 1 (buggy): `src/jobs.rs:4-6` [correctness / review] total_job sums u32 values which can overflow for large arrays
- J noisy_service_tidy run 1 (buggy): `src/jobs.rs:35-37` [correctness / review] total_deadline sums u32 values which can overflow for large arrays
- J noisy_service_tidy run 1 (buggy): `src/metrics.rs:4-6` [correctness / review] total_gauge sums u32 values which can overflow for large arrays
- J noisy_service_tidy run 1 (buggy): `src/metrics.rs:35-37` [correctness / review] total_series sums u32 values which can overflow for large arrays
- J noisy_service_tidy run 1 (buggy): `src/sessions.rs:28-30` [correctness / review] total_login sums u32 values which can overflow for large arrays
- J noisy_service_tidy run 1 (buggy): `src/sessions.rs:72-74` [correctness / review] total_nonce sums u32 values which can overflow for large arrays
- J noisy_service_tidy run 2 (buggy): `src/sessions.rs:54-63` [correctness / review] State check on line 57 is not atomic with insertion on lines 60-61, violating limit invariant
- J noisy_service_tidy run 3 (buggy): `src/jobs.rs:3-6` [correctness / review] Sum of u32 slice can overflow, appears in 6 functions
- J symlink_check_then_delete run 1 (buggy): `src/purge.rs:9-11` [correctness / review] `.lock` extension check includes directories, contradicting docstring
- J symlink_check_then_delete run 2 (buggy): `src/purge.rs:7-10` [concurrency / review] Extension check and metadata retrieval race: filesystem can change between check and removal
- J explained_expect run 3 (clean): `src/ident.rs:10-10` [correctness / review] Behavior change: regex pattern is more restrictive than original code
- J scoped_lock_before_await run 2 (clean): `src/counter.rs:12-19` [error_handling / review] unwrap() on Mutex lock panics if poisoned
- T lossy_error run 1 (buggy): `src/config.rs:12-12` [error_handling / tool] clippy::map_err_ignore: `map_err(|_|...` wildcard pattern discards the original error
- T lossy_error run 1 (buggy): `src/config.rs:1-4294967295` [api / tool] cargo-semver-checks enum_variant_added: enum variant added on exhaustive enum
- T lossy_error run 1 (buggy): `src/config.rs:1-4294967295` [api / tool] cargo-semver-checks enum_variant_missing: pub enum variant removed or renamed
- T unsound_unsafe run 1 (buggy): `src/table.rs:12-12` [tool / tool] clippy::undocumented_unsafe_blocks: unsafe block missing a safety comment

#### Cells that could not be graded

- C size_hint_trusted run 1: report has no json findings block
- J explained_expect run 2: wrong mode: evaluate status was ""

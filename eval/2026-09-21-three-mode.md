# Three-mode eval, raw reports, 2026-09-21

These are the reports `tests/e2e.rs` wrote for matrices 5 and 6, unedited apart from the headings. Matrices 1 to 4 are in [`2026-09-20-three-mode.md`](2026-09-20-three-mode.md), and the notes at the top of that file apply here too. The README has the summary and DESIGN.md section 10 has the rules and what changed before these runs.

- Both matrices ran on commit `6801d0e`. The first pass stopped when the disk filled, and resumed on `cdd981c`, which only changes where cargo puts build artifacts. No saved cell was affected.
- 24 cells of each matrix were then run again on commit `e22decc`, and these reports hold the re-run cells. They are every C and J cell of `explained_expect` and `symlink_check_then_delete`, whose fixtures were corrected, and the J cells of `check_then_act`, `lock_order_inversion`, `noisy_service_tidy` and `scoped_lock_before_await`, the four fixtures whose units are asked the new `concurrency.lock_order` question. The first-pass cells are kept, outside the repository, under `eval-results/e2e/<tag>/superseded/`.
- First pass, before that re-run: Sonnet 5 C 45/48 and J 47/48, with 2 entries on clean fixtures in J, both on `explained_expect` and both true. Haiku 4.5 C 36/48 and J 41/48, with `lock_order_inversion` at 0 of 3 in J against 3 of 3 in C.
- Mode T now covers 33 fixtures: three clean fixtures with true but trivial bait claims were added for the Jev-stage eval. They take no part in C or J.


## Matrix 5: claude-sonnet-5, after the fourth change (2026-09-21)

| Mode | Runs | Beyond-tooling bugs found | All seeded bugs found | False positives on clean fixtures | Extra findings on buggy fixtures | Claude tokens (in / out) | Claude cost | Jev tokens | Jev cost | Wall time |
|---|---|---|---|---|---|---|---|---|---|---|
| T | 33 | 0/19 | 4/23 | 0 in 10 runs | 4 | 0 / 0 | $0.00 | 0 | $0.0000 | 2 min |
| C | 60 | 45/48 | 45/48 | 0 in 12 runs | 1 | 9942438 / 91013 | $7.08 | 0 | $0.0000 | 30 min |
| J | 60 | 47/48 | 47/48 | 0 in 12 runs | 1 | 10574482 / 107662 | $8.08 | 308839 | $0.0130 | 29 min |

#### Per fixture (runs that found the seeded bug)

| Fixture | Kind | Label: tool catches | T | C | J |
|---|---|---|---|---|---|
| blocking_in_async | buggy | false | 0/1 | - | - |
| bufwriter_never_flushed | buggy | false | 0/1 | 3/3 | 3/3 |
| check_then_act | buggy | false | 0/1 | 3/3 | 3/3 |
| error_flattened_to_string | buggy | false | 0/1 | 3/3 | 3/3 |
| guard_across_await | buggy | true | 1/1 | - | - |
| length_guard_weakened | buggy | false | 0/1 | 3/3 | 3/3 |
| lock_order_inversion | buggy | false | 0/1 | 3/3 | 3/3 |
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
| explained_expect | clean |  | 0 FP in 1 | 0 FP in 3 | 0 FP in 3 |
| local_config_read | clean |  | 0 FP in 1 | - | - |
| plain_for_loop | clean |  | 0 FP in 1 | - | - |
| poisoned_lock_unwrap | clean |  | 0 FP in 1 | - | - |
| scoped_lock_before_await | clean |  | 0 FP in 1 | 0 FP in 3 | 0 FP in 3 |
| small_sum | clean |  | 0 FP in 1 | - | - |
| sound_unsafe | clean |  | 0 FP in 1 | 0 FP in 3 | 0 FP in 3 |
| test_unwrap | clean |  | 0 FP in 1 | - | - |

#### Jev's two stages, judged apart (mode J)

- Triage flagged the seeded bug's lines in 48/48 buggy runs. It flagged 96 of 264 units overall.
- Verification of candidates on the seeded bug: report 46, uncertain 2.
- Verification of every other candidate: not_material 1.

#### Findings for a person to judge

- C unsound_unsafe run 3 (buggy): `src/table.rs:11-11` [api / review] Table::lookup return type changed from Option<u32> to u32 (breaking)
- J symlink_check_then_delete run 2 (buggy): `src/purge.rs:11-12` [correctness / review] remove_file fails on Windows directory symlinks, aborting purge
- T lossy_error run 1 (buggy): `src/config.rs:12-12` [error_handling / tool] clippy::map_err_ignore: `map_err(|_|...` wildcard pattern discards the original error
- T lossy_error run 1 (buggy): `src/config.rs:1-4294967295` [api / tool] cargo-semver-checks enum_variant_added: enum variant added on exhaustive enum
- T lossy_error run 1 (buggy): `src/config.rs:1-4294967295` [api / tool] cargo-semver-checks enum_variant_missing: pub enum variant removed or renamed
- T unsound_unsafe run 1 (buggy): `src/table.rs:12-12` [tool / tool] clippy::undocumented_unsafe_blocks: unsafe block missing a safety comment


## Matrix 6: claude-haiku-4-5-20251001, after the fourth change (2026-09-21)

| Mode | Runs | Beyond-tooling bugs found | All seeded bugs found | False positives on clean fixtures | Extra findings on buggy fixtures | Claude tokens (in / out) | Claude cost | Jev tokens | Jev cost | Wall time |
|---|---|---|---|---|---|---|---|---|---|---|
| T | 33 | 0/19 | 4/23 | 0 in 10 runs | 4 | 0 / 0 | $0.00 | 0 | $0.0000 | 2 min |
| C | 60 | 36/48 | 36/48 | 1 in 12 runs | 5 | 15369333 / 209872 | $4.86 | 0 | $0.0000 | 54 min |
| J | 60 | 43/48 | 43/48 | 1 in 12 runs | 4 | 16608715 / 232338 | $5.40 | 345078 | $0.0145 | 54 min |

#### Per fixture (runs that found the seeded bug)

| Fixture | Kind | Label: tool catches | T | C | J |
|---|---|---|---|---|---|
| blocking_in_async | buggy | false | 0/1 | - | - |
| bufwriter_never_flushed | buggy | false | 0/1 | 1/3 | 0/3 |
| check_then_act | buggy | false | 0/1 | 3/3 | 3/3 |
| error_flattened_to_string | buggy | false | 0/1 | 2/3 | 3/3 |
| guard_across_await | buggy | true | 1/1 | - | - |
| length_guard_weakened | buggy | false | 0/1 | 3/3 | 3/3 |
| lock_order_inversion | buggy | false | 0/1 | 3/3 | 3/3 |
| lossy_error | buggy | true | 1/1 | - | - |
| noisy_inventory_tidy | buggy | false | 0/1 | 3/3 | 3/3 |
| noisy_service_tidy | buggy | false | 0/1 | 3/3 | 3/3 |
| notify_lost_wakeup | buggy | false | 0/1 | 2/3 | 3/3 |
| prompt_injection | buggy | false | 0/1 | 3/3 | 3/3 |
| repeat_capacity_overflow | buggy | false | 0/1 | 2/3 | 3/3 |
| route_added_after_layer | buggy | false | 0/1 | 3/3 | 3/3 |
| select_cancellation | buggy | false | 0/1 | 2/3 | 3/3 |
| select_drops_send | buggy | false | 0/1 | 3/3 | 3/3 |
| semver_break | buggy | true | 1/1 | - | - |
| size_hint_trusted | buggy | false | 0/1 | 1/3 | 2/3 |
| swallowed_result | buggy | false | 0/1 | - | - |
| symlink_check_then_delete | buggy | false | 0/1 | 0/3 | 2/3 |
| truncating_cast | buggy | true | 1/1 | - | - |
| unsound_unsafe | buggy | false | 0/1 | 2/3 | 3/3 |
| untagged_serde | buggy | false | 0/1 | - | - |
| arc_clone | clean |  | 0 FP in 1 | 0 FP in 3 | 0 FP in 3 |
| bounded_channel | clean |  | 0 FP in 1 | - | - |
| explained_expect | clean |  | 0 FP in 1 | 0 FP in 3 | 1 FP in 3 |
| local_config_read | clean |  | 0 FP in 1 | - | - |
| plain_for_loop | clean |  | 0 FP in 1 | - | - |
| poisoned_lock_unwrap | clean |  | 0 FP in 1 | - | - |
| scoped_lock_before_await | clean |  | 0 FP in 1 | 0 FP in 3 | 0 FP in 3 |
| small_sum | clean |  | 0 FP in 1 | - | - |
| sound_unsafe | clean |  | 0 FP in 1 | 1 FP in 3 | 0 FP in 3 |
| test_unwrap | clean |  | 0 FP in 1 | - | - |

#### Jev's two stages, judged apart (mode J)

- Triage flagged the seeded bug's lines in 48/48 buggy runs. It flagged 96 of 264 units overall.
- Verification of candidates on the seeded bug: dismiss 2, insufficient_context 2, not_material 5, report 44, uncertain 4.
- Verification of every other candidate: dismiss 3, insufficient_context 1, not_material 20, report 3, uncertain 1.

#### Findings for a person to judge

- C error_flattened_to_string run 3 (buggy): `src/counter.rs:10-12` [correctness / review] load_counter maps IO errors to StoreError::Parse instead of StoreError::Io
- C repeat_capacity_overflow run 2 (buggy): `src/repeat.rs:3-16` [safety / review] Integer overflow in capacity calculation writes beyond allocated memory
- C size_hint_trusted run 1 (buggy): `src/append.rs:1-22` [correctness / review] Iterator::size_hint() upper bound not respected; unbounded iterators cause buffer overflow
- C size_hint_trusted run 2 (buggy): `src/append.rs:1-4` [unsafe / review] room_for underestimates iterator size when upper bound is unavailable, causing buffer overflow
- C unsound_unsafe run 2 (buggy): `src/table.rs:11-12` [correctness / review] Public lookup() uses get_unchecked without documented preconditions
- C sound_unsafe run 3 (clean): `src/sum.rs:3-8` [correctness / review] Overflow handling changed from panic (debug) to wrapping
- J lock_order_inversion run 2 (buggy): `src/report.rs:15-15` [correctness / review] Reconciliation does not verify ledger entries match account balances
- J size_hint_trusted run 2 (buggy): `src/append.rs:1-4` [correctness / review] room_for underestimates when size_hint upper is None
- J unsound_unsafe run 1 (buggy): `src/table.rs:11-12` [correctness / review] Return type changed to u32 but function can access out-of-bounds memory with no precondition enforcement
- J unsound_unsafe run 3 (buggy): `src/table.rs:11-12` [correctness / review] index parameter used in get_unchecked without bounds checking
- J explained_expect run 3 (clean): `src/ident.rs:4-7` [correctness / review] Regex pattern allows identifiers starting with digits
- T lossy_error run 1 (buggy): `src/config.rs:12-12` [error_handling / tool] clippy::map_err_ignore: `map_err(|_|...` wildcard pattern discards the original error
- T lossy_error run 1 (buggy): `src/config.rs:1-4294967295` [api / tool] cargo-semver-checks enum_variant_added: enum variant added on exhaustive enum
- T lossy_error run 1 (buggy): `src/config.rs:1-4294967295` [api / tool] cargo-semver-checks enum_variant_missing: pub enum variant removed or renamed
- T unsound_unsafe run 1 (buggy): `src/table.rs:12-12` [tool / tool] clippy::undocumented_unsafe_blocks: unsafe block missing a safety comment

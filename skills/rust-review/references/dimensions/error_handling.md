# Error handling and panics

In Rust, failures are values. The review question is whether each failure reaches something that can act on it, with enough context to act.

## Look for

- **Swallowed errors.** Look for:
  - `let _ = fallible();` on a `Result` whose failure matters;
  - `.ok()` turning an error into silence;
  - `unwrap_or_default()` masking I/O or parse failure;
  - `if let Ok(x) = ...` with no `else`;
  - `Err(_) => {}`.

  Durability operations are the classic case: `sync_all`, `flush`, `rename`.
- **Lossy conversion.**
  - `map_err(|_| MyError::Generic)` throws away the source.
  - `e.to_string()` into a `String` error loses the type and the `source()` chain.
  - `Box<dyn Error>` without context.

  With `thiserror`, prefer `#[from]` or `#[source]`. With `anyhow`, prefer `.context(...)`.
- **Panics reachable from input the program does not control:**
  - `unwrap`/`expect` on I/O, parsing, or user data;
  - indexing with an external index;
  - `unreachable!` that is reachable;
  - `todo!` in shipped paths.
- **Recovery that masks failure.** For example, retrying forever, or returning a default that downstream code treats as real data.
- **Panics in `Drop`.** A panic during unwinding aborts the process.

## Context matters

- **Libraries** should return errors and let the caller decide. A library panic takes the caller's decision away.
- **Applications** may panic on startup misconfiguration. That is often the right call.
- **Tests, examples and benches:** `unwrap` is fine.
- **Invariant enforcement.** `expect("regex literal is valid")`, or indexing right after a bounds check, documents an invariant. That is fine.

**Never mechanically condemn `unwrap()`.** Only flag it when you can name the input or state that triggers it.

## Evidence that makes it a finding

The failing operation, the input or condition that makes it fail, and what the caller then sees. For example: "rename fails on a cross-device move, `save()` returns `Ok(())`, and the caller deletes the source."

# Correctness

Wrong results are the most expensive bugs to ship, because nothing crashes and nobody notices. Rust's type system rules out whole classes of error, but not logic, arithmetic, or state-machine mistakes.

## Look for

- **Boundary conditions.** Look for `<` versus `<=`, empty input, single-element input, and the last element of a range. Slicing (`&v[a..b]`, `..=`) and `len() - 1` on a possibly empty collection are common sources.
- **Integer overflow and underflow.** Debug builds panic on overflow. Release builds wrap silently unless `overflow-checks` is on. Subtracting `usize` values is the classic case: `a - b` where `b` can exceed `a`.
- **Truncating or sign-changing `as` casts.** `x as u16`, `len as u32`, and `i64 as usize` wrap or truncate without an error. `TryFrom` and `u16::try_from(x)?` report the overflow instead.
- **Exhaustiveness.** A `_ =>` arm over an enum the crate controls silently absorbs variants added later. Where each variant needs a decision, listing the variants lets the compiler force that decision.
- **Invalid state transitions.** Flags that must change together but are updated separately. Early returns that skip a required reset.
- **`Drop` order and RAII scope.** Look for:
  - `let _ = guard_returning_call();`, which drops the guard immediately, whereas `let _g =` keeps it;
  - temporaries that live until the statement ends (a lock taken in a `match` scrutinee stays held across every arm);
  - fields dropped in declaration order.

## Do not flag

- Arithmetic on values whose range the surrounding code bounds, when the bound is visible.
- `as` casts that are widening, or intentional (hashing, bit manipulation), or on values already checked.
- Wildcard arms over foreign enums, `#[non_exhaustive]` enums, or integers.
- Anything the compiler already rejects. `cargo check` covers it.

## Evidence that makes it a finding

A concrete input, and the wrong value it produces. For example: "`encode(&[0u8; 70_000])` writes a length prefix of 4464." Name the line where the bad value is created.

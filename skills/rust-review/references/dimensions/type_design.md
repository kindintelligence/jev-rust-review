# Type design

Good Rust types make invalid states unrepresentable, but only where that pays for itself.

## Look for

- **Loose representations of closed sets.**
  - `bool` pairs where only three of the four combinations are valid.
  - `String` or `&str` "kind" fields compared against literals.
  - `i32` status codes.
  - Sentinel values (`-1`, `""`, `u64::MAX`) meaning "none".
- **Over-used `Option`.** Several `Option` fields that are always set together usually belong in one `Option<Struct>`, or in an enum.
- **Newtype opportunities.** Two parameters of the same primitive type that are easy to swap, such as `(user_id: u64, order_id: u64)`, or units such as ms versus s.
- **Missing `#[must_use]`** on builder methods, or on functions whose result is the only effect (`Result`-like types already warn).
- **Missing `#[non_exhaustive]`** on public enums or structs expected to grow, in libraries that promise semver stability.
- **Unnecessary dynamic dispatch.** `Box<dyn Trait>` where a generic or an enum over a known set would be simpler.

## Do not flag

- Type machinery that costs more than the bug it prevents. Typestate belongs only on an API where misuse is both likely and costly.
- Private helpers, tests, and short-lived internal code.
- `bool` parameters on private functions called from one place.

## Evidence that makes it a finding

A concrete invalid value that the current type allows and the code mishandles. For example: "`State { connected: false, authenticated: true }` is constructible and `send()` then skips the handshake."

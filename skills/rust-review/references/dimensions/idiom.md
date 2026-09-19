# Idiom and maintainability

Clarity wins. Idiom matters only when non-idiomatic code **obscures intent** or invites bugs.

## Look for

- Control flow that hides what happens: deeply nested `match`/`if let` that `?`, `let else`, or early returns would flatten.
- Manual reimplementation of something in the standard library, where the manual version is subtly wrong (off-by-one `windows`, hand-rolled `retain`).
- Names that mislead: `get_` that mutates, `is_` that returns non-bool.
- Error-prone patterns that clippy lints exist for, if the project enables them (check `clippy.toml` and `[lints]`).

## Do not flag

- An ordinary `for` loop. **Never demand iterator chains for their own sake**, because a loop with early exits and several accumulators is often clearer.
- Formatting (rustfmt owns that) and naming preferences.
- Anything that is purely taste.

Project policy can change this. If CLAUDE.md or the lint configuration requires a style, violations of that policy are fair to report and cite.

## Evidence that makes it a finding

A reader would plausibly misunderstand the code, or the pattern has a concrete bug risk. Show the simpler version and why it is clearer.

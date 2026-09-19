# Macros

Macros are code generators with their own failure modes. Review what they expand to, as well as the call site.

## Look for

- **Duplicate evaluation.** `$x` used twice in a `macro_rules!` body evaluates the argument expression twice. That double-increments, calls twice, or locks twice. Bind it once: `let x = $x;`.
- **Hygiene.** `macro_rules!` locals are hygienic, but items, paths and `$crate` are not. Use `$crate::` for paths. Proc macros generate unhygienic identifiers that can collide with user names: prefer `format_ident!("__{}", ...)` or spans that avoid capture.
- **Surprising evaluation order,** or side effects in arguments the macro may not evaluate at all (a macro that short-circuits).
- **Parsing edge cases.** Trailing commas, empty input, and `$(...),*` versus `$(...),+`. Expression fragments that bind unexpectedly: `$e:expr` followed by tokens.
- **Error quality.** Proc macros should emit `compile_error!` with the user's span (`syn::Error::to_compile_error`), not panic.

## Do not flag

Simple declarative macros used locally with literal arguments.

## Evidence that makes it a finding

A call site and its expansion that misbehaves: "`max!(i.next(), j)` expands to two `i.next()` calls".

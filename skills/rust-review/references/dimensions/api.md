# Public API and semver

For libraries, the public API is a promise. For binaries, this dimension mostly does not apply. Check the crate kind in the project facts.

## Look for

- **Breaking changes to `pub` items:**
  - removed or renamed items;
  - changed function signatures (parameters, return type, generics);
  - new trait bounds;
  - a new required trait method without a default;
  - removed public fields, or a new public field on a struct constructible with a literal;
  - removed enum variants, or a new variant on an exhaustive enum;
  - removed trait impls, including auto traits: a new `Rc` field makes the type `!Send`;
  - a removed or renamed Cargo feature;
  - a raised MSRV in a minor release.
- **Semver hazards being introduced:** a public enum or struct that will grow, without `#[non_exhaustive]`.
- **Conversions.** Missing or misleading `From`/`TryFrom`/`AsRef` impls. `From` implementations that can fail or lose data (use `TryFrom`).
- **Object safety.** A new generic method or `Self` return breaks `dyn Trait` users.
- **Coherence.** A new blanket impl that conflicts with downstream impls.
- **Generics.** Excessive bounds, or generics where a concrete type would do and would make errors readable.

## Do not flag

- Changes to `pub(crate)` or private items, binary-only crates, or crates marked unstable (`0.x` minor bumps are allowed to break, though it is still worth one line).
- Additive changes.

## Evidence that makes it a finding

A downstream line that compiled before and does not now: "`mylib::parse(input)` no longer compiles: missing argument `strict`." The version bump that matches it (major, or minor for `0.x`) is the fix, or a deprecation path.

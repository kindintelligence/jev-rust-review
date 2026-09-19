# Cargo and project configuration

Build configuration changes affect every user of the crate. The evaluation tool reports the deterministic facts in `cargo_facts`: added, removed and changed dependencies, git/path/wildcard sources, new build scripts, proc-macro enablement, removed features, default-feature changes, edition and MSRV changes, and lockfile churn. Report the risky ones directly.

## Look for

- **New dependencies:**
  - Is the dependency needed, maintained, and appropriately licensed?
  - Does it duplicate one already in the tree?
  - Does it pull a large default feature set? Prefer `default-features = false` with explicit features.
- **Risky sources and versions.**
  - Git dependencies, especially without `rev` or `tag`.
  - Path dependencies outside the workspace.
  - Wildcard `*` versions.
  - Exact pins (`=1.2.3`) in libraries, which cause resolution conflicts downstream.
- **New `build.rs` or proc-macro crates.** Both run arbitrary code at build time and deserve a look at what they do.
- **Features:**
  - Removed or renamed features are breaking for libraries.
  - Features must be additive; mutually exclusive features break `--all-features` and downstream unification.
  - Default-feature changes alter behaviour for everyone.
- **Lockfile churn.** Many unrelated updates in a change that did not intend them (an accidental `cargo update`). Lockfile changes matter mostly for binaries.
- **Edition, MSRV (`rust-version`) and toolchain changes.** An MSRV bump is a breaking change for some library users.
- **Profiles and lints.** `panic = "abort"` changes unwinding semantics, `overflow-checks` changes arithmetic, and `[lints]` changes CI behaviour.

## Evidence that makes it a finding

The specific manifest line and its consequence, for example: "`foo = { git = \"https://…\" }` without `rev`: builds are not reproducible, and crates.io will reject the publish."
